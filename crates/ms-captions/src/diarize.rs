//! Speaker diarization (ported from CaptionSmith's diarize.rs). whisper.cpp
//! doesn't know who is speaking, so "who spoke when" comes from the
//! `sherpa-onnx-offline-speaker-diarization` sidecar (pyannote segmentation +
//! speaker embedding + clustering). Its two ONNX models are downloaded on
//! first use, SHA-256 verified, into a models folder.
//!
//! The engine-agnostic pieces (parsing speaker turns, aligning them onto
//! transcript segments) carry the value and are unit tested; the runner and
//! downloader are thin.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::whisper::Segment;

pub const SIDECAR: &str = "sherpa-onnx-offline-speaker-diarization";
pub const SEG_MODEL: &str = "segmentation.onnx";
pub const EMB_MODEL: &str = "embedding.onnx";

/// sherpa-onnx's own converted models (the ones CaptionSmith's CI verified).
/// SHA-256 of the downloaded files, computed 2026-10-14.
pub const SEG_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
pub const SEG_ARCHIVE_SHA256: &str =
    "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488";
pub const EMB_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
pub const EMB_SHA256: &str = "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b";

/// Error prefix the UI uses to offer the model download.
pub const MODELS_MISSING: &str = "DIARIZE_MODELS_MISSING|";

/// Per-speaker colours, shared with the editor (Smith cyan/violet first).
pub const SPEAKER_PALETTE: [&str; 8] = [
    "#45f2f2", "#a974ff", "#ffd24d", "#6ee27a", "#ff7a7a", "#7ab8ff", "#ff9de0", "#ffa94d",
];

/// A stretch of audio attributed to one speaker, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: u32,
}

/// Parse sidecar stdout: result lines look like `0.031 -- 1.743 speaker_00`;
/// anything else (logs, blanks) is skipped.
pub fn parse_turns(output: &str) -> Vec<Turn> {
    let ms = |s: &str| {
        s.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| (v * 1000.0).round() as u64)
    };
    output
        .lines()
        .filter_map(|line| {
            let (times, spk) = line.trim().rsplit_once(' ')?;
            let speaker = spk.strip_prefix("speaker_")?.trim().parse::<u32>().ok()?;
            let (s, e) = times.split_once("--")?;
            let (start_ms, end_ms) = (ms(s)?, ms(e)?);
            (end_ms > start_ms).then_some(Turn {
                start_ms,
                end_ms,
                speaker,
            })
        })
        .collect()
}

fn overlap(a0: u64, a1: u64, b0: u64, b1: u64) -> u64 {
    a1.min(b1).saturating_sub(a0.max(b0))
}

/// The speaker whose turns overlap `seg` most. Votes with word timings when
/// present (sharper across a turn change), else the whole segment. Ties go
/// to the lower index so results are deterministic.
pub fn majority_speaker(seg: &Segment, turns: &[Turn]) -> Option<u32> {
    let spans: Vec<(u64, u64)> = match seg.words.as_ref().filter(|w| !w.is_empty()) {
        Some(w) => w.iter().map(|w| (w.start_ms, w.end_ms)).collect(),
        None => vec![(seg.start_ms, seg.end_ms)],
    };
    let mut totals: HashMap<u32, u64> = HashMap::new();
    for (s, e) in spans {
        for t in turns {
            let ov = overlap(s, e, t.start_ms, t.end_ms);
            if ov > 0 {
                *totals.entry(t.speaker).or_default() += ov;
            }
        }
    }
    totals
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(spk, _)| spk)
}

/// Tag every segment with its majority speaker (or `None`).
pub fn assign_speakers(segments: &mut [Segment], turns: &[Turn]) {
    for s in segments.iter_mut() {
        s.speaker = majority_speaker(s, turns);
    }
}

/// Renumber speakers in order of first appearance across several clips'
/// results is not possible (clusters are per run); within one run, make
/// "Speaker 1" the first voice heard.
pub fn renumber_by_first_appearance(segments: &mut [Segment]) {
    let mut map: HashMap<u32, u32> = HashMap::new();
    for s in segments.iter_mut() {
        if let Some(sp) = s.speaker {
            let next = map.len() as u32;
            s.speaker = Some(*map.entry(sp).or_insert(next));
        }
    }
}

pub fn args(seg_model: &Path, emb_model: &Path, wav: &Path, threads: usize) -> Vec<String> {
    vec![
        format!("--segmentation.num-threads={threads}"),
        format!("--embedding.num-threads={threads}"),
        format!("--segmentation.pyannote-model={}", seg_model.display()),
        format!("--embedding.model={}", emb_model.display()),
        // Unknown speaker count: cluster by distance threshold.
        "--clustering.cluster-threshold=0.5".into(),
        wav.to_string_lossy().into_owned(),
    ]
}

/// The two model paths in `dir`, or a `DIARIZE_MODELS_MISSING|<dir>` error.
pub fn resolve_models(dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let (s, e) = (dir.join(SEG_MODEL), dir.join(EMB_MODEL));
    if s.is_file() && e.is_file() {
        Ok((s, e))
    } else {
        Err(format!("{MODELS_MISSING}{}", dir.display()))
    }
}

/// Run the sidecar on a 16 kHz mono WAV and return its speaker turns.
/// `cancel` is polled; the child is killed on cancel.
pub fn run(
    bin: &Path,
    seg_model: &Path,
    emb_model: &Path,
    wav: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<Turn>, String> {
    let mut cmd = Command::new(bin);
    cmd.args(args(seg_model, emb_model, wav, crate::whisper::threads()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start the diarization engine: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("diarization wait: {e}")),
        }
    };
    let out = reader.join().unwrap_or_default();
    if !status.success() {
        return Err("The diarization engine failed on this audio.".into());
    }
    Ok(parse_turns(&out))
}

/// Copy `r` to `dest` via `<dest>.part`, hashing as it streams; the file is
/// renamed into place only if its SHA-256 matches `sha256`.
pub fn install_verified(
    mut r: impl Read,
    dest: &Path,
    sha256: &str,
    total: Option<u64>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    use std::io::Write;
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let res = (|| {
        let mut f = std::fs::File::create(&part).map_err(|e| format!("create: {e}"))?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut got = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let n = r
                .read(&mut buf)
                .map_err(|e| format!("download error: {e}"))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            f.write_all(&buf[..n]).map_err(|e| format!("write: {e}"))?;
            got += n as u64;
            progress(got, total);
        }
        f.flush().map_err(|e| format!("flush: {e}"))?;
        let hex = crate::models::to_hex(&h.finalize());
        if hex != sha256 {
            return Err(format!(
                "checksum mismatch for {} (got {hex})",
                dest.display()
            ));
        }
        Ok(())
    })();
    match res {
        Ok(()) => std::fs::rename(&part, dest).map_err(|e| format!("finalize: {e}")),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Recursively find the first file named `name` under `dir`.
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(p);
        }
    }
    None
}

/// Download (and verify) whichever of the two models is missing in `dir`.
/// `progress(step, label, received, total)`; step is 1 or 2. The
/// segmentation archive is unpacked with the system `tar` (bsdtar on
/// Windows 10+ and macOS reads bzip2).
#[cfg(feature = "download")]
pub fn download_models(
    dir: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u32, &str, u64, Option<u64>),
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create folder: {e}"))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .map_err(|e| e.to_string())?;
    let get = |url: &str| {
        let r = client
            .get(url)
            .send()
            .map_err(|e| format!("request failed: {e}"))?;
        if !r.status().is_success() {
            return Err(format!("download failed: HTTP {}", r.status()));
        }
        Ok(r)
    };
    let seg = dir.join(SEG_MODEL);
    if !seg.is_file() {
        let archive = dir.join("segmentation.tar.bz2");
        let r = get(SEG_ARCHIVE_URL)?;
        let total = r.content_length();
        install_verified(r, &archive, SEG_ARCHIVE_SHA256, total, cancel, |g, t| {
            progress(1, "segmentation model", g, t)
        })?;
        let tmp = dir.join("seg-extract");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let mut cmd = Command::new("tar");
        cmd.arg("xf").arg(&archive).arg("-C").arg(&tmp);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
        let found = find_file(&tmp, "model.onnx");
        let res = match (ok, found) {
            (true, Some(f)) => std::fs::copy(&f, &seg)
                .map(|_| ())
                .map_err(|e| format!("could not save the model: {e}")),
            _ => Err("could not extract the segmentation model archive.".into()),
        };
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&archive);
        res?;
    }
    let emb = dir.join(EMB_MODEL);
    if !emb.is_file() {
        let r = get(EMB_URL)?;
        let total = r.content_length();
        install_verified(r, &emb, EMB_SHA256, total, cancel, |g, t| {
            progress(2, "embedding model", g, t)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whisper::Word;

    fn seg(s: u64, e: u64) -> Segment {
        Segment {
            start_ms: s,
            end_ms: e,
            text: "x".into(),
            words: None,
            speaker: None,
        }
    }

    #[test]
    fn parses_turns_and_skips_noise() {
        let t = parse_turns(
            "Started\n0.000 -- 1.500 speaker_00\nlog\n1.500 -- 3.200 speaker_01\n1.0 -- 1.0 speaker_00\n2.0 -- 1.0 speaker_01\n1.0 -- x speaker_00\n",
        );
        assert_eq!(
            t,
            vec![
                Turn {
                    start_ms: 0,
                    end_ms: 1500,
                    speaker: 0
                },
                Turn {
                    start_ms: 1500,
                    end_ms: 3200,
                    speaker: 1
                }
            ]
        );
        assert!(parse_turns("").is_empty());
    }

    #[test]
    fn assigns_by_majority_overlap_and_words() {
        let turns = [
            Turn {
                start_ms: 0,
                end_ms: 2000,
                speaker: 0,
            },
            Turn {
                start_ms: 2000,
                end_ms: 5000,
                speaker: 1,
            },
        ];
        let mut segs = vec![seg(0, 2400), seg(2400, 5000), seg(9000, 9500)];
        let mut spans = seg(0, 3000);
        spans.words = Some(vec![
            Word {
                start_ms: 2100,
                end_ms: 2600,
                text: "a".into(),
            },
            Word {
                start_ms: 2600,
                end_ms: 2900,
                text: "b".into(),
            },
        ]);
        segs.push(spans);
        assign_speakers(&mut segs, &turns);
        let got: Vec<_> = segs.iter().map(|s| s.speaker).collect();
        assert_eq!(got, vec![Some(0), Some(1), None, Some(1)]);
    }

    #[test]
    fn ties_prefer_lower_index_and_renumbering() {
        let turns = [
            Turn {
                start_ms: 0,
                end_ms: 500,
                speaker: 3,
            },
            Turn {
                start_ms: 500,
                end_ms: 1000,
                speaker: 2,
            },
        ];
        assert_eq!(majority_speaker(&seg(0, 1000), &turns), Some(2));
        let mut segs = vec![seg(0, 1), seg(1, 2), seg(2, 3)];
        segs[0].speaker = Some(5);
        segs[1].speaker = Some(2);
        segs[2].speaker = Some(5);
        renumber_by_first_appearance(&mut segs);
        let got: Vec<_> = segs.iter().map(|s| s.speaker).collect();
        assert_eq!(got, vec![Some(0), Some(1), Some(0)]);
    }

    #[test]
    fn args_and_model_resolution() {
        let a = args(
            Path::new("s.onnx"),
            Path::new("e.onnx"),
            Path::new("a.wav"),
            4,
        )
        .join(" ");
        assert_eq!(a, "--segmentation.num-threads=4 --embedding.num-threads=4 --segmentation.pyannote-model=s.onnx --embedding.model=e.onnx --clustering.cluster-threshold=0.5 a.wav");
        let d = std::env::temp_dir().join(format!("ms-diar-{}", std::process::id()));
        std::fs::create_dir_all(d.join("x/y")).unwrap();
        assert!(resolve_models(&d).unwrap_err().starts_with(MODELS_MISSING));
        std::fs::write(d.join(SEG_MODEL), b"s").unwrap();
        std::fs::write(d.join(EMB_MODEL), b"e").unwrap();
        assert!(resolve_models(&d).is_ok());
        std::fs::write(d.join("x/y/model.onnx"), b"m").unwrap();
        assert_eq!(find_file(&d, "model.onnx"), Some(d.join("x/y/model.onnx")));
        assert_eq!(find_file(&d, "nope"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn verified_install_rejects_bad_hash_and_cancel() {
        let d = std::env::temp_dir().join(format!("ms-diar-v-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let dest = d.join("m.onnx");
        let no = AtomicBool::new(false);
        let good = crate::models::to_hex(&Sha256::digest(b"hello"));
        assert!(
            install_verified(&b"hellO"[..], &dest, &good, None, &no, |_, _| {})
                .unwrap_err()
                .contains("checksum")
        );
        assert!(!dest.exists() && !d.join("m.onnx.part").exists());
        assert_eq!(
            install_verified(
                &b"hello"[..],
                &dest,
                &good,
                None,
                &AtomicBool::new(true),
                |_, _| {}
            ),
            Err("cancelled".into())
        );
        let mut seen = 0;
        install_verified(&b"hello"[..], &dest, &good, Some(5), &no, |g, _| seen = g).unwrap();
        assert_eq!(
            (std::fs::read(&dest).unwrap(), seen),
            (b"hello".to_vec(), 5)
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hashes_are_lowercase_hex() {
        for h in [SEG_ARCHIVE_SHA256, EMB_SHA256] {
            assert_eq!(h.len(), 64);
            assert!(h
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        }
    }

    /// A fake sidecar exercises spawn, stdout parsing, failure and cancel.
    #[cfg(unix)]
    #[test]
    fn runs_a_fake_sidecar() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("ms-fake-diar-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let script = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        let (s, e, w) = (Path::new("s"), Path::new("e"), Path::new("w"));
        let no = AtomicBool::new(false);
        let ok = script("ok", "echo Started; echo '0.0 -- 1.2 speaker_01'");
        assert_eq!(
            run(&ok, s, e, w, &no).unwrap(),
            vec![Turn {
                start_ms: 0,
                end_ms: 1200,
                speaker: 1
            }]
        );
        let bad = script("bad", "exit 2");
        assert!(run(&bad, s, e, w, &no).unwrap_err().contains("failed"));
        let slow = script("slow", "sleep 5");
        let t = std::time::Instant::now();
        assert_eq!(
            run(&slow, s, e, w, &AtomicBool::new(true)),
            Err("cancelled".into())
        );
        assert!(t.elapsed() < Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(&d);
    }
}
