//! Thin Tauri shell: every command delegates to a workspace crate so logic
//! stays testable without a webview.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ms_import::ytdlp::{route, Route};
use ms_media::encoders::QualityTier;
use ms_media::export::{export_args, Preset, SourceInfo};
use ms_media::probe::VideoMeta;
use ms_media::runner::{ffprobe_args, run_ffmpeg, run_tool, sidecar_path};
use ms_project::{ItemKind, Project, TrackKind};

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

/// Render the main video track with a preset. Probes each source (for audio
/// presence), picks the cached hardware encoder, runs ffmpeg off the UI thread.
#[tauri::command]
async fn export_project(json: String, preset: Preset, output: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let project = Project::from_json(&json).map_err(|e| e.to_string())?;
        let mut sources = HashMap::new();
        for it in project.main_video_clips() {
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
        let ffmpeg = tool("ffmpeg");
        let encoder = ms_media::encoders::detect_cached(&ffmpeg);
        let args = export_args(
            &project,
            preset,
            &sources,
            encoder,
            QualityTier::High,
            &output,
        )
        .map_err(|e| e.to_string())?;
        run_ffmpeg(&ffmpeg, &args).map_err(|e| e.to_string())?;
        Ok(output)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_info,
            probe_file,
            classify_url,
            new_project,
            save_project,
            load_project,
            export_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running MediaSmith");
}
