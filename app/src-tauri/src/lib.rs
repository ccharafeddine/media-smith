//! Thin Tauri shell: every command delegates to a workspace crate so logic
//! stays testable without a webview. Long-running media work (ffmpeg export,
//! yt-dlp download) lands here in phase 2.

use ms_import::ytdlp::{route, Route};
use ms_project::{Project, TrackKind};

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({ "name": "MediaSmith", "version": env!("CARGO_PKG_VERSION"), "platform": "desktop" })
}

#[tauri::command]
fn probe_file(path: String) -> Result<ms_media::probe::VideoMeta, String> {
    let ffprobe = ms_media::runner::sidecar_path("ffprobe").map_err(|e| e.to_string())?;
    let args = ms_media::runner::ffprobe_args(&path).map_err(|e| e.to_string())?;
    let json = ms_media::runner::run_tool(&ffprobe, &args).map_err(|e| e.to_string())?;
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    ms_media::probe::parse_meta(&json, &ext).map_err(|e| e.to_string())
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
    p.add_track(TrackKind::Video, "Video 2");
    p.add_track(TrackKind::Audio, "Audio");
    p.add_track(TrackKind::Caption, "Captions");
    p.to_json().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_project(path: String, json: String) -> Result<(), String> {
    Project::from_json(&json).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_project(path: String) -> Result<String, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Project::from_json(&s)
        .and_then(|p| p.to_json())
        .map_err(|e| e.to_string())
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
            load_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running MediaSmith");
}
