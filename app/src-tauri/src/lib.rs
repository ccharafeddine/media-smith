//! Thin Tauri shell: every command delegates to a workspace crate so logic
//! stays testable without a webview.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ms_import::ytdlp::{route, Route};
use ms_media::encoders::QualityTier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ms_ai::config::{AiSettings, ProviderConfig};
use ms_ai::keys::{KeyStore, KeyringStore};
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

// ---- AI (bring your own key) ------------------------------------------------

/// AI settings file (no secrets) + the OS keychain for keys.
struct AiState {
    path: PathBuf,
    settings: Mutex<AiSettings>,
}

impl AiState {
    fn load(path: PathBuf) -> Self {
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| AiSettings::from_json(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            settings: Mutex::new(settings),
        }
    }
    fn save(&self, s: &AiSettings) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, s.to_json()).map_err(|e| e.to_string())
    }
}

#[derive(serde::Serialize)]
struct ProviderView {
    #[serde(flatten)]
    config: ProviderConfig,
    has_key: bool,
}

#[derive(serde::Serialize)]
struct AiView {
    providers: Vec<ProviderView>,
    text_provider: Option<String>,
}

fn ai_view(s: &AiSettings) -> AiView {
    AiView {
        providers: s
            .providers
            .iter()
            .map(|p| ProviderView {
                config: p.clone(),
                has_key: KeyringStore.get(&p.id).ok().flatten().is_some(),
            })
            .collect(),
        text_provider: s.text_provider.clone(),
    }
}

#[tauri::command]
fn ai_settings(state: tauri::State<'_, AiState>) -> AiView {
    ai_view(&state.settings.lock().unwrap())
}

/// Save a provider. `key`: `Some("")` deletes the stored key, `None` keeps it.
#[tauri::command]
fn ai_save_provider(
    state: tauri::State<'_, AiState>,
    config: ProviderConfig,
    key: Option<String>,
    make_default: bool,
) -> Result<AiView, String> {
    let mut s = state.settings.lock().unwrap();
    let id = config.id.clone();
    s.upsert(config).map_err(|e| e.to_string())?;
    match key.as_deref().map(str::trim) {
        Some("") => KeyringStore.delete(&id).map_err(|e| e.to_string())?,
        Some(k) => KeyringStore.set(&id, k).map_err(|e| e.to_string())?,
        None => {}
    }
    if make_default {
        s.text_provider = Some(id);
    }
    state.save(&s)?;
    Ok(ai_view(&s))
}

#[tauri::command]
fn ai_delete_provider(state: tauri::State<'_, AiState>, id: String) -> Result<AiView, String> {
    let mut s = state.settings.lock().unwrap();
    s.remove(&id);
    KeyringStore.delete(&id).map_err(|e| e.to_string())?;
    state.save(&s)?;
    Ok(ai_view(&s))
}

fn ai_client(state: &AiState, id: Option<&str>) -> Result<(ms_ai::client::Client, String), String> {
    let s = state.settings.lock().unwrap();
    let id = id
        .map(str::to_string)
        .or_else(|| s.text_provider.clone())
        .ok_or("Add an AI provider in Settings first.")?;
    let cfg = s.get(&id).map_err(|e| e.to_string())?.clone();
    drop(s);
    let key = KeyringStore.get(&cfg.id).map_err(|e| e.to_string())?;
    let model = cfg.model.clone();
    Ok((
        ms_ai::client::Client::new(&cfg, key).map_err(|e| e.to_string())?,
        model,
    ))
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
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AiState::load(dir.join("ai-providers.json")));
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
            ai_rewrite_captions
        ])
        .run(tauri::generate_context!())
        .expect("error while running MediaSmith");
}
