//! Small shared helpers: URL validation, cancel racing, process spawning.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// User-facing message returned when a download is cancelled. Frontends match
/// on this to swallow the error instead of showing it.
pub const DOWNLOAD_CANCELLED: &str = "Download cancelled.";

/// Message for a page nothing could find a video on.
pub const UNSUPPORTED_SITE: &str = "This site isn't supported. Try the Browse button to open the page and play the video, or download it yourself and open the file.";

/// Prefix on an error that the UI should turn into a "Sign in" button.
pub const SIGNIN_PREFIX: &str = "[signin] ";

/// Sanity ceiling for a URL download: a runaway guard against filling the disk.
pub const MAX_DOWNLOAD_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// The same ceiling in yt-dlp's `--max-filesize` syntax.
pub const MAX_DOWNLOAD_YTDLP: &str = "16G";

/// Browser-like UA: many sites serve a stripped page (or a 403) to bots.
pub const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

/// Extensions treated as a direct video-file link.
pub const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mov", "mkv", "webm", "avi", "m4v"];

/// Whether `url` is http/https: the only schemes we fetch. Blocks `file://`
/// and `--flag`-shaped strings before they reach yt-dlp or ffmpeg.
#[must_use]
pub fn is_http_url(url: &str) -> bool {
    url::Url::parse(url)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

/// `scheme://host[:port]` of a URL, for an `Origin` header.
#[must_use]
pub fn origin_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let origin = u.origin().ascii_serialization();
    (origin != "null").then_some(origin)
}

/// Lowercased host of `url`, without a leading `www.`.
#[must_use]
pub fn host_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let h = u.host_str()?.to_ascii_lowercase();
    Some(h.strip_prefix("www.").unwrap_or(&h).to_string())
}

/// If the URL's path ends in a known video extension, that extension.
#[must_use]
pub fn direct_media_extension(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let ext = Path::new(parsed.path())
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)?;
    VIDEO_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

/// `Some("m3u8")`/`Some("mpd")` for a direct manifest link.
#[must_use]
pub fn direct_manifest_extension(url: &str) -> Option<&'static str> {
    let parsed = url::Url::parse(url).ok()?;
    let ext = Path::new(parsed.path())
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "m3u8" => Some("m3u8"),
        "mpd" => Some("mpd"),
        _ => None,
    }
}

/// Whether `url` is a YouTube page (yt-dlp needs a JS runtime for those).
#[must_use]
pub fn is_youtube(url: &str) -> bool {
    host_of(url).is_some_and(|h| {
        h == "youtube.com"
            || h.ends_with(".youtube.com")
            || h == "youtu.be"
            || h == "youtube-nocookie.com"
    })
}

/// Resolve once `flag` is set; race this against a download await.
pub async fn cancelled(flag: &AtomicBool) {
    while !flag.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    }
}

/// A tokio command that never flashes a console window on Windows.
#[must_use]
pub fn command(program: &Path) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.kill_on_drop(true);
    c
}

/// The last non-empty line of `s` (a short failure summary).
#[must_use]
pub fn last_line(s: &str) -> &str {
    s.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("")
}

/// Keep only the last `max` bytes of `s` (on a char boundary).
pub fn keep_tail(s: &mut String, max: usize) {
    if s.len() > max {
        let cut = s.len() - max;
        let cut = (cut..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(0);
        s.drain(..cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_and_https_urls_pass() {
        assert!(is_http_url("https://example.com/v.mp4"));
        assert!(is_http_url("http://host/path?q=1"));
        assert!(!is_http_url("file:///etc/passwd"));
        assert!(!is_http_url("ftp://host/x"));
        assert!(!is_http_url("--config-location=/tmp/x"));
        assert!(!is_http_url("just some text"));
    }

    #[test]
    fn classifies_direct_links() {
        assert_eq!(direct_media_extension("https://h/x.MP4").as_deref(), Some("mp4"));
        assert_eq!(direct_media_extension("https://h/a/b.webm?t=1").as_deref(), Some("webm"));
        assert_eq!(direct_media_extension("https://youtube.com/watch?v=x"), None);
        assert_eq!(direct_manifest_extension("https://c/x/master.M3U8?tok=1"), Some("m3u8"));
        assert_eq!(direct_manifest_extension("https://c/m.mpd"), Some("mpd"));
        assert_eq!(direct_manifest_extension("https://c/m.mp4"), None);
    }

    #[test]
    fn hosts_and_origins() {
        assert_eq!(host_of("https://www.Example.com/x").as_deref(), Some("example.com"));
        assert_eq!(origin_of("https://a.example:8443/x/y?z").as_deref(), Some("https://a.example:8443"));
        assert_eq!(origin_of("not a url"), None);
        assert!(is_youtube("https://youtu.be/abc"));
        assert!(is_youtube("https://m.youtube.com/watch?v=1"));
        assert!(!is_youtube("https://notyoutube.com/"));
    }

    #[test]
    fn tail_helpers() {
        assert_eq!(last_line("a\nlast \n\n"), "last");
        let mut s = "ééééé".to_string();
        keep_tail(&mut s, 3);
        assert!(s.len() <= 3 && s.chars().all(|c| c == 'é'));
    }
}
