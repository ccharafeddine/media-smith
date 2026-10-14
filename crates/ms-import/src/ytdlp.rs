//! yt-dlp routing helpers (ported from ClipSmith `commands.rs`).

use std::path::Path;

/// Extensions treated as a direct video-file link.
pub const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mov", "mkv", "webm", "avi", "m4v"];

/// Source quality: best video-only + best audio, else best muxed file, else any
/// video + audio. Video-only first avoids a duplicate audio track.
pub const YTDLP_FORMAT: &str = "bv*[acodec=none]+ba/b/bv*+ba";
/// Highest resolution, then frame rate, then bitrate.
pub const YTDLP_SORT: &str = "res,fps,br";
/// mp4 when codecs fit, mkv otherwise: never downgrade to fit a container.
pub const YTDLP_MERGE: &str = "mp4/mkv";

/// Only http(s) URLs are ever handed to yt-dlp or the scraper.
pub fn is_http_url(s: &str) -> bool {
    url::Url::parse(s)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

/// If the URL's path ends in a known video extension, return it (lowercased).
pub fn direct_media_extension(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let ext = Path::new(parsed.path())
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)?;
    VIDEO_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

/// True when yt-dlp has no extractor for the page: the cue for the scraper.
pub fn ytdlp_unsupported(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("unsupported url") || lower.contains("no video formats found")
}

/// Keep only the final path component of a server-supplied filename.
pub fn safe_filename(name: &str) -> String {
    let last = name
        .rsplit(['/', '\\'])
        .find(|s| !s.is_empty() && *s != ".." && *s != ".")
        .unwrap_or("");
    if last.is_empty() {
        "clip.mp4".into()
    } else {
        last.into()
    }
}

/// Build yt-dlp args for a source-quality download. `--` ends option parsing
/// so a URL can never be read as a flag.
pub fn ytdlp_args(output_template: &str, url: &str, ffmpeg_dir: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--no-playlist".into(),
        "--no-config".into(),
        "-f".into(),
        YTDLP_FORMAT.into(),
        "-S".into(),
        YTDLP_SORT.into(),
        "--merge-output-format".into(),
        YTDLP_MERGE.into(),
        "-o".into(),
        output_template.into(),
        "--newline".into(),
    ];
    if let Some(d) = ffmpeg_dir {
        a.push("--ffmpeg-location".into());
        a.push(d.into());
    }
    a.push("--".into());
    a.push(url.into());
    a
}

/// What the importer should do with a URL.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    Direct(String),
    Ytdlp,
}

/// Route: direct HTTP fetch for file links, yt-dlp (then scrape) otherwise.
pub fn route(url: &str) -> Option<Route> {
    if !is_http_url(url) {
        return None;
    }
    Some(match direct_media_extension(url) {
        Some(ext) => Route::Direct(ext),
        None => Route::Ytdlp,
    })
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
    fn safe_filename_keeps_only_the_final_component() {
        assert_eq!(safe_filename("clip.mp4"), "clip.mp4");
        assert_eq!(safe_filename("../../etc/evil"), "evil");
        assert_eq!(safe_filename("/abs/path/x.webm"), "x.webm");
        assert_eq!(safe_filename(""), "clip.mp4");
        assert_eq!(safe_filename("../"), "clip.mp4");
    }

    #[test]
    fn direct_media_extension_classifies_links() {
        assert_eq!(
            direct_media_extension("https://h/x.MP4"),
            Some("mp4".to_string())
        );
        assert_eq!(
            direct_media_extension("https://h/a/b.webm?t=1"),
            Some("webm".to_string())
        );
        assert_eq!(
            direct_media_extension("https://youtube.com/watch?v=x"),
            None
        );
    }

    #[test]
    fn ytdlp_unsupported_detects_fallback_cases() {
        assert!(ytdlp_unsupported(
            "ERROR: Unsupported URL: https://x.example/p"
        ));
        assert!(ytdlp_unsupported(
            "ERROR: [generic] p: No video formats found!; please report this issue"
        ));
        assert!(!ytdlp_unsupported(
            "ERROR: Unable to download webpage: HTTP Error 404"
        ));
        assert!(!ytdlp_unsupported(""));
    }

    #[test]
    fn ytdlp_args_request_source_quality_safely() {
        let a = ytdlp_args("/tmp/source.%(ext)s", "https://x/v", Some("/bin"));
        let fi = a.iter().position(|s| s == "-f").unwrap();
        assert_eq!(a[fi + 1], YTDLP_FORMAT);
        assert!(!a.iter().any(|s| s.contains("height<=")));
        assert_eq!(a[a.len() - 2], "--");
        assert_eq!(a.last().unwrap(), "https://x/v");
    }

    #[test]
    fn routes_urls() {
        assert_eq!(route("https://h/x.mov"), Some(Route::Direct("mov".into())));
        assert_eq!(route("https://example.com/watch/1"), Some(Route::Ytdlp));
        assert_eq!(route("file:///x.mp4"), None);
    }
}
