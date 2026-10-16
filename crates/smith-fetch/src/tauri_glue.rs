//! Tauri glue shared by the apps: commands, the in-app Browse window (sniff
//! mode + sign-in), hidden sniff fallback, cookie capture, settings, and the
//! opt-in clipboard watcher.
//!
//! Register with [`init`] in `setup`, add the commands from [`commands!`]-style
//! list in the README to `generate_handler!`, and ship the `sniff` capability
//! (remote pages in the Browse window may call only `sniff_report`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::cookies::{self, Cookie, SavedSite, Store};
use crate::fetcher::{DownloadOptions, Fetcher, Hooks, MediaInfo, Tools};
use crate::http::{self, Ctx, MAX_MANIFEST_BYTES};
use crate::sniff::{self, Collector, Hit, HitKind, INJECT_JS};
use crate::update::{Outcome, Status, Updater};
use crate::util::{self, is_http_url};
use crate::ytdlp;

/// Label of the visible Browse window.
pub const BROWSE_LABEL: &str = "smith-browse";
/// Label of the hidden sniff window.
pub const HIDDEN_LABEL: &str = "smith-sniff-hidden";
/// How long a hidden sniff pass watches a page.
const HIDDEN_SNIFF_SECS: u64 = 15;

/// User settings for downloads (persisted as JSON in the app data folder).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Read cookies from this browser for every download (`None` = off).
    pub browser_cookies: Option<String>,
    /// Offer to download links copied to the clipboard (default off).
    pub clipboard_watch: bool,
}

/// Settings plus read-only facts for the Settings panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    pub auto_update: bool,
    pub browsers: Vec<BrowserChoice>,
}

/// A browser in the cookies picker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserChoice {
    pub id: String,
    /// Known not to work on this OS (Chromium cookie encryption on Windows).
    pub unreliable: bool,
}

/// What a download produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchResult {
    /// The file to open.
    pub path: String,
    /// Every file (several for a playlist).
    pub files: Vec<String>,
    /// Where playlist files were saved (kept after exit).
    pub folder: Option<String>,
}

/// Managed state.
pub struct SmithFetch {
    pub fetcher: Arc<Fetcher>,
    app_name: String,
    download_dir: PathBuf,
    data_dir: PathBuf,
    browse: Arc<Mutex<Collector>>,
    hidden: Arc<Mutex<Collector>>,
    settings: Mutex<Settings>,
    clip_running: Arc<AtomicBool>,
}

impl SmithFetch {
    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("fetch-settings.json")
    }

    fn save_settings(&self, s: &Settings) {
        let _ = std::fs::create_dir_all(&self.data_dir);
        if let Ok(t) = serde_json::to_string_pretty(s) {
            let _ = std::fs::write(self.settings_path(), t);
        }
    }

    fn options(&self, o: Option<DownloadOptions>) -> DownloadOptions {
        let mut o = o.unwrap_or_default();
        if o.browser_cookies.is_none() {
            o.browser_cookies = self.settings.lock().map(|s| s.browser_cookies.clone()).unwrap_or_default();
        }
        o
    }
}

/// The bundled sidecar `name` next to the app binary (Tauri strips the triple).
#[must_use]
pub fn sidecar_path(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    dir.join(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() })
}

/// Set up the engine: managed state, daily update check, clipboard watcher.
/// `download_dir` is the app's temp download folder (wiped on exit).
///
/// # Errors
/// If the app data folder can't be resolved.
pub fn init<R: Runtime>(app: &AppHandle<R>, app_name: &str, download_dir: PathBuf) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let updater = Arc::new(Updater::new(data_dir.join("bin"), sidecar_path("yt-dlp")));
    let (h1, h2) = (app.clone(), app.clone());
    let hooks = Hooks {
        progress: Arc::new(move |p| {
            let _ = h1.emit("download-progress", p);
        }),
        status: Arc::new(move |s| {
            let _ = h2.emit("download-status", s.to_string());
        }),
    };
    let tools = Tools { ffmpeg: sidecar_path("ffmpeg"), updater: updater.clone() };
    let mut fetcher = Fetcher::new(app_name, tools, Store::new(data_dir.join("cookies")), hooks);
    let sniff_app = app.clone();
    fetcher.sniff = Some(Arc::new(move |url: String| {
        let app = sniff_app.clone();
        Box::pin(async move { hidden_sniff(&app, &url).await })
    }));
    let settings: Settings = std::fs::read_to_string(data_dir.join("fetch-settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let clip = settings.clipboard_watch;
    app.manage(SmithFetch {
        fetcher: Arc::new(fetcher),
        app_name: app_name.to_string(),
        download_dir,
        data_dir,
        browse: Arc::default(),
        hidden: Arc::default(),
        settings: Mutex::new(settings),
        clip_running: Arc::new(AtomicBool::new(false)),
    });
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        updater.maybe_update().await;
    });
    if clip {
        start_clipboard_watch(app);
    }
    Ok(())
}

// ---------------------------------------------------------------- downloads

/// Download a URL (yt-dlp → scraper → hidden sniff). Playlist downloads are
/// moved to `<Documents>/<App>/Downloads/<time>/` and kept.
#[tauri::command]
pub async fn fetch_download<R: Runtime>(app: AppHandle<R>, url: String, options: Option<DownloadOptions>) -> Result<FetchResult, String> {
    let st = app.state::<SmithFetch>();
    let o = st.options(options);
    let files = st.fetcher.download(url.trim(), &o, &st.download_dir).await?;
    finish(&app, &st, files, o.playlist)
}

/// Back-compat wrapper: download and return the local path.
#[tauri::command]
pub async fn download_video<R: Runtime>(app: AppHandle<R>, url: String) -> Result<String, String> {
    fetch_download(app, url, None).await.map(|r| r.path)
}

fn finish<R: Runtime>(app: &AppHandle<R>, st: &SmithFetch, files: Vec<PathBuf>, playlist: bool) -> Result<FetchResult, String> {
    let mut files = files;
    let mut folder = None;
    if playlist && files.len() > 1 {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let dest = app
            .path()
            .document_dir()
            .map_err(|e| e.to_string())?
            .join(&st.app_name)
            .join("Downloads")
            .join(format!("playlist-{stamp}"));
        std::fs::create_dir_all(&dest).map_err(|e| format!("couldn't create the playlist folder: {e}"))?;
        files = files
            .into_iter()
            .map(|f| {
                let to = dest.join(f.file_name().unwrap_or_default());
                if std::fs::rename(&f, &to).is_ok() || std::fs::copy(&f, &to).is_ok() {
                    to
                } else {
                    f
                }
            })
            .collect();
        folder = Some(dest.to_string_lossy().into_owned());
    }
    let list: Vec<String> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let path = list.first().cloned().ok_or("the download produced no file")?;
    Ok(FetchResult { path, files: list, folder })
}

/// Look up title / live / formats for the format picker.
#[tauri::command]
pub async fn fetch_probe<R: Runtime>(app: AppHandle<R>, url: String, options: Option<DownloadOptions>) -> Result<MediaInfo, String> {
    let st = app.state::<SmithFetch>();
    let o = st.options(options);
    st.fetcher.cancel.store(false, Ordering::SeqCst);
    st.fetcher.probe(url.trim(), &o).await
}

#[tauri::command]
pub fn cancel_download<R: Runtime>(app: AppHandle<R>) {
    app.state::<SmithFetch>().fetcher.cancel.store(true, Ordering::SeqCst);
}

/// End a live recording and keep what was recorded.
#[tauri::command]
pub fn stop_recording<R: Runtime>(app: AppHandle<R>) {
    app.state::<SmithFetch>().fetcher.stop.store(true, Ordering::SeqCst);
}

/// Download one media request seen in the Browse window.
#[tauri::command]
pub async fn fetch_download_hit<R: Runtime>(app: AppHandle<R>, hit: Hit, options: Option<DownloadOptions>) -> Result<FetchResult, String> {
    let st = app.state::<SmithFetch>();
    let o = st.options(options);
    let files = st.fetcher.download_hit(&hit, &o, &st.download_dir).await?;
    finish(&app, &st, files, false)
}

// ---------------------------------------------------------------- browse / sniff

fn build_window<R: Runtime>(app: &AppHandle<R>, label: &str, url: &str, visible: bool) -> Result<tauri::WebviewWindow<R>, String> {
    let parsed: url::Url = url.parse().map_err(|_| "That isn't a valid link.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http and https links can be opened.".into());
    }
    let script = if visible { INJECT_JS.to_string() } else { format!("window.__SMITH_SNIFF_AUTOPLAY__ = true;\n{INJECT_JS}") };
    let name = app.state::<SmithFetch>().app_name.clone();
    let w = WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title(format!("{name} Browser: play the video, then pick it in {name}"))
        .inner_size(1100.0, 760.0)
        .visible(visible)
        .focused(visible)
        .initialization_script(&script)
        .build()
        .map_err(|e| format!("couldn't open the browser window: {e}"))?;
    #[cfg(windows)]
    windows_sniff::attach(app, &w, label);
    Ok(w)
}

/// Open (or navigate) the Browse window. Hits stream to the main window as
/// `sniff-hits` events.
#[tauri::command]
pub async fn browse_open<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !is_http_url(&url) {
        return Err("Only http and https links can be opened.".into());
    }
    let st = app.state::<SmithFetch>();
    if let Ok(mut c) = st.browse.lock() {
        c.clear();
    }
    let _ = app.emit("sniff-hits", Vec::<Hit>::new());
    if let Some(w) = app.get_webview_window(BROWSE_LABEL) {
        let _ = w.navigate(url.parse().map_err(|_| "bad link".to_string())?);
        let _ = w.set_focus();
        return Ok(());
    }
    let w = build_window(&app, BROWSE_LABEL, &url, true)?;
    let h = app.clone();
    w.on_window_event(move |e| {
        if matches!(e, tauri::WindowEvent::Destroyed) {
            let _ = h.emit("browse-closed", ());
        }
    });
    Ok(())
}

#[tauri::command]
pub fn browse_close<R: Runtime>(app: AppHandle<R>) {
    if let Some(w) = app.get_webview_window(BROWSE_LABEL) {
        let _ = w.close();
    }
}

/// Current Browse-window candidates.
#[tauri::command]
pub fn browse_hits<R: Runtime>(app: AppHandle<R>) -> Vec<Hit> {
    app.state::<SmithFetch>().browse.lock().map(|c| c.candidates()).unwrap_or_default()
}

/// A media request seen by a page script.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reported {
    pub url: String,
    pub content_type: Option<String>,
}

/// Called by the injected script in the Browse / hidden windows only (the
/// `sniff` capability grants remote pages this one command).
#[tauri::command]
pub fn sniff_report<R: Runtime>(app: AppHandle<R>, webview: tauri::Webview<R>, page_url: String, hits: Vec<Reported>, drm: bool) {
    let label = webview.label().to_string();
    if label != BROWSE_LABEL && label != HIDDEN_LABEL {
        return;
    }
    for r in hits.into_iter().take(50) {
        if let Some(kind) = sniff::classify(&r.url, r.content_type.as_deref()) {
            add_hit(&app, &label, Hit { url: r.url, kind, page_url: page_url.clone(), headers: vec![], size: None, height: 0, live: false, drm: false });
        }
    }
    if drm {
        mark_page_drm(&app, &label);
    }
}

fn collector<R: Runtime>(app: &AppHandle<R>, label: &str) -> Arc<Mutex<Collector>> {
    let st = app.state::<SmithFetch>();
    if label == HIDDEN_LABEL {
        st.hidden.clone()
    } else {
        st.browse.clone()
    }
}

fn mark_page_drm<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let c = collector(app, label);
    if let Ok(mut c) = c.lock() {
        c.page_drm = true;
    }
    if label == BROWSE_LABEL {
        let _ = app.emit("sniff-drm", true);
    }
}

/// Record a hit; manifests are fetched once to check DRM / live / height.
pub(crate) fn add_hit<R: Runtime>(app: &AppHandle<R>, label: &str, hit: Hit) {
    let c = collector(app, label);
    let new = c.lock().map(|mut c| c.add(hit.clone())).unwrap_or(false);
    if !new {
        return;
    }
    if label == BROWSE_LABEL {
        emit_hits(app);
    }
    if matches!(hit.kind, HitKind::Hls | HitKind::Dash) {
        let app = app.clone();
        let label = label.to_string();
        tauri::async_runtime::spawn(async move {
            let never = AtomicBool::new(false);
            let ctx = Ctx { referer: Some(&hit.page_url).filter(|r| !r.is_empty()).map(String::as_str), headers: &hit.headers, cookie: None };
            if let Ok(text) = http::fetch_text(&hit.url, &ctx, MAX_MANIFEST_BYTES, &never).await {
                let height = if hit.kind == HitKind::Dash {
                    sniff::dash_max_height(&text)
                } else {
                    crate::scrape::best_hls_variant(&text, &hit.url).map_or(0, |v| v.height)
                };
                let c = collector(&app, &label);
                if let Ok(mut c) = c.lock() {
                    c.annotate(&hit.url, sniff::manifest_drm(&text), sniff::manifest_live(&text), height);
                }
                if label == BROWSE_LABEL {
                    emit_hits(&app);
                }
            }
        });
    }
}

fn emit_hits<R: Runtime>(app: &AppHandle<R>) {
    let list = app.state::<SmithFetch>().browse.lock().map(|c| c.candidates()).unwrap_or_default();
    let _ = app.emit("sniff-hits", list);
}

/// Load `url` in a hidden window for a few seconds (videos muted + played)
/// and return what it requested. Used as the last automatic fallback.
pub async fn hidden_sniff<R: Runtime>(app: &AppHandle<R>, url: &str) -> Vec<Hit> {
    let st = app.state::<SmithFetch>();
    if let Ok(mut c) = st.hidden.lock() {
        c.clear();
    }
    if let Some(w) = app.get_webview_window(HIDDEN_LABEL) {
        let _ = w.close();
    }
    let Ok(w) = build_window(app, HIDDEN_LABEL, url, false) else { return vec![] };
    let cancel = st.fetcher.cancel.clone();
    let mut waited = 0u64;
    let mut found_at: Option<u64> = None;
    while waited < HIDDEN_SNIFF_SECS * 2 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        waited += 1;
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let have = st.hidden.lock().map(|c| !c.candidates().is_empty()).unwrap_or(false);
        if have && found_at.is_none() {
            found_at = Some(waited);
        }
        // Give the player 3 s after the first hit to request its best manifest.
        if found_at.is_some_and(|f| waited >= f + 6) {
            break;
        }
    }
    let _ = w.close();
    // Let manifest annotation (DRM check) finish.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let c = st.hidden.lock().map(|c| c.clone()).unwrap_or_default();
    let mut v = c.candidates();
    if c.page_drm {
        for h in &mut v {
            h.drm = true;
        }
    }
    v
}

/// Save the Browse window's cookies for the site it's on (in-app sign-in).
/// Returns the sites saved.
#[tauri::command]
pub async fn browse_save_signin<R: Runtime>(app: AppHandle<R>) -> Result<Vec<String>, String> {
    let w = app.get_webview_window(BROWSE_LABEL).ok_or("Open the site in the browser first, then sign in.")?;
    let page = w.url().map_err(|e| e.to_string())?;
    let host = page.host_str().ok_or("That page has no site to save.")?.to_string();
    let site = cookies::site_key(&host);
    // Off the main thread: on Windows, reading cookies on it can deadlock.
    let all = tauri::async_runtime::spawn_blocking(move || w.cookies()).await.map_err(|e| e.to_string())?.map_err(|e| format!("couldn't read the browser's cookies: {e}"))?;
    let list: Vec<Cookie> = all
        .iter()
        .filter_map(|c| {
            let domain = c.domain().map(str::to_string).unwrap_or_else(|| host.clone());
            (cookies::site_key(&domain) == site).then(|| Cookie {
                name: c.name().to_string(),
                value: c.value().to_string(),
                domain: if c.domain().is_some() && !domain.starts_with('.') && domain != host { format!(".{domain}") } else { domain },
                path: c.path().unwrap_or("/").to_string(),
                secure: c.secure().unwrap_or(false),
                http_only: c.http_only().unwrap_or(false),
                expires: c.expires_datetime().map(|d| d.unix_timestamp()),
            })
        })
        .collect();
    if list.is_empty() {
        return Err(format!("No sign-in was found for {site}. Sign in in the browser window first."));
    }
    app.state::<SmithFetch>().fetcher.cookies.save(&list).map_err(|e| format!("couldn't save the sign-in: {e}"))
}

// ---------------------------------------------------------------- cookies & settings

#[tauri::command]
pub fn cookies_list<R: Runtime>(app: AppHandle<R>) -> Vec<SavedSite> {
    app.state::<SmithFetch>().fetcher.cookies.list()
}

/// Forget one saved site, or every one when `site` is omitted.
#[tauri::command]
pub fn cookies_forget<R: Runtime>(app: AppHandle<R>, site: Option<String>) {
    app.state::<SmithFetch>().fetcher.cookies.forget(site.as_deref());
}

/// Import a Netscape cookies.txt (split per site).
#[tauri::command]
pub fn cookies_import<R: Runtime>(app: AppHandle<R>, path: String) -> Result<Vec<String>, String> {
    app.state::<SmithFetch>().fetcher.cookies.import(std::path::Path::new(&path))
}

#[tauri::command]
pub fn fetch_settings_get<R: Runtime>(app: AppHandle<R>) -> SettingsView {
    let st = app.state::<SmithFetch>();
    SettingsView {
        settings: st.settings.lock().map(|s| s.clone()).unwrap_or_default(),
        auto_update: st.fetcher.tools.updater.auto_update(),
        browsers: ytdlp::BROWSERS
            .iter()
            .filter(|b| cfg!(target_os = "macos") || **b != "safari")
            .map(|b| BrowserChoice { id: (*b).to_string(), unreliable: ytdlp::browser_cookies_unreliable(b) })
            .collect(),
    }
}

/// Partial settings update from the UI.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    /// `Some("")` turns browser cookies off.
    pub browser_cookies: Option<String>,
    pub clipboard_watch: Option<bool>,
    pub auto_update: Option<bool>,
}

#[tauri::command]
pub fn fetch_settings_set<R: Runtime>(app: AppHandle<R>, patch: SettingsPatch) -> SettingsView {
    let st = app.state::<SmithFetch>();
    let mut start_clip = false;
    if let Ok(mut s) = st.settings.lock() {
        if let Some(b) = patch.browser_cookies {
            s.browser_cookies = (!b.is_empty() && ytdlp::is_supported_browser(&b)).then_some(b);
        }
        if let Some(c) = patch.clipboard_watch {
            s.clipboard_watch = c;
            start_clip = c;
        }
        st.save_settings(&s);
    }
    if let Some(a) = patch.auto_update {
        st.fetcher.tools.updater.set_auto_update(a);
    }
    if start_clip {
        start_clipboard_watch(&app);
    }
    fetch_settings_get(app)
}

#[tauri::command]
pub async fn tools_status<R: Runtime>(app: AppHandle<R>) -> Status {
    let u = app.state::<SmithFetch>().fetcher.tools.updater.clone();
    u.status().await
}

/// "Update now" in Settings (ignores the daily throttle).
#[tauri::command]
pub async fn tools_update_now<R: Runtime>(app: AppHandle<R>) -> Result<Outcome, String> {
    let u = app.state::<SmithFetch>().fetcher.tools.updater.clone();
    let out = u.update_ytdlp().await?;
    if u.deno_path().is_some() {
        let _ = u.update_deno().await;
    }
    Ok(out)
}

// ---------------------------------------------------------------- clipboard

/// Whether `s` is a single http(s) link worth offering.
#[must_use]
pub fn clipboard_link(s: &str) -> Option<String> {
    let t = s.trim();
    (t.len() < 2048 && !t.contains(char::is_whitespace) && is_http_url(t)).then(|| t.to_string())
}

/// Poll the clipboard (1.5 s) while the setting is on; emit `clipboard-link`
/// for each new http(s) link. Never reads anything else out of the clipboard.
fn start_clipboard_watch<R: Runtime>(app: &AppHandle<R>) {
    let st = app.state::<SmithFetch>();
    if st.clip_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let running = st.clip_running.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        let mut board = arboard::Clipboard::new().ok();
        // Don't offer whatever was already copied before the watcher started.
        if let Some(b) = board.as_mut() {
            last = b.get_text().ok().and_then(|t| clipboard_link(&t));
        }
        loop {
            std::thread::sleep(Duration::from_millis(1500));
            let on = app.state::<SmithFetch>().settings.lock().map(|s| s.clipboard_watch).unwrap_or(false);
            if !on {
                break;
            }
            if board.is_none() {
                board = arboard::Clipboard::new().ok();
            }
            let Some(b) = board.as_mut() else { continue };
            if let Some(link) = b.get_text().ok().and_then(|t| clipboard_link(&t)) {
                if last.as_deref() != Some(link.as_str()) {
                    last = Some(link.clone());
                    let _ = app.emit("clipboard-link", link);
                }
            }
        }
        running.store(false, Ordering::SeqCst);
    });
}

// ---------------------------------------------------------------- WebView2 (Windows)

#[cfg(windows)]
mod windows_sniff {
    //! WebView2 `WebResourceResponseReceived`: sees every response the page
    //! gets (with request headers and Content-Type), including ones made by
    //! workers that the injected script can't see. Read-only observation.

    use tauri::{AppHandle, Manager, Runtime};
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_2, ICoreWebView2WebResourceResponseReceivedEventArgs};
    use webview2_com::WebResourceResponseReceivedEventHandler;
    use windows::core::{Interface, BOOL, HSTRING, PWSTR};

    use crate::sniff::{self, Hit};

    /// Request headers worth replaying for the download.
    fn keep_header(name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        matches!(n.as_str(), "user-agent" | "referer" | "origin" | "authorization") || n.starts_with("x-")
    }

    unsafe fn read(args: &ICoreWebView2WebResourceResponseReceivedEventArgs) -> windows::core::Result<Option<(String, Option<String>, Vec<(String, String)>, Option<u64>)>> {
        let req = args.Request()?;
        let mut uri = PWSTR::null();
        req.Uri(&mut uri)?;
        let url = webview2_com::take_pwstr(uri);
        let resp = args.Response()?;
        let rh = resp.Headers()?;
        let get = |name: &str| -> Option<String> {
            let mut v = PWSTR::null();
            rh.GetHeader(&HSTRING::from(name), &mut v).ok().map(|()| webview2_com::take_pwstr(v)).filter(|s| !s.is_empty())
        };
        let ct = get("Content-Type");
        if sniff::classify(&url, ct.as_deref()).is_none() {
            return Ok(None);
        }
        let size = get("Content-Length").and_then(|s| s.parse().ok());
        let mut headers = Vec::new();
        let it = req.Headers()?.GetIterator()?;
        let mut has = BOOL(0);
        it.HasCurrentHeader(&mut has)?;
        while has.as_bool() && headers.len() < 40 {
            let (mut n, mut v) = (PWSTR::null(), PWSTR::null());
            it.GetCurrentHeader(&mut n, &mut v)?;
            let (n, v) = (webview2_com::take_pwstr(n), webview2_com::take_pwstr(v));
            if keep_header(&n) {
                headers.push((n, v));
            }
            it.MoveNext(&mut has)?;
        }
        Ok(Some((url, ct, headers, size)))
    }

    pub fn attach<R: Runtime>(app: &AppHandle<R>, w: &tauri::WebviewWindow<R>, label: &str) {
        let app = app.clone();
        let label = label.to_string();
        let _ = w.with_webview(move |wv| unsafe {
            let Ok(core) = wv.controller().CoreWebView2() else { return };
            let Ok(core2) = core.cast::<ICoreWebView2_2>() else { return };
            let app2 = app.clone();
            let label2 = label.clone();
            let handler = WebResourceResponseReceivedEventHandler::create(Box::new(move |sender, args| {
                let Some(args) = args else { return Ok(()) };
                if let Ok(Some((url, ct, headers, size))) = read(&args) {
                    let page = sender
                        .and_then(|s| {
                            let mut p = PWSTR::null();
                            s.Source(&mut p).ok().map(|()| webview2_com::take_pwstr(p))
                        })
                        .unwrap_or_default();
                    if let Some(kind) = sniff::classify(&url, ct.as_deref()) {
                        let hit = Hit { url, kind, page_url: page, headers, size, height: 0, live: false, drm: false };
                        let app3 = app2.clone();
                        let label3 = label2.clone();
                        let _ = app2.run_on_main_thread(move || super::add_hit(&app3, &label3, hit));
                    }
                }
                Ok(())
            }));
            let mut token = std::mem::zeroed();
            let _ = core2.add_WebResourceResponseReceived(&handler, &mut token);
            let _ = app.state::<super::SmithFetch>();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_links_only() {
        assert_eq!(clipboard_link("  https://youtu.be/x \n").as_deref(), Some("https://youtu.be/x"));
        assert_eq!(clipboard_link("hello world"), None);
        assert_eq!(clipboard_link("file:///c/x"), None);
        assert_eq!(clipboard_link("https://a.example/x y"), None);
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings { browser_cookies: Some("firefox".into()), clipboard_watch: true };
        let t = serde_json::to_string(&s).unwrap();
        assert!(t.contains("browserCookies"));
        assert_eq!(serde_json::from_str::<Settings>(&t).unwrap(), s);
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), Settings::default());
        let _ = util::DOWNLOAD_CANCELLED;
    }
}
