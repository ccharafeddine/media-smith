//! The download orchestrator shared by the apps.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::cookies::{self, Store};
use crate::http::{self, Ctx, MAX_HTML_BYTES};
use crate::scrape::{self, MediaKind};
use crate::sniff::{Hit, HitKind};
use crate::stream;
use crate::update::Updater;
use crate::util::{self, is_http_url, DOWNLOAD_CANCELLED, UNSUPPORTED_SITE};
use crate::ytdlp::{self, Cookies, RunError};

pub use crate::ytdlp::ProbeInfo as MediaInfo;

/// Most media candidates the scraper tries per page.
const MAX_SCRAPE_ATTEMPTS: usize = 3;
/// Most embedded players the scraper follows (one yt-dlp try + one scrape each).
const MAX_EMBEDS: usize = 3;

/// The helper binaries.
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub updater: Arc<Updater>,
}

/// Progress (0.0-1.0) and short status lines for the UI.
#[derive(Clone)]
pub struct Hooks {
    pub progress: Arc<dyn Fn(f64) + Send + Sync>,
    pub status: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Hooks {
    #[must_use]
    pub fn silent() -> Self {
        Self {
            progress: Arc::new(|_| {}),
            status: Arc::new(|_| {}),
        }
    }
}

/// Runs a hidden sniff pass on a page and returns the media it saw.
pub type SniffFn =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Vec<Hit>> + Send>> + Send + Sync>;

/// Per-download choices from the UI.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadOptions {
    /// A `-f` spec from the format picker (`None` = best available).
    pub format: Option<String>,
    /// Download the whole playlist (opt-in; capped at 50 items).
    pub playlist: bool,
    /// The URL is a live stream: record until Stop.
    pub live: bool,
    /// Record a live stream from its start where the site allows it.
    pub live_from_start: bool,
    pub referer: Option<String>,
    pub headers: Vec<(String, String)>,
    /// Read cookies from this browser (Settings), e.g. "firefox".
    pub browser_cookies: Option<String>,
    /// Skip the per-site cookies saved by in-app sign-in.
    pub no_saved_cookies: bool,
    /// Skip the scraper/sniff fallbacks (smoke tests, probes).
    pub no_fallback: bool,
}

/// The engine. One per app; `cancel`/`stop` are shared with the UI commands.
pub struct Fetcher {
    pub app_name: String,
    pub tools: Tools,
    pub cookies: Store,
    pub hooks: Hooks,
    pub cancel: Arc<AtomicBool>,
    /// Ends a live recording and keeps what was recorded.
    pub stop: Arc<AtomicBool>,
    pub sniff: Option<SniffFn>,
    version_cache: Mutex<Option<(PathBuf, Option<String>)>>,
}

fn cancelled_err() -> String {
    DOWNLOAD_CANCELLED.to_string()
}

impl Fetcher {
    #[must_use]
    pub fn new(app_name: &str, tools: Tools, cookies: Store, hooks: Hooks) -> Self {
        Self {
            app_name: app_name.to_string(),
            tools,
            cookies,
            hooks,
            cancel: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            sniff: None,
            version_cache: Mutex::new(None),
        }
    }

    fn status(&self, s: &str) {
        (self.hooks.status)(s);
    }

    fn ffmpeg_dir(&self) -> Option<String> {
        self.tools
            .ffmpeg
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
    }

    async fn ytdlp_version(&self, exe: &Path) -> Option<String> {
        let mut g = self.version_cache.lock().await;
        if let Some((p, v)) = g.as_ref() {
            if p == exe {
                return v.clone();
            }
        }
        let v = ytdlp::version(exe).await;
        *g = Some((exe.to_path_buf(), v.clone()));
        v
    }

    fn cookie_source(&self, url: &str, o: &DownloadOptions) -> Cookies {
        if !o.no_saved_cookies {
            if let Some(f) = self.cookies.file_for_url(url) {
                return Cookies::File(f);
            }
        }
        match &o.browser_cookies {
            Some(b) if ytdlp::is_supported_browser(b) => Cookies::Browser(b.clone()),
            _ => Cookies::None,
        }
    }

    fn cookie_header(&self, url: &str, o: &DownloadOptions) -> Option<String> {
        if o.no_saved_cookies {
            return None;
        }
        let host = url::Url::parse(url).ok()?.host_str()?.to_string();
        cookies::header_for_host(&self.cookies.cookies_for_url(url), &host)
    }

    /// Deno for YouTube (fetched on first use) when yt-dlp can use it.
    async fn deno_for(&self, url: &str, exe: &Path) -> Option<String> {
        if !util::is_youtube(url) {
            return None;
        }
        let v = self.ytdlp_version(exe).await?;
        if !ytdlp::supports_js_runtimes(&v) {
            return None;
        }
        if self.tools.updater.deno_path().is_none() {
            self.status("Getting YouTube support (one-time download)…");
        }
        match self.tools.updater.ensure_deno().await {
            Ok(p) => Some(p.to_string_lossy().into_owned()),
            Err(_) => None,
        }
    }

    /// Ask yt-dlp what a URL offers (title, live, formats for the picker).
    ///
    /// # Errors
    /// A user-facing message when yt-dlp can't read the URL.
    pub async fn probe(&self, url: &str, o: &DownloadOptions) -> Result<MediaInfo, String> {
        if !is_http_url(url) {
            return Err("Only http and https links are supported.".to_string());
        }
        let exe = self.tools.updater.ytdlp_path();
        let deno = self.deno_for(url, &exe).await;
        let mut args: Vec<String> = vec![
            "--ignore-config".into(),
            "-J".into(),
            "--socket-timeout".into(),
            "20".into(),
        ];
        if o.playlist {
            args.extend(["--flat-playlist".into(), "--yes-playlist".into()]);
        } else {
            args.push("--no-playlist".into());
        }
        match self.cookie_source(url, o) {
            Cookies::File(p) => args.extend(["--cookies".into(), p.to_string_lossy().into_owned()]),
            Cookies::Browser(b) => args.extend(["--cookies-from-browser".into(), b]),
            Cookies::None => {}
        }
        if let Some(d) = deno {
            args.extend(["--js-runtimes".into(), format!("deno:{d}")]);
        }
        args.extend(["--".into(), url.into()]);
        let child = util::command(&exe)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("yt-dlp failed to start: {e}"))?;
        let out = tokio::select! {
            biased;
            () = util::cancelled(&self.cancel) => return Err(cancelled_err()),
            r = tokio::time::timeout(std::time::Duration::from_secs(90), child.wait_with_output()) => r
                .map_err(|_| "Looking up that link took too long.".to_string())?
                .map_err(|e| e.to_string())?,
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        match ytdlp::parse_probe(&stdout) {
            Some(i) if out.status.success() => Ok(i),
            _ => Err(ytdlp::friendly_error(
                &String::from_utf8_lossy(&out.stderr),
                &self.app_name,
            )),
        }
    }

    async fn ytdlp_once(
        &self,
        url: &str,
        o: &DownloadOptions,
        dir: &Path,
        impersonate: bool,
        referer: Option<&str>,
    ) -> Result<Vec<PathBuf>, RunError> {
        let exe = self.tools.updater.ytdlp_path();
        let deno = self.deno_for(url, &exe).await;
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir)
            .map_err(|e| RunError::Failed(format!("could not prepare download folder: {e}")))?;
        let template = if o.playlist {
            dir.join("%(playlist_index)03d - %(title).60B.%(ext)s")
        } else {
            dir.join("source.%(ext)s")
        };
        let template = template.to_string_lossy().into_owned();
        let ffdir = self.ffmpeg_dir();
        let args = ytdlp::build_args(&ytdlp::Args {
            url,
            template: &template,
            ffmpeg_dir: ffdir.as_deref(),
            referer: referer.or(o.referer.as_deref()),
            headers: &o.headers,
            cookies: self.cookie_source(url, o),
            format: o.format.as_deref(),
            playlist: o.playlist,
            impersonate,
            deno: deno.as_deref(),
            live: o.live,
            live_from_start: o.live_from_start,
        });
        let progress = self.hooks.progress.clone();
        let status = self.hooks.status.clone();
        let stop = o.live.then_some(&*self.stop);
        ytdlp::run(&exe, args, dir, &self.cancel, stop, &*progress, &*status).await
    }

    /// yt-dlp with the retry ladder: impersonation on 403/bot walls, then a
    /// (rate-limited) yt-dlp update and one more try.
    async fn ytdlp_with_retries(
        &self,
        url: &str,
        o: &DownloadOptions,
        dir: &Path,
        referer: Option<&str>,
    ) -> Result<Vec<PathBuf>, RunError> {
        let mut last = match self.ytdlp_once(url, o, dir, false, referer).await {
            Ok(f) => return Ok(f),
            Err(RunError::Cancelled) => return Err(RunError::Cancelled),
            Err(RunError::Failed(e)) => e,
        };
        if ytdlp::wants_impersonation(&last) {
            self.status("Retrying as a regular browser…");
            match self.ytdlp_once(url, o, dir, true, referer).await {
                Ok(f) => return Ok(f),
                Err(RunError::Cancelled) => return Err(RunError::Cancelled),
                Err(RunError::Failed(e)) => {
                    if !e.to_lowercase().contains("impersonate target") {
                        last = e;
                    }
                }
            }
        }
        if ytdlp::is_unsupported(&last) || ytdlp::wants_impersonation(&last) {
            self.status("Checking for a downloader update…");
            if self.tools.updater.update_after_failure().await {
                *self.version_cache.lock().await = None;
                self.status("Downloader updated. Retrying…");
                let imp = ytdlp::wants_impersonation(&last);
                match self.ytdlp_once(url, o, dir, imp, referer).await {
                    Ok(f) => return Ok(f),
                    Err(RunError::Cancelled) => return Err(RunError::Cancelled),
                    Err(RunError::Failed(e)) => last = e,
                }
            }
        }
        Err(RunError::Failed(last))
    }

    /// Download `url` into `dir` (wiped first). Returns the file(s): one,
    /// or several for an opt-in playlist.
    ///
    /// # Errors
    /// A user-facing message ([`DOWNLOAD_CANCELLED`] on cancel; sign-in-fixable
    /// errors start with [`util::SIGNIN_PREFIX`]).
    pub async fn download(
        &self,
        url: &str,
        o: &DownloadOptions,
        dir: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        self.cancel.store(false, Ordering::SeqCst);
        self.stop.store(false, Ordering::SeqCst);
        if !is_http_url(url) {
            return Err("Only http and https links are supported.".to_string());
        }
        let cookie = self.cookie_header(url, o);
        let ctx = Ctx {
            referer: o.referer.as_deref(),
            headers: &o.headers,
            cookie: cookie.as_deref(),
        };
        if let Some(ext) = util::direct_media_extension(url) {
            return self.direct(url, &ext, &ctx, dir).await.map(|p| vec![p]);
        }
        if let Some(m) = util::direct_manifest_extension(url) {
            if m != "m3u8" {
                // DASH: check for DRM, then let yt-dlp pick and merge the
                // best video + audio representations (ffmpeg's demuxer
                // chokes on many real-world MPDs).
                if let Ok(text) = http::fetch_text(url, &ctx, MAX_HTML_BYTES, &self.cancel).await {
                    if crate::sniff::manifest_drm(&text) {
                        return Err(stream::DRM_MESSAGE.to_string());
                    }
                }
                return match self.ytdlp_with_retries(url, o, dir, None).await {
                    Ok(files) => Ok(self.finish_live(o, files).await),
                    Err(RunError::Cancelled) => Err(cancelled_err()),
                    Err(RunError::Failed(e)) => Err(ytdlp::friendly_error(&e, &self.app_name)),
                };
            }
            return self
                .stream(url, m == "m3u8", &ctx, dir)
                .await
                .map(|p| vec![p]);
        }
        match self.ytdlp_with_retries(url, o, dir, None).await {
            Ok(files) => Ok(self.finish_live(o, files).await),
            Err(RunError::Cancelled) => Err(cancelled_err()),
            Err(RunError::Failed(e)) if ytdlp::is_unsupported(&e) && !o.no_fallback => {
                self.fallbacks(url, o, dir).await
            }
            Err(RunError::Failed(e)) => Err(ytdlp::friendly_error(&e, &self.app_name)),
        }
    }

    /// A stopped live recording from yt-dlp is MPEG-TS whatever its name: remux.
    async fn finish_live(&self, o: &DownloadOptions, files: Vec<PathBuf>) -> Vec<PathBuf> {
        if !o.live {
            return files;
        }
        let mut out = Vec::new();
        for f in files {
            let ts = f.with_extension("ts");
            if ts != f && std::fs::rename(&f, &ts).is_err() {
                out.push(f);
                continue;
            }
            self.status("Finishing recording…");
            out.push(stream::ts_to_mp4(&self.tools.ffmpeg, &ts).await);
        }
        out
    }

    async fn direct(
        &self,
        url: &str,
        ext: &str,
        ctx: &Ctx<'_>,
        dir: &Path,
    ) -> Result<PathBuf, String> {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not prepare download folder: {e}"))?;
        let out = dir.join(format!("source.{ext}"));
        let progress = self.hooks.progress.clone();
        match http::fetch_to_file(url, ctx, &out, &self.cancel, &*progress).await {
            Ok(()) => Ok(out),
            Err(e) => {
                let _ = std::fs::remove_dir_all(dir);
                Err(e)
            }
        }
    }

    async fn stream(
        &self,
        url: &str,
        hls: bool,
        ctx: &Ctx<'_>,
        dir: &Path,
    ) -> Result<PathBuf, String> {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not prepare download folder: {e}"))?;
        let progress = self.hooks.progress.clone();
        let status = self.hooks.status.clone();
        stream::download(
            &self.tools.ffmpeg,
            url,
            hls,
            ctx,
            dir,
            &self.cancel,
            Some(&self.stop),
            &*progress,
            &*status,
        )
        .await
    }

    /// Scraper, then a hidden sniff pass.
    async fn fallbacks(
        &self,
        url: &str,
        o: &DownloadOptions,
        dir: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        self.status("Looking for the video on the page…");
        let scraped = self.scrape_fallback(url, o, dir).await;
        match scraped {
            Ok(p) => return Ok(vec![p]),
            Err(e) if e == DOWNLOAD_CANCELLED => return Err(e),
            Err(_) => {}
        }
        let scrape_err = scraped.err().unwrap_or_default();
        if let Some(sniff) = &self.sniff {
            self.status("Opening the page in the background to find the video…");
            let hits = sniff(url.to_string()).await;
            if self.cancel.load(Ordering::SeqCst) {
                return Err(cancelled_err());
            }
            if hits.iter().all(|h| h.drm) && !hits.is_empty() {
                return Err(stream::DRM_MESSAGE.to_string());
            }
            let mut last = scrape_err;
            for h in hits.iter().filter(|h| !h.drm).take(MAX_SCRAPE_ATTEMPTS) {
                match self.download_hit(h, o, dir).await {
                    Ok(f) => return Ok(f),
                    Err(e) if e == DOWNLOAD_CANCELLED => return Err(e),
                    Err(e) => last = e,
                }
            }
            return Err(if last.is_empty() {
                UNSUPPORTED_SITE.to_string()
            } else {
                last
            });
        }
        Err(scrape_err)
    }

    /// Download one media request captured by sniff mode.
    ///
    /// # Errors
    /// A user-facing message; DRM hits are refused.
    pub async fn download_hit(
        &self,
        h: &Hit,
        o: &DownloadOptions,
        dir: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        self.cancel.store(false, Ordering::SeqCst);
        self.stop.store(false, Ordering::SeqCst);
        if h.drm {
            return Err(stream::DRM_MESSAGE.to_string());
        }
        if !is_http_url(&h.url) {
            return Err(UNSUPPORTED_SITE.to_string());
        }
        let referer = (!h.page_url.is_empty()).then_some(h.page_url.as_str());
        let cookie = self
            .cookie_header(&h.url, o)
            .or_else(|| referer.and_then(|r| self.cookie_header(r, o)));
        let ctx = Ctx {
            referer,
            headers: &h.headers,
            cookie: cookie.as_deref(),
        };
        match h.kind {
            HitKind::Hls => self.stream(&h.url, true, &ctx, dir).await.map(|p| vec![p]),
            HitKind::Dash => self.stream(&h.url, false, &ctx, dir).await.map(|p| vec![p]),
            HitKind::File => {
                let ext = util::direct_media_extension(&h.url).unwrap_or_else(|| "mp4".to_string());
                self.direct(&h.url, &ext, &ctx, dir).await.map(|p| vec![p])
            }
            HitKind::Segment => Err(UNSUPPORTED_SITE.to_string()),
        }
    }

    async fn try_media(
        &self,
        candidates: &[scrape::Candidate],
        referer: &str,
        o: &DownloadOptions,
        dir: &Path,
        last_err: &mut String,
    ) -> Result<Option<PathBuf>, String> {
        let cookie = self.cookie_header(referer, o);
        let ctx = Ctx {
            referer: Some(referer),
            headers: &[],
            cookie: cookie.as_deref(),
        };
        for c in candidates
            .iter()
            .filter(|c| c.kind != MediaKind::Embed)
            .take(MAX_SCRAPE_ATTEMPTS)
        {
            let r = match &c.kind {
                MediaKind::File(ext) => self.direct(&c.url, ext, &ctx, dir).await,
                MediaKind::Hls => self.stream(&c.url, true, &ctx, dir).await,
                _ => self.stream(&c.url, false, &ctx, dir).await,
            };
            match r {
                Ok(p) => return Ok(Some(p)),
                Err(e) if e == DOWNLOAD_CANCELLED => return Err(e),
                Err(e) => *last_err = e,
            }
        }
        Ok(None)
    }

    /// The generic page scraper and its embedded players.
    async fn scrape_fallback(
        &self,
        page_url: &str,
        o: &DownloadOptions,
        dir: &Path,
    ) -> Result<PathBuf, String> {
        let cookie = self.cookie_header(page_url, o);
        let ctx = Ctx {
            referer: None,
            headers: &[],
            cookie: cookie.as_deref(),
        };
        let html = http::fetch_text(page_url, &ctx, MAX_HTML_BYTES, &self.cancel).await?;
        let candidates = scrape::extract_candidates(&html, page_url);
        let mut last_err = UNSUPPORTED_SITE.to_string();
        if let Some(p) = self
            .try_media(&candidates, page_url, o, dir, &mut last_err)
            .await?
        {
            return Ok(p);
        }
        let single = DownloadOptions {
            playlist: false,
            live: false,
            ..o.clone()
        };
        for embed in candidates
            .iter()
            .filter(|c| c.kind == MediaKind::Embed)
            .take(MAX_EMBEDS)
        {
            match self
                .ytdlp_once(&embed.url, &single, dir, false, Some(page_url))
                .await
            {
                Ok(mut f) if !f.is_empty() => return Ok(f.remove(0)),
                Ok(_) | Err(RunError::Failed(_)) => {}
                Err(RunError::Cancelled) => return Err(cancelled_err()),
            }
            let ectx = Ctx {
                referer: Some(page_url),
                headers: &[],
                cookie: None,
            };
            match http::fetch_text(&embed.url, &ectx, MAX_HTML_BYTES, &self.cancel).await {
                Ok(h) => {
                    let inner = scrape::extract_candidates(&h, &embed.url);
                    if let Some(p) = self
                        .try_media(&inner, &embed.url, o, dir, &mut last_err)
                        .await?
                    {
                        return Ok(p);
                    }
                }
                Err(e) if e == DOWNLOAD_CANCELLED => return Err(e),
                Err(_) => {}
            }
        }
        Err(last_err)
    }
}
