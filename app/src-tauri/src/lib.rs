//! Thin Tauri shell: every command delegates to a workspace crate so logic
//! stays testable without a webview.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ms_import::ytdlp::{route, Route};
use ms_media::encoders::QualityTier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ms_ai::config::ProviderConfig;
use ms_ai::keys::KeyringStore;
use ms_media::export::{
    escape_filter_path, export_args, export_duration_ms, ExportOptions, Preset, SourceInfo,
};
use ms_media::probe::VideoMeta;
use ms_media::runner::{
    ffprobe_args, run_ffmpeg_progress, run_tool, run_tool_bytes, sidecar_path, RunError,
};
use ms_project::{ItemKind, Project, TrackKind};
use tauri::{Emitter, Manager};

/// Find a media tool: the bundled sidecar next to the exe, else `MS_<TOOL>`
/// (e.g. `MS_FFMPEG`), else whatever is on PATH.
fn tool(name: &str) -> PathBuf {
    if let Ok(p) = sidecar_path(name) {
        if p.exists() {
            return p;
        }
    }
    if let Ok(p) = std::env::var(format!("MS_{}", name.to_uppercase())) {
        return PathBuf::from(p);
    }
    PathBuf::from(name)
}

fn probe(path: &str) -> Result<VideoMeta, String> {
    let json = run_tool(
        &tool("ffprobe"),
        &ffprobe_args(path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    ms_media::probe::parse_meta(&json, &ext).map_err(|e| e.to_string())
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({ "name": "MediaSmith", "version": env!("CARGO_PKG_VERSION"), "platform": "desktop" })
}

#[tauri::command]
async fn probe_file(path: String) -> Result<VideoMeta, String> {
    tauri::async_runtime::spawn_blocking(move || probe(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn classify_url(url: String) -> Result<String, String> {
    match route(&url) {
        Some(Route::Direct(ext)) => Ok(format!("direct:{ext}")),
        Some(Route::Ytdlp) => Ok("ytdlp".into()),
        None => Err("Only http(s) links can be imported.".into()),
    }
}

#[tauri::command]
fn new_project(name: String, width: u32, height: u32, fps: u32) -> Result<String, String> {
    let mut p = Project::new(&name, width, height, fps);
    p.add_track(TrackKind::Video, "Video 1");
    p.add_track(TrackKind::Audio, "Audio");
    p.add_track(TrackKind::Caption, "Captions");
    p.to_json().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_project(path: String, json: String) -> Result<(), String> {
    let p = Project::from_json(&json).map_err(|e| e.to_string())?;
    std::fs::write(path, p.to_json().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_project(path: String) -> Result<String, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Project::from_json(&s)
        .and_then(|p| p.to_json())
        .map_err(|e| e.to_string())
}

/// Cancel flag for the running export (one export at a time).
#[derive(Default)]
struct ExportState(Arc<AtomicBool>);

#[derive(Clone, serde::Serialize)]
struct Progress {
    ratio: f64,
}

/// Render the main video track with a preset, burning in caption/text layers
/// and mixing the audio track. Emits `export-progress` events; cancellable
/// with `cancel_export`.
#[tauri::command]
async fn export_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, ExportState>,
    json: String,
    preset: Preset,
    output: String,
) -> Result<String, String> {
    let cancel = state.0.clone();
    cancel.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let project = Project::from_json(&json).map_err(|e| e.to_string())?;
        let mut sources = HashMap::new();
        for t in &project.tracks {
            for it in &t.items {
                if let ItemKind::Clip { source, .. } = &it.kind {
                    if !sources.contains_key(source) {
                        let meta = probe(source)?;
                        sources.insert(
                            source.clone(),
                            SourceInfo {
                                has_audio: meta.has_audio,
                            },
                        );
                    }
                }
            }
        }
        let tmp = std::env::temp_dir().join(format!("mediasmith-export-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let mut opts = ExportOptions::default();
        if let Some(fonts) = fonts_dir(&app) {
            opts.fontsdir = Some(escape_filter_path(&fonts.to_string_lossy()));
        }
        if ms_media::ass::has_overlays(&project) {
            let ass = tmp.join("overlays.ass");
            std::fs::write(&ass, ms_media::ass::build_ass(&project, preset.size()))
                .map_err(|e| e.to_string())?;
            opts.subtitles = Some(escape_filter_path(&ass.to_string_lossy()));
        }
        let ffmpeg = tool("ffmpeg");
        let encoder = ms_media::encoders::detect_cached(&ffmpeg);
        let args = export_args(
            &project,
            preset,
            &sources,
            encoder,
            QualityTier::High,
            &opts,
            &output,
        )
        .map_err(|e| e.to_string())?;
        let total = export_duration_ms(&project);
        let mut last = -1.0;
        let result = run_ffmpeg_progress(&ffmpeg, &args, total, &cancel, |ratio| {
            if ratio - last >= 0.005 || ratio >= 1.0 {
                last = ratio;
                let _ = app.emit("export-progress", Progress { ratio });
            }
        });
        let _ = std::fs::remove_dir_all(&tmp);
        match result {
            Ok(()) => Ok(output),
            Err(RunError::Cancelled) => {
                let _ = std::fs::remove_file(&output);
                Err("Export cancelled".into())
            }
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_export(state: tauri::State<'_, ExportState>) {
    state.0.store(true, Ordering::Relaxed);
}

/// ClipSmith-style filmstrip for a clip's visible range, as a PNG data URI.
#[tauri::command]
async fn filmstrip(
    path: String,
    source_in_ms: u64,
    duration_ms: u64,
    count: u32,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ms_media::runner::reject_flaglike(&path).map_err(|e| e.to_string())?;
        let args = ms_media::filmstrip::filmstrip_args(
            &path,
            source_in_ms as f64 / 1000.0,
            duration_ms as f64 / 1000.0,
            count,
        );
        let png = run_tool_bytes(&tool("ffmpeg"), &args).map_err(|e| e.to_string())?;
        Ok(ms_media::filmstrip::png_data_uri(&png))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Bundled fonts (Syne) for libass: the resource dir when installed, the
/// source tree in dev.
fn fonts_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let installed = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join("resources").join("fonts"));
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("fonts");
    installed
        .into_iter()
        .chain(std::iter::once(dev))
        .find(|d| d.join("Syne.ttf").is_file())
}

// ---- Auto-captions (whisper) -------------------------------------------------

#[derive(Default)]
struct CaptionState {
    model_cancel: Arc<AtomicBool>,
    transcribe_cancel: Arc<AtomicBool>,
}

fn models_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let d = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

#[derive(Clone, serde::Serialize)]
struct ModelProgress {
    file: String,
    received: u64,
    total: Option<u64>,
}

#[tauri::command]
fn caption_models(app: tauri::AppHandle) -> Result<Vec<ms_captions::models::ModelInfo>, String> {
    Ok(ms_captions::models::list(&models_dir(&app)?))
}

#[tauri::command]
async fn download_caption_model(app: tauri::AppHandle, file: String) -> Result<(), String> {
    let cancel = app.state::<CaptionState>().model_cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let dir = models_dir(&app)?;
        ms_captions::models::download(&dir, &file, &cancel, |received, total| {
            let _ = app.emit(
                "model-download-progress",
                ModelProgress {
                    file: file.clone(),
                    received,
                    total,
                },
            );
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_caption_model(state: tauri::State<'_, CaptionState>) {
    state.model_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn delete_caption_model(app: tauri::AppHandle, file: String) -> Result<(), String> {
    ms_captions::models::delete(&models_dir(&app)?, &file).map_err(|e| e.to_string())
}

fn diarization_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(models_dir(app)?.join("diarization"))
}

#[derive(Clone, serde::Serialize)]
struct DiarizeProgress {
    step: u32,
    label: String,
    received: u64,
    total: Option<u64>,
}

/// Are the speaker-detection models and sidecar present?
#[tauri::command]
fn diarization_status(app: tauri::AppHandle) -> Result<(bool, bool), String> {
    let models = ms_captions::diarize::resolve_models(&diarization_dir(&app)?).is_ok();
    Ok((models, tool(ms_captions::diarize::SIDECAR).is_file()))
}

/// Download the two speaker-detection models (~46 MB, SHA-256 verified),
/// emitting `diarize-download-progress`. Cancelled by `cancel_caption_model`.
#[tauri::command]
async fn download_diarization_models(app: tauri::AppHandle) -> Result<(), String> {
    let cancel = app.state::<CaptionState>().model_cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let dir = diarization_dir(&app)?;
        ms_captions::diarize::download_models(&dir, &cancel, |step, label, received, total| {
            let _ = app.emit(
                "diarize-download-progress",
                DiarizeProgress {
                    step,
                    label: label.into(),
                    received,
                    total,
                },
            );
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Keep a copy of an in-memory file (the bundled sample, a dropped file) in
/// the app cache so the native pipeline can read it by path. Only the bare
/// file name is used, so a crafted name can't escape the folder.
#[tauri::command]
fn stash_media(app: tauri::AppHandle, name: String, bytes: Vec<u8>) -> Result<String, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("media");
    let path = stash_path(&dir, &name)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("save {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

fn stash_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    // Last segment of either separator, by hand: Path parsing is platform-specific
    // (on Windows "a:b" is a drive prefix), and this must behave the same everywhere.
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    if base.is_empty() || base == "." || base == ".." {
        return Err(format!("bad file name: {name}"));
    }
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "._- ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(dir.join(clean))
}

/// Offline path: install a whisper model file the user already has
/// (allowlisted name + SHA-256, same checks as a download).
#[tauri::command]
async fn import_caption_model(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = models_dir(&app)?;
        let no = AtomicBool::new(false);
        ms_captions::models::import_file(&dir, std::path::Path::new(&path), &no)
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Offline path: install a speaker-detection model from a local file.
#[tauri::command]
async fn import_diarization_model(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = diarization_dir(&app)?;
        ms_captions::diarize::import_model(&dir, std::path::Path::new(&path)).map(str::to_string)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_auto_caption(state: tauri::State<'_, CaptionState>) {
    state.transcribe_cancel.store(true, Ordering::Relaxed);
}

/// Transcribe every main-track clip with whisper and replace the caption
/// track (created if missing) with the result. Emits `caption-progress`
/// (0..1 over all clips). Returns the updated project JSON.
#[tauri::command]
async fn auto_caption(
    app: tauri::AppHandle,
    json: String,
    model: Option<String>,
    language: Option<String>,
    speakers: Option<bool>,
) -> Result<String, String> {
    let cancel = app.state::<CaptionState>().transcribe_cancel.clone();
    let speakers = speakers.unwrap_or(false);
    cancel.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        use ms_captions::{models, place, whisper};
        let mut project = Project::from_json(&json).map_err(|e| e.to_string())?;
        let file = model.unwrap_or_else(|| models::DEFAULT_MODEL.into());
        let model_path = models::resolve(&models_dir(&app)?, &file)
            .map_err(|_| format!("MODEL_NOT_FOUND|{file}"))?;
        let clips: Vec<(u64, u64, String, u64)> = project
            .main_video_clips()
            .iter()
            .filter_map(|it| match &it.kind {
                ItemKind::Clip {
                    source,
                    source_in_ms,
                    ..
                } => Some((it.start_ms, it.duration_ms, source.clone(), *source_in_ms)),
                _ => None,
            })
            .collect();
        if clips.is_empty() {
            return Err("Add a clip to the main video track first.".into());
        }
        // Speaker detection needs its sidecar and models up front, so a
        // missing piece fails before minutes of transcription.
        let diar = if speakers {
            let (seg, emb) = ms_captions::diarize::resolve_models(&diarization_dir(&app)?)?;
            let bin = tool(ms_captions::diarize::SIDECAR);
            if !bin.is_file() {
                return Err(
                    "The speaker-detection engine isn't installed in this build (sherpa-onnx sidecar missing)."
                        .into(),
                );
            }
            Some((bin, seg, emb))
        } else {
            None
        };
        let tmp = std::env::temp_dir().join(format!("mediasmith-captions-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let opts = whisper::Options {
            language,
            words: true,
            ..Default::default()
        };
        let n = clips.len() as f64;
        let mut cues = Vec::new();
        let mut done: Vec<(u64, u64, PathBuf, Vec<whisper::Segment>)> = Vec::new();
        let result = (|| {
            for (k, (start, dur, source, src_in)) in clips.iter().enumerate() {
                let meta = probe(source)?;
                if !meta.has_audio {
                    continue;
                }
                ms_media::runner::reject_flaglike(source).map_err(|e| e.to_string())?;
                let wav = tmp.join(format!("clip{k}.wav"));
                ms_media::runner::run_ffmpeg(
                    &tool("ffmpeg"),
                    &whisper::extract_wav_args(source, *src_in, *dur, &wav),
                )
                .map_err(|e| e.to_string())?;
                let segs = whisper::transcribe(
                    &tool("whisper-cli"),
                    &model_path,
                    &wav,
                    &opts,
                    &cancel,
                    |p| {
                        let _ = app.emit(
                            "caption-progress",
                            Progress {
                                ratio: (k as f64 + p / 100.0) / n,
                            },
                        );
                    },
                )
                .map_err(|e| e.to_string())?;
                done.push((*start, *dur, wav, segs));
            }
            // One diarization pass over every clip's audio joined end to end,
            // so a voice keeps the same speaker id across clips.
            if let (Some((bin, seg, emb)), false) = (&diar, done.is_empty()) {
                use ms_captions::diarize;
                let wavs: Vec<PathBuf> = done.iter().map(|d| d.2.clone()).collect();
                let all = tmp.join("all.wav");
                let durs = diarize::concat_wavs(&wavs, &all)?;
                let turns = diarize::run(bin, seg, emb, &all, &cancel)?;
                for (d, t) in done.iter_mut().zip(diarize::split_turns(&turns, &durs)) {
                    diarize::assign_speakers(&mut d.3, &t);
                }
                diarize::renumber_by_first_appearance(done.iter_mut().flat_map(|d| d.3.iter_mut()));
            }
            for (start, dur, _, segs) in &done {
                cues.extend(place::place_segments(*start, *dur, segs, 6));
            }
            Ok::<(), String>(())
        })();
        let _ = std::fs::remove_dir_all(&tmp);
        result?;
        let track = match project.tracks.iter().find(|t| t.kind == TrackKind::Caption) {
            Some(t) => t.id.clone(),
            None => project.add_track(TrackKind::Caption, "Captions"),
        };
        let items: Vec<ms_project::CaptionCue> = cues
            .into_iter()
            .map(|c| ms_project::CaptionCue {
                start_ms: c.start_ms,
                duration_ms: c.duration_ms,
                text: c.text,
                words: c
                    .words
                    .into_iter()
                    .map(|w| ms_project::CaptionWord {
                        start_ms: w.start_ms,
                        end_ms: w.end_ms,
                        text: w.text,
                    })
                    .collect(),
                speaker: c.speaker,
            })
            .collect();
        project
            .replace_caption_cues(&track, &items, &ms_project::TextStyle::default())
            .map_err(|e| e.to_string())?;
        project.to_json().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- AI (bring your own key) ------------------------------------------------

/// AI settings file (no secrets) + the OS keychain for keys. The logic
/// lives in ms_ai::service (unit tested); these commands only adapt it.
type AiState = ms_ai::service::AiService<KeyringStore>;
use ms_ai::service::AiView;

#[tauri::command]
fn ai_settings(state: tauri::State<'_, AiState>) -> AiView {
    state.view()
}

/// Save a provider. `key`: `Some("")` deletes the stored key, `None` keeps it.
#[tauri::command]
fn ai_save_provider(
    state: tauri::State<'_, AiState>,
    config: ProviderConfig,
    key: Option<String>,
    make_default: bool,
) -> Result<AiView, String> {
    state.save_provider(config, key, make_default)
}

#[tauri::command]
fn ai_delete_provider(state: tauri::State<'_, AiState>, id: String) -> Result<AiView, String> {
    state.delete_provider(&id)
}

fn ai_client(state: &AiState, id: Option<&str>) -> Result<(ms_ai::client::Client, String), String> {
    state.client(id)
}

#[tauri::command]
async fn ai_test_provider(app: tauri::AppHandle, id: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (c, _) = ai_client(&app.state::<AiState>(), Some(&id))?;
        c.list_models().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn ai_rewrite_captions(
    app: tauri::AppHandle,
    lines: Vec<String>,
    instruction: String,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (c, model) = ai_client(&app.state::<AiState>(), None)?;
        ms_ai::rewrite::rewrite_captions(&c, &model, &lines, &instruction)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ExportState::default())
        .manage(CaptionState::default())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AiState::load(dir.join("ai-providers.json"), KeyringStore));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            probe_file,
            classify_url,
            new_project,
            save_project,
            load_project,
            export_project,
            cancel_export,
            filmstrip,
            ai_settings,
            ai_save_provider,
            ai_delete_provider,
            ai_test_provider,
            ai_rewrite_captions,
            caption_models,
            download_caption_model,
            cancel_caption_model,
            delete_caption_model,
            auto_caption,
            cancel_auto_caption,
            diarization_status,
            download_diarization_models,
            import_caption_model,
            import_diarization_model,
            stash_media
        ])
        .run(tauri::generate_context!())
        .expect("error while running MediaSmith");
}

#[cfg(test)]
mod tests {
    #[test]
    fn stash_path_keeps_only_a_clean_file_name() {
        let d = std::path::Path::new("cache");
        assert_eq!(
            super::stash_path(d, "sample.mp4").unwrap(),
            d.join("sample.mp4")
        );
        assert_eq!(
            super::stash_path(d, "../../evil.mp4").unwrap(),
            d.join("evil.mp4")
        );
        assert_eq!(
            super::stash_path(d, "a:b*c.mov").unwrap(),
            d.join("a_b_c.mov")
        );
        assert!(super::stash_path(d, "..").is_err());
    }
}
