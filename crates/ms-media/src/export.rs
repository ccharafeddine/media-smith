//! Single-track export: render the first video track of a project to one file
//! with an output preset. Clips are concatenated in timeline order (gaps are
//! closed up in this phase), each scaled to fit and letterboxed to the preset
//! canvas. Pure argument building; the shell runs ffmpeg.

use std::collections::HashMap;

use ms_project::{ItemKind, Project, TrackKind};

use crate::encoders::{QualityTier, VideoEncoder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    /// 1080x1920: TikTok, Reels, Shorts.
    Vertical,
    /// 1920x1080: YouTube.
    Landscape,
    /// 1080x1080.
    Square,
    /// 480 px wide palette GIF at 15 fps, square canvas.
    Gif,
}

impl Preset {
    pub fn size(self) -> (u32, u32) {
        match self {
            Self::Vertical => (1080, 1920),
            Self::Landscape => (1920, 1080),
            Self::Square => (1080, 1080),
            Self::Gif => (480, 480),
        }
    }

    pub fn extension(self) -> &'static str {
        if self == Self::Gif {
            "gif"
        } else {
            "mp4"
        }
    }

    fn fps(self, project_fps: u32) -> u32 {
        if self == Self::Gif {
            15
        } else {
            project_fps.clamp(1, 120)
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExportError {
    #[error("the main video track has no clips to export")]
    Empty,
    #[error("no probe info for source {0}")]
    UnknownSource(String),
    #[error("refusing flag-like path: {0}")]
    FlagLike(String),
}

/// What export needs to know about each source file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceInfo {
    pub has_audio: bool,
}

fn secs(ms: u64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

/// Extra render inputs beyond the main track.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// ASS subtitle file already escaped for a filtergraph (see
    /// [`escape_filter_path`]); burned in after scaling. `None` = no overlays.
    pub subtitles: Option<String>,
}

/// Escape a file path for use as a filter option value: forward slashes,
/// `:` and `'` escaped, wrapped in single quotes (works for `C:\...`).
pub fn escape_filter_path(path: &str) -> String {
    let p = path
        .replace('\\', "/")
        .replace('\'', r"\'")
        .replace(':', r"\:");
    format!("'{p}'")
}

/// Length of the rendered timeline: the end of the last main-track clip.
pub fn export_duration_ms(project: &Project) -> u64 {
    project
        .main_video_clips()
        .iter()
        .map(|i| i.end_ms())
        .max()
        .unwrap_or(0)
}

/// Build the ffmpeg argument list for exporting `project` to `output`.
///
/// Timeline time is preserved: gaps on the main track render as black (and
/// silence), so caption/text overlays and audio-track clips stay in sync.
pub fn export_args(
    project: &Project,
    preset: Preset,
    sources: &HashMap<String, SourceInfo>,
    encoder: VideoEncoder,
    tier: QualityTier,
    opts: &ExportOptions,
    output: &str,
) -> Result<Vec<String>, ExportError> {
    if output.starts_with('-') {
        return Err(ExportError::FlagLike(output.into()));
    }
    let clips = project.main_video_clips();
    if clips.is_empty() {
        return Err(ExportError::Empty);
    }
    let (w, h) = preset.size();
    let fps = preset.fps(project.canvas.fps);
    let gif = preset == Preset::Gif;

    let mut args: Vec<String> = vec!["-y".into(), "-hide_banner".into()];
    let mut graph = Vec::new();
    let mut concat_in = String::new();
    let mut segments = 0usize;
    let mut input = 0usize;
    let mut cursor = 0u64;
    let mut push_input = |args: &mut Vec<String>,
                          source: &str,
                          in_ms: u64,
                          dur_ms: u64|
     -> Result<usize, ExportError> {
        if source.starts_with('-') {
            return Err(ExportError::FlagLike(source.into()));
        }
        args.extend([
            "-ss".into(),
            secs(in_ms),
            "-t".into(),
            secs(dur_ms),
            "-i".into(),
            source.into(),
        ]);
        input += 1;
        Ok(input - 1)
    };

    for it in &clips {
        let ItemKind::Clip {
            source,
            source_in_ms,
            volume,
        } = &it.kind
        else {
            unreachable!("main_video_clips only returns clips")
        };
        if it.start_ms > cursor {
            let gap = secs(it.start_ms - cursor);
            let k = segments;
            graph.push(format!(
                "color=c=black:s={w}x{h}:r={fps}:d={gap},format=yuv420p,setsar=1[v{k}]"
            ));
            if !gif {
                graph.push(format!(
                    "anullsrc=r=48000:cl=stereo,atrim=duration={gap}[a{k}]"
                ));
                concat_in.push_str(&format!("[v{k}][a{k}]"));
            } else {
                concat_in.push_str(&format!("[v{k}]"));
            }
            segments += 1;
        }
        let info = sources
            .get(source)
            .ok_or_else(|| ExportError::UnknownSource(source.clone()))?;
        let i = push_input(&mut args, source, *source_in_ms, it.duration_ms)?;
        let k = segments;
        graph.push(format!(
            "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps},format=yuv420p[v{k}]"
        ));
        if !gif {
            if info.has_audio {
                graph.push(format!(
                    "[{i}:a]aresample=48000,volume={volume},aformat=channel_layouts=stereo[a{k}]"
                ));
            } else {
                graph.push(format!(
                    "anullsrc=r=48000:cl=stereo,atrim=duration={}[a{k}]",
                    secs(it.duration_ms)
                ));
            }
            concat_in.push_str(&format!("[v{k}][a{k}]"));
        } else {
            concat_in.push_str(&format!("[v{k}]"));
        }
        segments += 1;
        cursor = it.end_ms();
    }

    let n = segments;
    let sub = |from: &str, to: &str| match &opts.subtitles {
        Some(f) => format!("[{from}]subtitles=filename={f}[{to}]"),
        None => format!("[{from}]null[{to}]"),
    };
    if gif {
        graph.push(format!("{concat_in}concat=n={n}:v=1:a=0[cv0]"));
        graph.push(sub("cv0", "cv"));
        graph.push("[cv]split[g1][g2];[g1]palettegen=stats_mode=diff[p];[g2][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle[out]".into());
        args.extend([
            "-filter_complex".into(),
            graph.join(";"),
            "-map".into(),
            "[out]".into(),
            "-loop".into(),
            "0".into(),
        ]);
    } else {
        graph.push(format!("{concat_in}concat=n={n}:v=1:a=1[cv0][ma]"));
        graph.push(sub("cv0", "outv"));
        // Audio-track clips: delayed to their timeline start and mixed under
        // the main track's audio (which sets the output length).
        let mut mix = vec!["[ma]".to_string()];
        for t in project
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio && !t.muted)
        {
            for it in &t.items {
                let ItemKind::Clip {
                    source,
                    source_in_ms,
                    volume,
                } = &it.kind
                else {
                    continue;
                };
                let info = sources
                    .get(source)
                    .ok_or_else(|| ExportError::UnknownSource(source.clone()))?;
                if !info.has_audio || it.start_ms >= cursor {
                    continue;
                }
                let i = push_input(&mut args, source, *source_in_ms, it.duration_ms)?;
                let d = it.start_ms;
                graph.push(format!(
                    "[{i}:a]aresample=48000,volume={volume},aformat=channel_layouts=stereo,adelay={d}|{d}[m{i}]"
                ));
                mix.push(format!("[m{i}]"));
            }
        }
        if mix.len() > 1 {
            graph.push(format!(
                "{}amix=inputs={}:duration=first:normalize=0[outa]",
                mix.concat(),
                mix.len()
            ));
        } else {
            graph.push("[ma]anull[outa]".into());
        }
        args.extend([
            "-filter_complex".into(),
            graph.join(";"),
            "-map".into(),
            "[outv]".into(),
            "-map".into(),
            "[outa]".into(),
        ]);
        args.extend(encoder.video_args(tier));
        args.extend(
            ["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"]
                .iter()
                .map(|s| (*s).to_string()),
        );
    }
    args.push(output.into());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ms_project::TextStyle;

    fn project() -> (Project, HashMap<String, SourceInfo>) {
        let mut p = Project::new("t", 1080, 1920, 30);
        let v = p.add_track(TrackKind::Video, "V1");
        let c = p.add_track(TrackKind::Caption, "C");
        p.add_item(
            &v,
            0,
            2000,
            ItemKind::Clip {
                source: "a.mp4".into(),
                source_in_ms: 1500,
                volume: 1.0,
            },
        )
        .unwrap();
        p.add_item(
            &v,
            2000,
            1000,
            ItemKind::Clip {
                source: "b.mov".into(),
                source_in_ms: 0,
                volume: 0.5,
            },
        )
        .unwrap();
        p.add_item(
            &c,
            0,
            500,
            ItemKind::Caption {
                text: "hi".into(),
                style: TextStyle::default(),
            },
        )
        .unwrap();
        let mut s = HashMap::new();
        s.insert("a.mp4".to_string(), SourceInfo { has_audio: true });
        s.insert("b.mov".to_string(), SourceInfo { has_audio: false });
        (p, s)
    }

    fn graph(a: &[String]) -> &str {
        let i = a.iter().position(|x| x == "-filter_complex").unwrap();
        &a[i + 1]
    }

    #[test]
    fn presets_have_expected_sizes() {
        assert_eq!(Preset::Vertical.size(), (1080, 1920));
        assert_eq!(Preset::Landscape.size(), (1920, 1080));
        assert_eq!(Preset::Square.size(), (1080, 1080));
        assert_eq!(Preset::Gif.extension(), "gif");
        assert_eq!(Preset::Square.extension(), "mp4");
    }

    #[test]
    fn mp4_export_trims_scales_and_concats_with_audio() {
        let (p, s) = project();
        let a = export_args(
            &p,
            Preset::Landscape,
            &s,
            VideoEncoder::X264,
            QualityTier::High,
            &ExportOptions::default(),
            "out.mp4",
        )
        .unwrap();
        let joined = a.join(" ");
        assert!(joined.contains("-ss 1.500 -t 2.000 -i a.mp4"));
        assert!(joined.contains("-ss 0.000 -t 1.000 -i b.mov"));
        let g = graph(&a);
        assert!(g.contains("scale=1920:1080:force_original_aspect_ratio=decrease"));
        assert!(
            !g.contains("[1:a]"),
            "b.mov has no audio, so silence is generated"
        );
        assert!(g.contains("anullsrc"));
        assert!(!g.contains("volume=0.5"));
        assert!(g.contains("concat=n=2:v=1:a=1"));
        assert!(a.contains(&"libx264".to_string()));
        assert_eq!(a.last().unwrap(), "out.mp4");
    }

    #[test]
    fn gif_export_uses_palette_and_no_audio() {
        let (p, s) = project();
        let a = export_args(
            &p,
            Preset::Gif,
            &s,
            VideoEncoder::X264,
            QualityTier::High,
            &ExportOptions::default(),
            "out.gif",
        )
        .unwrap();
        let g = graph(&a);
        assert!(g.contains("fps=15"));
        assert!(g.contains("palettegen") && g.contains("paletteuse"));
        assert!(g.contains("concat=n=2:v=1:a=0"));
        assert!(!a.contains(&"aac".to_string()));
    }

    #[test]
    fn rejects_empty_unknown_and_flaglike() {
        let (p, s) = project();
        let e = |p: &Project, s: &HashMap<String, SourceInfo>, out: &str| {
            export_args(
                p,
                Preset::Square,
                s,
                VideoEncoder::X264,
                QualityTier::High,
                &ExportOptions::default(),
                out,
            )
        };
        assert_eq!(e(&p, &s, "-o"), Err(ExportError::FlagLike("-o".into())));
        assert_eq!(
            e(&p, &HashMap::new(), "o.mp4"),
            Err(ExportError::UnknownSource("a.mp4".into()))
        );
        let empty = Project::new("e", 1, 1, 30);
        assert_eq!(e(&empty, &s, "o.mp4"), Err(ExportError::Empty));
    }

    /// Real render through ffmpeg. Runs only when MS_FFMPEG points at an
    /// ffmpeg binary (CI and dev boxes without one skip it).
    #[test]
    fn renders_real_files_when_ffmpeg_available() {
        let Ok(ff) = std::env::var("MS_FFMPEG") else {
            return;
        };
        let ff = std::path::PathBuf::from(ff);
        let dir = std::env::temp_dir().join(format!("ms-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mp4").to_string_lossy().into_owned();
        let mk: Vec<String> = [
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x360:rate=30:duration=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            &src,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        crate::runner::run_ffmpeg(&ff, &mk).unwrap();
        let mut p = Project::new("r", 1080, 1920, 30);
        let v = p.add_track(TrackKind::Video, "V1");
        p.add_item(
            &v,
            0,
            1000,
            ItemKind::Clip {
                source: src.clone(),
                source_in_ms: 500,
                volume: 1.0,
            },
        )
        .unwrap();
        p.add_item(
            &v,
            1500,
            1000,
            ItemKind::Clip {
                source: src.clone(),
                source_in_ms: 1500,
                volume: 1.0,
            },
        )
        .unwrap();
        // Overlays and an audio-track clip, to exercise subtitles + amix.
        let c = p.add_track(TrackKind::Caption, "C");
        p.add_item(
            &c,
            200,
            1500,
            ItemKind::Caption {
                text: "Hello {world}".into(),
                style: ms_project::TextStyle::default(),
            },
        )
        .unwrap();
        let t = p.add_track(TrackKind::Text, "T");
        p.add_item(
            &t,
            0,
            2500,
            ItemKind::Text {
                text: "TITLE".into(),
                style: ms_project::TextStyle {
                    y: 0.1,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let au = p.add_track(TrackKind::Audio, "A");
        p.add_item(
            &au,
            300,
            1000,
            ItemKind::Clip {
                source: src.clone(),
                source_in_ms: 0,
                volume: 0.5,
            },
        )
        .unwrap();
        let mut s = HashMap::new();
        s.insert(src.clone(), SourceInfo { has_audio: true });
        for preset in [
            Preset::Vertical,
            Preset::Landscape,
            Preset::Square,
            Preset::Gif,
        ] {
            let out = dir
                .join(format!("out-{preset:?}.{}", preset.extension()))
                .to_string_lossy()
                .into_owned();
            let ass_path = dir.join(format!("subs-{preset:?}.ass"));
            std::fs::write(&ass_path, crate::ass::build_ass(&p, preset.size())).unwrap();
            let opts = ExportOptions {
                subtitles: Some(escape_filter_path(&ass_path.to_string_lossy())),
            };
            let a = export_args(
                &p,
                preset,
                &s,
                VideoEncoder::X264,
                QualityTier::Draft,
                &opts,
                &out,
            )
            .unwrap();
            crate::runner::run_ffmpeg(&ff, &a).unwrap_or_else(|e| panic!("{preset:?}: {e}"));
            let ffprobe = std::env::var("MS_FFPROBE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    ff.with_file_name(if cfg!(windows) {
                        "ffprobe.exe"
                    } else {
                        "ffprobe"
                    })
                });
            if ffprobe.exists() {
                let json =
                    crate::runner::run_tool(&ffprobe, &crate::runner::ffprobe_args(&out).unwrap())
                        .unwrap();
                let m = crate::probe::parse_meta(&json, preset.extension()).unwrap();
                assert_eq!((m.width, m.height), preset.size(), "{preset:?}");
                assert!(
                    (m.duration_secs - 2.5).abs() < 0.3,
                    "{preset:?} duration {}",
                    m.duration_secs
                );
                assert_eq!(m.has_audio, preset != Preset::Gif);
            } else {
                assert!(std::fs::metadata(&out).unwrap().len() > 1000);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gaps_render_black_and_audio_track_is_mixed() {
        let (mut p, mut s) = project();
        let v = p.tracks[0].id.clone();
        let b = p.tracks[0].items[1].id.clone();
        p.move_item(&b, 3000, Some(&v)).unwrap();
        let au = p.add_track(TrackKind::Audio, "Music");
        p.add_item(
            &au,
            500,
            2000,
            ItemKind::Clip {
                source: "m.mp3".into(),
                source_in_ms: 0,
                volume: 0.8,
            },
        )
        .unwrap();
        s.insert("m.mp3".into(), SourceInfo { has_audio: true });
        let opts = ExportOptions {
            subtitles: Some(escape_filter_path(r"C:\tmp\s.ass")),
        };
        let a = export_args(
            &p,
            Preset::Vertical,
            &s,
            VideoEncoder::X264,
            QualityTier::High,
            &opts,
            "o.mp4",
        )
        .unwrap();
        let g = graph(&a);
        assert!(g.contains("color=c=black:s=1080x1920:r=30:d=1.000"), "{g}");
        assert!(g.contains("concat=n=3:v=1:a=1"));
        assert!(g.contains("adelay=500|500"));
        assert!(g.contains("amix=inputs=2:duration=first:normalize=0[outa]"));
        assert!(g.contains(r"subtitles=filename='C\:/tmp/s.ass'"), "{g}");
        assert!(a.join(" ").contains("-i m.mp3"));
        assert_eq!(export_duration_ms(&p), 4000);
    }

    #[test]
    fn muted_audio_tracks_are_skipped() {
        let (mut p, mut s) = project();
        let au = p.add_track(TrackKind::Audio, "Music");
        p.add_item(
            &au,
            0,
            1000,
            ItemKind::Clip {
                source: "m.mp3".into(),
                source_in_ms: 0,
                volume: 1.0,
            },
        )
        .unwrap();
        p.tracks.last_mut().unwrap().muted = true;
        s.insert("m.mp3".into(), SourceInfo { has_audio: true });
        let a = export_args(
            &p,
            Preset::Square,
            &s,
            VideoEncoder::X264,
            QualityTier::High,
            &ExportOptions::default(),
            "o.mp4",
        )
        .unwrap();
        assert!(graph(&a).contains("[ma]anull[outa]"));
        assert!(!a.join(" ").contains("m.mp3"));
    }
}
