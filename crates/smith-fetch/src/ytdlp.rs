//! yt-dlp: argument building, running, progress parsing, error mapping and
//! `-J` format parsing for the format picker.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::util::{self, cancelled, is_http_url, MAX_DOWNLOAD_YTDLP, SIGNIN_PREFIX};

/// Default format selection: best video-only + best audio (merged), else the
/// best single file with audio, else any video + audio.
pub const FORMAT_BEST: &str = "bv*[acodec=none]+ba/b/bv*+ba";
/// Format sort: highest resolution, then frame rate, then bitrate.
pub const FORMAT_SORT: &str = "res,fps,br";
/// Merge container: mp4 when the codecs fit, mkv otherwise.
pub const MERGE: &str = "mp4/mkv";
/// Most items an opt-in playlist download takes.
pub const PLAYLIST_MAX_ITEMS: u32 = 50;

/// Where cookies for a yt-dlp run come from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Cookies {
    #[default]
    None,
    /// A Netscape cookies.txt file (yt-dlp `--cookies`).
    File(PathBuf),
    /// A browser profile (yt-dlp `--cookies-from-browser`), e.g. "firefox".
    Browser(String),
}

/// Everything that shapes one yt-dlp invocation.
#[derive(Debug, Clone, Default)]
pub struct Args<'a> {
    pub url: &'a str,
    /// Output template (`dir/source.%(ext)s`).
    pub template: &'a str,
    pub ffmpeg_dir: Option<&'a str>,
    pub referer: Option<&'a str>,
    pub headers: &'a [(String, String)],
    pub cookies: Cookies,
    /// A specific `-f` spec from the format picker.
    pub format: Option<&'a str>,
    pub playlist: bool,
    /// `--impersonate chrome` (curl_cffi, bundled in the official exe).
    pub impersonate: bool,
    /// `--js-runtimes deno:<path>` for YouTube's JS challenges.
    pub deno: Option<&'a str>,
    /// Record a live stream (MPEG-TS so a stopped recording stays playable).
    pub live: bool,
    pub live_from_start: bool,
}

/// Header names we never forward from a sniffed request (yt-dlp/ffmpeg set
/// their own, or they'd leak hop-by-hop state).
fn forwardable_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    !matches!(
        n.as_str(),
        "host" | "content-length" | "connection" | "accept-encoding" | "range" | "cookie"
    ) && !n.starts_with("sec-")
        && !n.starts_with(':')
}

/// Build the yt-dlp argument list. The URL always comes last, after `--`.
#[must_use]
pub fn build_args(a: &Args<'_>) -> Vec<String> {
    let mut v: Vec<String> = vec![
        // Ignore any user/global config so a planted file can't inject options.
        "--ignore-config".into(),
        "--max-filesize".into(),
        MAX_DOWNLOAD_YTDLP.into(),
        "--no-part".into(),
        "--newline".into(),
        "--socket-timeout".into(),
        "20".into(),
        "-f".into(),
        a.format.unwrap_or(FORMAT_BEST).into(),
        "-S".into(),
        FORMAT_SORT.into(),
        "--merge-output-format".into(),
        MERGE.into(),
        "-o".into(),
        a.template.into(),
    ];
    if a.playlist {
        v.extend(["--yes-playlist".into(), "--playlist-items".into(), format!("1:{PLAYLIST_MAX_ITEMS}")]);
    } else {
        v.push("--no-playlist".into());
    }
    if a.live {
        v.push("--hls-use-mpegts".into());
        if a.live_from_start {
            v.push("--live-from-start".into());
        }
    }
    if let Some(loc) = a.ffmpeg_dir {
        v.extend(["--ffmpeg-location".into(), loc.into()]);
    }
    if let Some(r) = a.referer.filter(|r| is_http_url(r)) {
        v.extend(["--referer".into(), r.into()]);
    }
    for (k, val) in a.headers {
        if forwardable_header(k) && !k.contains(':') && !val.contains(['\r', '\n']) {
            v.extend(["--add-header".into(), format!("{k}:{val}")]);
        }
    }
    match &a.cookies {
        Cookies::None => {}
        Cookies::File(p) => v.extend(["--cookies".into(), p.to_string_lossy().into_owned()]),
        Cookies::Browser(b) if is_supported_browser(b) => {
            v.extend(["--cookies-from-browser".into(), b.clone()]);
        }
        Cookies::Browser(_) => {}
    }
    if a.impersonate {
        v.extend(["--impersonate".into(), "chrome".into()]);
    }
    if let Some(d) = a.deno {
        v.extend(["--js-runtimes".into(), format!("deno:{d}")]);
    }
    v.push("--".into());
    v.push(a.url.into());
    v
}

/// Browsers yt-dlp can read cookies from (lowercase names it accepts).
pub const BROWSERS: [&str; 8] = [
    "firefox", "chrome", "edge", "brave", "chromium", "opera", "vivaldi", "safari",
];

/// Whether `b` is a browser name we pass to `--cookies-from-browser`.
#[must_use]
pub fn is_supported_browser(b: &str) -> bool {
    BROWSERS.contains(&b)
}

/// Whether reading this browser's cookies is known to fail on this OS
/// (Chromium app-bound encryption on Windows since Chrome 127).
#[must_use]
pub fn browser_cookies_unreliable(b: &str) -> bool {
    cfg!(windows) && b != "firefox"
}

/// Parse a yt-dlp progress line ("[download]  12.3% of ...") into 0.0-1.0.
#[must_use]
pub fn parse_percent(line: &str) -> Option<f64> {
    if !line.contains("[download]") && !line.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let idx = line.find('%')?;
    let prefix = &line[..idx];
    let start = prefix.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    prefix[start..]
        .trim()
        .parse::<f64>()
        .ok()
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// Playlist progress ("[download] Downloading item 3 of 12").
#[must_use]
pub fn parse_playlist_item(line: &str) -> Option<(u32, u32)> {
    let i = line.find("Downloading item ")?;
    let rest = &line[i + "Downloading item ".len()..];
    let mut it = rest.split_whitespace();
    let n = it.next()?.parse().ok()?;
    (it.next()? == "of").then_some(())?;
    let total = it.next()?.parse().ok()?;
    Some((n, total))
}

/// yt-dlp failed because it has no extractor (or found no formats): the cue
/// for the scraper/sniff fallbacks and an update-and-retry.
#[must_use]
pub fn is_unsupported(stderr: &str) -> bool {
    let l = stderr.to_lowercase();
    l.contains("unsupported url") || l.contains("no video formats found")
}

/// A 403 / bot wall / TLS-fingerprint block: retry with `--impersonate`.
#[must_use]
pub fn wants_impersonation(stderr: &str) -> bool {
    let l = stderr.to_lowercase();
    l.contains("http error 403")
        || l.contains("cloudflare")
        || l.contains("anti-bot")
        || l.contains("impersonat")
        || l.contains("captcha")
}

/// The run needs the user's sign-in cookies.
#[must_use]
pub fn needs_signin(stderr: &str) -> bool {
    let l = stderr.to_lowercase();
    [
        "sign in to confirm",
        "login required",
        "requires authentication",
        "use --cookies",
        "account cookies",
        "confirm your age",
        "age-restricted",
        "members-only",
        "members only",
        "join this channel",
        "only available for registered users",
        "rate-limit reached or login",
    ]
    .iter()
    .any(|n| l.contains(n))
}

/// Map yt-dlp stderr to a clear, user-facing message. Errors that a sign-in
/// would fix carry [`SIGNIN_PREFIX`] so the UI can offer the Sign in button.
#[must_use]
pub fn friendly_error(stderr: &str, app: &str) -> String {
    let lower = stderr.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has(&["unsupported url"]) {
        util::UNSUPPORTED_SITE.to_string()
    } else if has(&["drm", "protected by digital rights"]) {
        "That video is DRM-protected, so it can't be saved.".to_string()
    } else if has(&["private video", "this video is private"]) {
        format!("{SIGNIN_PREFIX}That video is private. If your account can see it, sign in to the site in {app} and try again.")
    } else if has(&["members-only", "members only", "join this channel"]) {
        format!("{SIGNIN_PREFIX}That video is for channel members only. If you're a member, sign in in {app} and try again.")
    } else if has(&["confirm your age", "age-restricted", "age restricted", "inappropriate for some users"]) {
        format!("{SIGNIN_PREFIX}That video is age-restricted. Sign in to the site in {app} and try again.")
    } else if needs_signin(stderr) {
        format!("{SIGNIN_PREFIX}That site wants you to sign in before it will share the video. Sign in in {app}, or pick a browser's cookies in Settings.")
    } else if has(&["not available in your country", "made this video available in your country", "geo restrict", "geo-restrict", "blocked it in your country"]) {
        "That video isn't available in your region.".to_string()
    } else if has(&["this live event will begin", "premieres in", "live event will begin"]) {
        "That stream hasn't started yet. Try again once it's live or finished.".to_string()
    } else if has(&["video unavailable", "has been removed", "no longer available", "does not exist", "http error 404", "http error 410"]) {
        "That video is unavailable. It may have been removed, or the link is wrong.".to_string()
    } else if has(&["http error 429", "too many requests", "rate-limit", "rate limit"]) {
        "The site is rate-limiting downloads right now. Wait a few minutes and try again.".to_string()
    } else if has(&["http error 403", "forbidden"]) {
        format!("The site refused the download (HTTP 403). {app} already retried with an updated downloader; try Browse to play it in {app}.")
    } else if has(&["no space left", "disk full", "not enough space"]) {
        "Your disk is full. Free up some space and try again.".to_string()
    } else if has(&["requested format is not available"]) {
        "That video isn't offered in that format. Try \"Best available\".".to_string()
    } else if has(&["unable to download webpage", "failed to resolve", "getaddrinfo", "connection", "timed out", "network is unreachable", "http error"]) {
        "Couldn't reach that link. Check the URL and your connection.".to_string()
    } else {
        let summary = util::last_line(stderr);
        if summary.is_empty() {
            "Download failed.".to_string()
        } else {
            format!("Download failed: {summary}")
        }
    }
}

/// Why a yt-dlp run didn't produce files.
#[derive(Debug)]
pub enum RunError {
    Cancelled,
    /// yt-dlp failed; carries its stderr.
    Failed(String),
}

/// Run yt-dlp at `exe` with `args` (writing into `dir`). Progress lines go to
/// `progress`, other stderr to `status` hints. Racing `cancel` kills it and
/// wipes `dir`; racing `stop` (live recording) ends it but keeps what was
/// recorded. Returns the files in `dir`, sorted.
pub async fn run(
    exe: &Path,
    args: Vec<String>,
    dir: &Path,
    cancel: &AtomicBool,
    stop: Option<&AtomicBool>,
    progress: &(dyn Fn(f64) + Send + Sync),
    status: &(dyn Fn(&str) + Send + Sync),
) -> Result<Vec<PathBuf>, RunError> {
    let mut child = util::command(exe)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::Failed(format!("yt-dlp failed to start: {e}")))?;
    let mut out = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut err = BufReader::new(child.stderr.take().expect("piped")).lines();
    let mut errors = String::new();
    let never = AtomicBool::new(false);
    let stop = stop.unwrap_or(&never);
    let mut stopped = false;
    let (mut out_done, mut err_done) = (false, false);
    while !(out_done && err_done) {
        tokio::select! {
            biased;
            () = cancelled(cancel) => {
                let _ = child.kill().await;
                let _ = std::fs::remove_dir_all(dir);
                return Err(RunError::Cancelled);
            }
            () = cancelled(stop), if !stopped => {
                stopped = true;
                let _ = child.kill().await;
                break;
            }
            l = out.next_line(), if !out_done => match l {
                Ok(Some(line)) => handle_line(&line, progress, status, &mut errors),
                _ => out_done = true,
            },
            l = err.next_line(), if !err_done => match l {
                Ok(Some(line)) => handle_line(&line, progress, status, &mut errors),
                _ => err_done = true,
            },
        }
    }
    let ok = if stopped {
        true
    } else {
        child.wait().await.map(|s| s.success()).unwrap_or(false)
    };
    let files = list_files(dir);
    if !ok && !(stopped && !files.is_empty()) {
        return Err(RunError::Failed(errors));
    }
    if files.is_empty() {
        return Err(RunError::Failed(if errors.is_empty() {
            "the download produced no file".to_string()
        } else {
            errors
        }));
    }
    Ok(files)
}

fn handle_line(
    line: &str,
    progress: &(dyn Fn(f64) + Send + Sync),
    status: &(dyn Fn(&str) + Send + Sync),
    errors: &mut String,
) {
    if let Some((n, total)) = parse_playlist_item(line) {
        status(&format!("Downloading item {n} of {total}"));
    } else if let Some(p) = parse_percent(line) {
        progress(p);
    } else if line.contains("ERROR") || line.contains("WARNING") || line.starts_with("error") {
        errors.push_str(line);
        errors.push('\n');
        util::keep_tail(errors, 8192);
    } else if line.starts_with("[Merger]") || line.starts_with("[ExtractAudio]") {
        status("Finishing up…");
    }
}

/// Media files in `dir` (yt-dlp leftovers like `.part`/`.ytdl` excluded), sorted.
#[must_use]
pub fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .filter(|p| {
                    let e = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    !matches!(e, "part" | "ytdl" | "temp" | "json" | "txt")
                        && std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// `yt-dlp --version`, trimmed (e.g. `2026.09.30`).
pub async fn version(exe: &Path) -> Option<String> {
    let o = util::command(exe).arg("--version").output().await.ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Whether a yt-dlp version string supports `--js-runtimes` (2025.11.12+).
#[must_use]
pub fn supports_js_runtimes(version: &str) -> bool {
    let nums: Vec<u32> = version
        .split(['.', '-'])
        .take(3)
        .filter_map(|p| p.parse().ok())
        .collect();
    nums.len() == 3 && (nums[0], nums[1], nums[2]) >= (2025, 11, 12)
}

/// One choice in the format picker.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FormatChoice {
    /// The `-f` spec to pass back.
    pub spec: String,
    /// "1080p · mp4 · 60 fps".
    pub label: String,
    pub height: u32,
    /// Approximate size in bytes, when yt-dlp knows it.
    pub filesize: Option<u64>,
}

/// What `yt-dlp -J` says about a URL.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub title: String,
    pub extractor: String,
    pub is_live: bool,
    pub duration: Option<f64>,
    /// Entries when the URL is a playlist (flat listing).
    pub playlist_count: Option<u32>,
    /// Best first; the first entry is always "Best available".
    pub formats: Vec<FormatChoice>,
}

/// Parse `yt-dlp -J` output into picker choices: one per distinct height
/// (best video at that height, paired with best audio when video-only).
#[must_use]
pub fn parse_probe(json: &str) -> Option<ProbeInfo> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let mut info = ProbeInfo {
        title: s("title"),
        extractor: s("extractor_key"),
        is_live: v.get("is_live").and_then(serde_json::Value::as_bool).unwrap_or(false)
            || s("live_status") == "is_live",
        duration: v.get("duration").and_then(serde_json::Value::as_f64),
        playlist_count: None,
        formats: vec![FormatChoice {
            spec: FORMAT_BEST.to_string(),
            label: "Best available".to_string(),
            height: u32::MAX,
            filesize: None,
        }],
    };
    if s("_type") == "playlist" {
        info.playlist_count = v
            .get("entries")
            .and_then(|e| e.as_array())
            .map(|a| u32::try_from(a.len()).unwrap_or(u32::MAX))
            .or_else(|| v.get("playlist_count").and_then(serde_json::Value::as_u64).map(|n| u32::try_from(n).unwrap_or(u32::MAX)));
        return Some(info);
    }
    let fmts = v.get("formats").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    let mut best: std::collections::BTreeMap<u32, (f64, FormatChoice)> = Default::default();
    for f in &fmts {
        let vcodec = f.get("vcodec").and_then(|x| x.as_str()).unwrap_or("none");
        let Some(h) = f.get("height").and_then(serde_json::Value::as_u64) else { continue };
        if vcodec == "none" || h == 0 {
            continue;
        }
        let h = u32::try_from(h).unwrap_or(0);
        let id = f.get("format_id").and_then(|x| x.as_str()).unwrap_or("");
        if id.is_empty() || f.get("has_drm").and_then(serde_json::Value::as_bool).unwrap_or(false) {
            continue;
        }
        let acodec = f.get("acodec").and_then(|x| x.as_str()).unwrap_or("none");
        let tbr = f.get("tbr").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        let fps = f.get("fps").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        let ext = f.get("ext").and_then(|x| x.as_str()).unwrap_or("");
        let size = f
            .get("filesize")
            .or_else(|| f.get("filesize_approx"))
            .and_then(serde_json::Value::as_u64);
        let spec = if acodec == "none" {
            format!("{id}+ba/{id}")
        } else {
            id.to_string()
        };
        let mut label = format!("{h}p");
        if !ext.is_empty() {
            label.push_str(&format!(" · {ext}"));
        }
        if fps > 31.0 {
            label.push_str(&format!(" · {} fps", fps.round()));
        }
        let score = tbr + fps;
        let choice = FormatChoice { spec, label, height: h, filesize: size };
        match best.get(&h) {
            Some((s, _)) if *s >= score => {}
            _ => {
                best.insert(h, (score, choice));
            }
        }
    }
    info.formats.extend(best.into_values().rev().map(|(_, c)| c));
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>(url: &'a str) -> Args<'a> {
        Args { url, template: "/tmp/source.%(ext)s", ..Default::default() }
    }

    #[test]
    fn args_request_source_quality_safely() {
        let mut a = base("https://v.example/e/1");
        a.ffmpeg_dir = Some("/bin");
        a.referer = Some("https://page.example/a");
        let v = build_args(&a);
        let s = v.join(" ");
        assert!(s.contains("-f bv*[acodec=none]+ba/b/bv*+ba"));
        assert!(s.contains("-S res,fps,br"));
        assert!(s.contains("--merge-output-format mp4/mkv"));
        assert!(s.contains("--ignore-config"));
        assert!(s.contains("--no-playlist"));
        assert!(s.contains("--referer https://page.example/a"));
        assert_eq!(&v[v.len() - 2..], ["--", "https://v.example/e/1"]);
        let mut b = base("https://v.example/");
        b.referer = Some("--exec=x");
        assert!(!build_args(&b).iter().any(|x| x.contains("exec")));
    }

    #[test]
    fn args_cover_cookies_impersonation_runtime_playlist_live() {
        let headers = vec![
            ("User-Agent".to_string(), "UA".to_string()),
            ("Cookie".to_string(), "secret".to_string()),
            ("Bad:Name".to_string(), "x".to_string()),
            ("X-Evil".to_string(), "a\r\nb".to_string()),
        ];
        let mut a = base("https://v.example/p");
        a.headers = &headers;
        a.cookies = Cookies::File(PathBuf::from("/c/site.txt"));
        a.impersonate = true;
        a.deno = Some("/d/deno");
        a.playlist = true;
        a.live = true;
        a.format = Some("137+ba/137");
        let s = build_args(&a).join(" ");
        assert!(s.contains("--add-header User-Agent:UA"));
        assert!(!s.contains("secret") && !s.contains("Bad:Name") && !s.contains("X-Evil"));
        assert!(s.contains("--cookies /c/site.txt"));
        assert!(s.contains("--impersonate chrome"));
        assert!(s.contains("--js-runtimes deno:/d/deno"));
        assert!(s.contains("--yes-playlist --playlist-items 1:50"));
        assert!(!s.contains("--no-playlist"));
        assert!(s.contains("--hls-use-mpegts"));
        assert!(s.contains("-f 137+ba/137"));
        let mut b = base("https://v.example/p");
        b.cookies = Cookies::Browser("firefox".into());
        assert!(build_args(&b).join(" ").contains("--cookies-from-browser firefox"));
        b.cookies = Cookies::Browser("--exec".into());
        assert!(!build_args(&b).join(" ").contains("cookies-from-browser"));
    }

    #[test]
    fn progress_and_playlist_lines() {
        assert_eq!(parse_percent("[download]  12.5% of 10MiB at 1MiB/s"), Some(0.125));
        assert_eq!(parse_percent("[download] 100% of 3MiB"), Some(1.0));
        assert_eq!(parse_percent("[youtube] abc: Downloading webpage"), None);
        assert_eq!(parse_playlist_item("[download] Downloading item 3 of 12"), Some((3, 12)));
        assert_eq!(parse_playlist_item("[download] 3%"), None);
    }

    #[test]
    fn classifies_failures() {
        assert!(is_unsupported("ERROR: Unsupported URL: https://x.example/p"));
        assert!(is_unsupported("ERROR: [generic] p: No video formats found!"));
        assert!(!is_unsupported("ERROR: Unable to download webpage: HTTP Error 404"));
        assert!(wants_impersonation("ERROR: [generic] Got HTTP Error 403 caused by Cloudflare anti-bot challenge"));
        assert!(!wants_impersonation("ERROR: HTTP Error 404"));
        assert!(needs_signin("Sign in to confirm you're not a bot. Use --cookies-from-browser"));
        assert!(!needs_signin("HTTP Error 404: Not Found"));
    }

    #[test]
    fn friendly_errors_keep_specific_causes_and_flag_signin() {
        let f = |e: &str| friendly_error(e, "ClipSmith");
        let cases = [
            ("ERROR: [youtube] abc: Private video. Sign in if you've been granted access", "private"),
            ("ERROR: [youtube] abc: Join this channel to get access to members-only content", "members only"),
            ("ERROR: [youtube] abc: Sign in to confirm your age.", "age-restricted"),
            ("ERROR: [youtube] abc: Sign in to confirm you're not a bot. Use --cookies-from-browser", "sign in"),
            ("ERROR: [x] 1: The uploader has not made this video available in your country", "region"),
            ("ERROR: [y] 2: This video is DRM protected", "DRM"),
            ("ERROR: [youtube] abc: This live event will begin in 3 hours.", "hasn't started"),
            ("ERROR: [youtube] abc: Video unavailable. This video has been removed by the uploader", "unavailable"),
            ("ERROR: unable to download video data: HTTP Error 429: Too Many Requests", "rate-limiting"),
            ("ERROR: unable to download video data: HTTP Error 403: Forbidden", "403"),
            ("ERROR: unable to write data: [Errno 28] No space left on device", "disk is full"),
            ("ERROR: [vimeo] 1: Requested format is not available", "format"),
            ("ERROR: Unable to download webpage: <urlopen error [Errno 11001] getaddrinfo failed>", "Couldn't reach"),
        ];
        for (e, want) in cases {
            let got = f(e);
            assert!(got.contains(want), "{e:?} -> {got:?} (wanted {want:?})");
        }
        assert!(f("Sign in to confirm you're not a bot").starts_with(SIGNIN_PREFIX));
        assert!(!f("HTTP Error 404").starts_with(SIGNIN_PREFIX));
        assert_eq!(f(""), "Download failed.");
        assert_eq!(f("weird\nlast line \n"), "Download failed: last line");
    }

    #[test]
    fn js_runtime_support_by_version() {
        assert!(supports_js_runtimes("2025.11.12"));
        assert!(supports_js_runtimes("2026.09.30"));
        assert!(!supports_js_runtimes("2025.10.22"));
        assert!(!supports_js_runtimes("garbage"));
    }

    #[test]
    fn browsers() {
        assert!(is_supported_browser("firefox"));
        assert!(!is_supported_browser("netscape"));
        assert!(!browser_cookies_unreliable("firefox"));
    }

    #[test]
    fn parses_probe_formats_per_height() {
        let j = r#"{"title":"T","extractor_key":"Youtube","duration":12.5,"is_live":false,
          "formats":[
            {"format_id":"18","vcodec":"avc1","acodec":"mp4a","height":360,"ext":"mp4","tbr":500},
            {"format_id":"137","vcodec":"avc1","acodec":"none","height":1080,"ext":"mp4","tbr":4000,"fps":30,"filesize":1000},
            {"format_id":"248","vcodec":"vp9","acodec":"none","height":1080,"ext":"webm","tbr":3000,"fps":30},
            {"format_id":"299","vcodec":"avc1","acodec":"none","height":1080,"ext":"mp4","tbr":6000,"fps":60},
            {"format_id":"140","vcodec":"none","acodec":"mp4a","ext":"m4a"},
            {"format_id":"drm","vcodec":"avc1","acodec":"none","height":2160,"has_drm":true}
          ]}"#;
        let p = parse_probe(j).unwrap();
        assert_eq!(p.title, "T");
        assert_eq!(p.formats[0].label, "Best available");
        assert_eq!(p.formats[1].height, 1080);
        assert_eq!(p.formats[1].spec, "299+ba/299");
        assert!(p.formats[1].label.contains("60 fps"));
        assert_eq!(p.formats[2].spec, "18");
        assert_eq!(p.formats.len(), 3);
        let pl = parse_probe(r#"{"_type":"playlist","title":"L","entries":[{},{},{}]}"#).unwrap();
        assert_eq!(pl.playlist_count, Some(3));
        assert!(parse_probe("not json").is_none());
    }
}
