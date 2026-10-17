//! Sniff mode, pure parts: classify media requests a real webview made,
//! detect DRM, and group hits into download candidates.
//!
//! Requests come from the app's own embedded browser (WebView2 network events
//! on Windows, an injected script elsewhere). Nothing here captures packets
//! or intercepts TLS. DRM-protected media is detected and reported as such,
//! never decrypted.

use serde::{Deserialize, Serialize};

/// What a sniffed request points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum HitKind {
    Hls,
    Dash,
    File,
    /// A media segment (`.ts`, `.m4s`, ranged MP4 chunk): proof a stream is
    /// playing, but not downloadable on its own.
    Segment,
}

/// One media request seen in the browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub url: String,
    pub kind: HitKind,
    /// The page that made the request (Referer for the download).
    #[serde(default)]
    pub page_url: String,
    /// Request headers worth replaying (UA, Referer, Origin, auth tokens).
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Content-Length when known.
    #[serde(default)]
    pub size: Option<u64>,
    /// Best resolution hint, 0 if unknown.
    #[serde(default)]
    pub height: u32,
    /// Live stream (HLS without `#EXT-X-ENDLIST`).
    #[serde(default)]
    pub live: bool,
    /// Set once the manifest was checked and found DRM-protected.
    #[serde(default)]
    pub drm: bool,
    /// Duration in seconds when known (HLS `#EXTINF` sum, DASH
    /// `mediaPresentationDuration`).
    #[serde(default)]
    pub duration: Option<f64>,
    /// `size` is an estimate (duration × bitrate), not a byte count.
    #[serde(default)]
    pub estimated: bool,
    /// Served from a well-known ad / tracker host.
    #[serde(default)]
    pub likely_ad: bool,
    /// Set by [`Collector::candidates_with`]: below the size / duration
    /// minimum or an ad. Shown collapsed in the UI, never dropped.
    #[serde(default)]
    pub hidden: bool,
    /// Why it's hidden ("ad", "small", "short").
    #[serde(default)]
    pub hidden_reason: Option<String>,
}

impl Hit {
    /// A fresh, unannotated hit.
    #[must_use]
    pub fn new(url: String, kind: HitKind, page_url: String) -> Self {
        let likely_ad = is_ad_host(&url);
        Self {
            url,
            kind,
            page_url,
            headers: vec![],
            size: None,
            height: 0,
            live: false,
            drm: false,
            duration: None,
            estimated: false,
            likely_ad,
            hidden: false,
            hidden_reason: None,
        }
    }
}

const SEGMENT_EXTS: [&str; 5] = ["ts", "m4s", "m4v_seg", "aac", "m4a"];
const FILE_EXTS: [&str; 6] = ["mp4", "webm", "mov", "m4v", "mkv", "mp3"];

/// Hosts whose media requests are ads/trackers, never the video you want.
/// Their media is kept but marked `likely_ad` (and hidden by default).
const AD_HOSTS: [&str; 22] = [
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "imasdk.googleapis.com",
    "adnxs.com",
    "moatads.com",
    "springserve.com",
    "spotxchange.com",
    "teads.tv",
    "adsrvr.org",
    "2mdn.net",
    "amazon-adsystem.com",
    "adsafeprotected.com",
    "serving-sys.com",
    "innovid.com",
    "flashtalking.com",
    "fwmrm.net",
    "freewheel.tv",
    "pubmatic.com",
    "rubiconproject.com",
    "smartadserver.com",
    "yieldmo.com",
];

/// Whether `url` is served from a known ad / tracker host.
#[must_use]
pub fn is_ad_host(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    AD_HOSTS
        .iter()
        .any(|a| host == *a || host.ends_with(&format!(".{a}")))
}

/// Classify a request by URL and (when known) response Content-Type.
#[must_use]
pub fn classify(url: &str, content_type: Option<&str>) -> Option<HitKind> {
    let u = url::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    let ct = ct.split(';').next().unwrap_or("").trim();
    match ct {
        "application/vnd.apple.mpegurl"
        | "application/x-mpegurl"
        | "audio/mpegurl"
        | "audio/x-mpegurl" => return Some(HitKind::Hls),
        "application/dash+xml" => return Some(HitKind::Dash),
        "video/mp2t" | "video/iso.segment" => return Some(HitKind::Segment),
        _ => {}
    }
    let path = u.path().to_ascii_lowercase();
    let ext = path
        .rsplit('/')
        .next()
        .and_then(|f| f.rsplit_once('.'))
        .map(|(_, e)| e.to_string());
    match ext.as_deref() {
        Some("m3u8") => return Some(HitKind::Hls),
        Some("mpd") => return Some(HitKind::Dash),
        Some(e) if SEGMENT_EXTS.contains(&e) => return Some(HitKind::Segment),
        _ => {}
    }
    let ranged = u
        .query_pairs()
        .any(|(k, _)| k == "range" || k == "bytestart");
    if ct.starts_with("video/")
        || ct.starts_with("audio/")
        || ext.as_deref().is_some_and(|e| FILE_EXTS.contains(&e))
    {
        // A byte-range chunk of a progressive file is still that file.
        if ranged && host.ends_with("googlevideo.com") {
            return Some(HitKind::Segment);
        }
        return Some(HitKind::File);
    }
    None
}

/// Whether a manifest (HLS or DASH text) declares DRM: Widevine, PlayReady,
/// FairPlay, SAMPLE-AES or CENC `ContentProtection`. Plain AES-128 HLS (the
/// key is served to every player) is ordinary HLS and is not DRM.
#[must_use]
pub fn manifest_drm(text: &str) -> bool {
    let l = text.to_ascii_lowercase();
    if l.contains("<contentprotection") {
        return true;
    }
    l.lines().any(|line| {
        (line.starts_with("#ext-x-key") || line.starts_with("#ext-x-session-key"))
            && (line.contains("method=sample-aes")
                || line.contains("com.apple.streamingkeydelivery")
                || line.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed")
                || line.contains("9a04f079-9840-4286-ab92-e65be0885f95")
                || line.contains("widevine")
                || line.contains("playready"))
    })
}

/// Whether an HLS media/master text looks live (a media playlist with no
/// `#EXT-X-ENDLIST`, or `#EXT-X-PLAYLIST-TYPE:EVENT`). DASH: `type="dynamic"`.
#[must_use]
pub fn manifest_live(text: &str) -> bool {
    let l = text.to_ascii_lowercase();
    if l.contains("<mpd") {
        return l.contains("type=\"dynamic\"");
    }
    let media = l.contains("#extinf");
    (media && !l.contains("#ext-x-endlist")) || l.contains("#ext-x-playlist-type:event")
}

/// Highest `height` in a DASH manifest's video representations, 0 if none.
#[must_use]
pub fn dash_max_height(mpd: &str) -> u32 {
    let mut best = 0;
    let mut rest = mpd;
    while let Some(i) = rest.find("height=\"") {
        rest = &rest[i + 8..];
        if let Some(end) = rest.find('"') {
            if let Ok(h) = rest[..end].parse::<u32>() {
                best = best.max(h);
            }
        }
    }
    best
}

/// Total duration of an HLS media playlist (sum of `#EXTINF`), if any.
#[must_use]
pub fn hls_duration(text: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("#EXTINF:") {
            let n = rest.split(',').next().unwrap_or("").trim();
            if let Ok(d) = n.parse::<f64>() {
                if d.is_finite() && d >= 0.0 {
                    total += d;
                    any = true;
                }
            }
        }
    }
    any.then_some(total)
}

/// Sum of `#EXT-X-BYTERANGE` lengths in an HLS media playlist (exact size
/// for byte-range playlists), if every segment declares one.
#[must_use]
pub fn hls_byterange_total(text: &str) -> Option<u64> {
    let (mut total, mut ranges, mut segs) = (0u64, 0usize, 0usize);
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("#EXTINF:") {
            segs += 1;
        } else if let Some(r) = l.strip_prefix("#EXT-X-BYTERANGE:") {
            if let Ok(n) = r.split('@').next().unwrap_or("").trim().parse::<u64>() {
                total += n;
                ranges += 1;
            }
        }
    }
    (segs > 0 && ranges == segs).then_some(total)
}

/// ISO 8601 duration as used by DASH (`PT1H2M3.5S`), in seconds.
#[must_use]
pub fn iso_duration_secs(s: &str) -> Option<f64> {
    let s = s.trim().strip_prefix('P')?;
    let (date, time) = s.split_once('T').unwrap_or((s, ""));
    let mut total = 0.0;
    let mut num = String::new();
    for (part, is_time) in [(date, false), (time, true)] {
        for c in part.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
                continue;
            }
            let v: f64 = num.parse().ok()?;
            num.clear();
            total += v * match (c, is_time) {
                ('D', false) => 86_400.0,
                ('H', true) => 3_600.0,
                ('M', true) => 60.0,
                ('S', true) => 1.0,
                _ => return None,
            };
        }
        if !num.is_empty() {
            return None;
        }
    }
    Some(total)
}

/// `mediaPresentationDuration` of a DASH manifest, in seconds.
#[must_use]
pub fn dash_duration(mpd: &str) -> Option<f64> {
    let i = mpd.find("mediaPresentationDuration=\"")?;
    let rest = &mpd[i + 27..];
    iso_duration_secs(&rest[..rest.find('"')?])
}

/// Bitrate (bits/s) of the best DASH rendition: the highest video
/// `bandwidth` plus the highest audio one when audio is separate.
#[must_use]
pub fn dash_bandwidth(mpd: &str) -> u64 {
    let (mut video, mut audio, mut other) = (0u64, 0u64, 0u64);
    let mut set_kind = "";
    for tag in mpd.split('<').skip(1) {
        let name = tag
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("");
        let attr = |a: &str| -> Option<&str> {
            let k = format!("{a}=\"");
            let i = tag.find(&k)?;
            let r = &tag[i + k.len()..];
            r.find('"').map(|e| &r[..e])
        };
        let mime = attr("mimeType")
            .or_else(|| attr("contentType"))
            .unwrap_or("");
        if name == "AdaptationSet" {
            set_kind = if mime.contains("audio") {
                "audio"
            } else if mime.contains("video") {
                "video"
            } else {
                ""
            };
        } else if name == "Representation" {
            let Some(bw) = attr("bandwidth").and_then(|b| b.parse::<u64>().ok()) else {
                continue;
            };
            let kind = if mime.contains("audio") {
                "audio"
            } else if mime.contains("video") {
                "video"
            } else {
                set_kind
            };
            match kind {
                "audio" => audio = audio.max(bw),
                "video" => video = video.max(bw),
                _ => other = other.max(bw),
            }
        }
    }
    if video == 0 {
        video = other;
    }
    video + audio
}

/// Estimated bytes for `secs` of media at `bits_per_sec`.
#[must_use]
pub fn estimate_bytes(bits_per_sec: u64, secs: f64) -> Option<u64> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    (bits_per_sec > 0 && secs.is_finite() && secs > 0.0)
        .then(|| (bits_per_sec as f64 / 8.0 * secs) as u64)
}

/// Total size from a `Content-Range: bytes 0-99/12345` header (`*` = unknown).
#[must_use]
pub fn content_range_total(v: &str) -> Option<u64> {
    v.rsplit_once('/')?.1.trim().parse().ok()
}

/// What to hide in the Browse list. `min_bytes == 0` turns filtering off
/// (ads are still marked, and hidden only while filtering is on).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SniffFilter {
    pub min_bytes: u64,
    pub min_secs: f64,
}

/// Default minimum size in MB (see the Browse settings).
pub const DEFAULT_MIN_MB: u32 = 10;
/// Media shorter than this is hidden while the filter is on (ads are 6-30 s).
pub const MIN_SECS: f64 = 30.0;
/// Size presets offered in the UI (MB, 0 = off).
pub const MIN_MB_PRESETS: [u32; 6] = [0, 5, 10, 25, 50, 100];

impl SniffFilter {
    #[must_use]
    pub fn from_mb(mb: u32) -> Self {
        Self {
            min_bytes: u64::from(mb) * 1024 * 1024,
            min_secs: if mb == 0 { 0.0 } else { MIN_SECS },
        }
    }

    /// Why `h` should be hidden, if it should. Unknown size and unknown
    /// duration never hide a hit: it might be the real video.
    #[must_use]
    pub fn reason(&self, h: &Hit) -> Option<&'static str> {
        if self.min_bytes == 0 || h.live {
            return None;
        }
        if h.likely_ad {
            return Some("ad");
        }
        if h.size.is_some_and(|s| s < self.min_bytes) {
            return Some("small");
        }
        if h.duration.is_some_and(|d| d < self.min_secs) {
            return Some("short");
        }
        None
    }
}

impl Default for SniffFilter {
    fn default() -> Self {
        Self::from_mb(DEFAULT_MIN_MB)
    }
}

/// Sort key: visible first, DRM last, then largest (estimated) size, then
/// longest, then tallest; unknown sizes after known ones.
fn rank(
    h: &Hit,
) -> (
    bool,
    bool,
    bool,
    std::cmp::Reverse<u64>,
    std::cmp::Reverse<u64>,
    std::cmp::Reverse<u32>,
    u8,
) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dur = h.duration.map_or(0, |d| (d * 1000.0) as u64);
    let kind = match h.kind {
        HitKind::Hls => 0,
        HitKind::Dash => 1,
        _ => 2,
    };
    (
        h.hidden,
        h.drm,
        h.size.is_none(),
        std::cmp::Reverse(h.size.unwrap_or(0)),
        std::cmp::Reverse(dur),
        std::cmp::Reverse(h.height),
        kind,
    )
}

/// Collects hits from one browsing session and ranks them.
#[derive(Debug, Default, Clone)]
pub struct Collector {
    hits: Vec<Hit>,
    /// The page used EME (`requestMediaKeySystemAccess`): its video is DRM.
    pub page_drm: bool,
}

/// Most hits kept per session (segment spam is collapsed anyway).
const MAX_HITS: usize = 200;

impl Collector {
    /// Add a hit; returns true when it's new and worth showing.
    pub fn add(&mut self, hit: Hit) -> bool {
        if let Some(h) = self
            .hits
            .iter_mut()
            .find(|h| same_resource(&h.url, &hit.url))
        {
            // A ranged response reports a chunk; the largest seen (or the
            // Content-Range total) is the best size we have.
            let grew = hit.size.is_some_and(|s| h.size.is_none_or(|o| s > o));
            if grew {
                h.size = hit.size;
                h.estimated = false;
            }
            h.height = h.height.max(hit.height);
            if h.headers.is_empty() {
                h.headers = hit.headers;
            }
            return grew;
        }
        if self.hits.len() >= MAX_HITS {
            return false;
        }
        let show = hit.kind != HitKind::Segment;
        self.hits.push(hit);
        show
    }

    /// Record a size and/or duration learned later (probe or manifest).
    /// `estimated` sizes never replace a real byte count.
    pub fn set_size(
        &mut self,
        url: &str,
        size: Option<u64>,
        estimated: bool,
        duration: Option<f64>,
    ) {
        if let Some(h) = self.hits.iter_mut().find(|h| h.url == url) {
            if let Some(s) = size {
                if h.size.is_none()
                    || (h.estimated && !estimated)
                    || (h.estimated == estimated && s > h.size.unwrap_or(0))
                {
                    h.size = Some(s);
                    h.estimated = estimated;
                }
            }
            if duration.is_some() {
                h.duration = duration;
            }
        }
    }

    /// Mark a hit as checked (DRM / live / height) after reading its manifest.
    pub fn annotate(&mut self, url: &str, drm: bool, live: bool, height: u32) {
        if let Some(h) = self.hits.iter_mut().find(|h| h.url == url) {
            h.drm = drm;
            h.live = live;
            h.height = h.height.max(height);
        }
    }

    /// Downloadable candidates with the default filter.
    #[must_use]
    pub fn candidates(&self) -> Vec<Hit> {
        self.candidates_with(&SniffFilter::default())
    }

    /// Downloadable candidates, largest first (estimated size for streams,
    /// then duration). Hits the filter catches are kept, marked `hidden`, and
    /// sorted after the rest. Segments and variant playlists are dropped
    /// (their master playlist stands for them).
    #[must_use]
    pub fn candidates_with(&self, f: &SniffFilter) -> Vec<Hit> {
        let mut v: Vec<Hit> = self
            .hits
            .iter()
            .filter(|h| h.kind != HitKind::Segment)
            .filter(|h| !is_child_playlist(h, &self.hits))
            .cloned()
            .map(|mut h| {
                let r = f.reason(&h);
                h.hidden = r.is_some();
                h.hidden_reason = r.map(str::to_string);
                h
            })
            .collect();
        v.sort_by_key(rank);
        v
    }

    /// Whether `url` (ignoring volatile query params) was already seen.
    #[must_use]
    pub fn has(&self, url: &str) -> bool {
        self.hits.iter().any(|h| same_resource(&h.url, url))
    }

    /// Whether any media at all was seen (including segments).
    #[must_use]
    pub fn saw_media(&self) -> bool {
        !self.hits.is_empty()
    }

    pub fn clear(&mut self) {
        self.hits.clear();
        self.page_drm = false;
    }
}

/// Same resource ignoring volatile query params (tokens, ranges, cache busters).
fn same_resource(a: &str, b: &str) -> bool {
    let strip = |s: &str| -> String {
        url::Url::parse(s)
            .map(|mut u| {
                u.set_query(None);
                u.set_fragment(None);
                u.to_string()
            })
            .unwrap_or_else(|_| s.to_string())
    };
    a == b || strip(a) == strip(b)
}

/// An HLS variant playlist fetched by the player after its master playlist:
/// same directory as another HLS hit that was seen first. Keep the master.
fn is_child_playlist(h: &Hit, all: &[Hit]) -> bool {
    if h.kind != HitKind::Hls {
        return false;
    }
    let dir = |s: &str| {
        s.rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default()
    };
    let Some(pos) = all.iter().position(|x| x.url == h.url) else {
        return false;
    };
    all[..pos].iter().any(|x| {
        x.kind == HitKind::Hls && (dir(&x.url) == dir(&h.url) || h.url.starts_with(&dir(&x.url)))
    })
}

/// Script injected into every page of the app's browse window (all
/// platforms). It reports media URLs the page requests (fetch, XHR, media
/// element sources, resource timing) and whether the page uses EME (DRM) to
/// the app via the single `sniff_report` command. In hidden-sniff mode
/// (`window.__SMITH_SNIFF_AUTOPLAY__`) it also mutes and plays videos.
pub const INJECT_JS: &str = r#"
(function () {
  if (window.__smithSniff) return; window.__smithSniff = true;
  var seen = {}, queue = [], drm = false, timer = null;
  var re = /\.(m3u8|mpd|mp4|webm|m4v|mov|mkv|ts|m4s)(\?|#|$)/i;
  function abs(u) { try { return new URL(u, location.href).href; } catch (e) { return null; } }
  function push(u, ct) {
    if (!u || typeof u !== 'string') return;
    u = abs(u); if (!u || !/^https?:/i.test(u)) return;
    if (!re.test(u) && !(ct && /^(video|audio)\/|mpegurl|dash\+xml/i.test(ct))) return;
    if (seen[u]) return; seen[u] = 1; queue.push({ url: u, contentType: ct || null });
    schedule();
  }
  function schedule() { if (!timer) timer = setTimeout(flush, 600); }
  function flush() {
    timer = null;
    if (!queue.length && !drm) return;
    var batch = queue.splice(0, 50);
    try {
      var inv = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;
      if (inv) inv('sniff_report', { pageUrl: location.href, hits: batch, drm: drm });
    } catch (e) {}
    if (queue.length) schedule();
  }
  var of = window.fetch;
  if (of) window.fetch = function (i, o) {
    var u = typeof i === 'string' ? i : (i && i.url);
    var p = of.apply(this, arguments);
    try { p.then(function (r) { push(r.url || u, r.headers && r.headers.get('content-type')); }, function () {}); } catch (e) {}
    return p;
  };
  var oo = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (m, u) {
    var x = this;
    try { x.addEventListener('load', function () { push(x.responseURL || u, x.getResponseHeader('content-type')); }); } catch (e) {}
    return oo.apply(this, arguments);
  };
  function scan() {
    document.querySelectorAll('video,audio,source').forEach(function (v) {
      [v.currentSrc, v.src, v.getAttribute && v.getAttribute('src')].forEach(function (s) {
        if (s && !/^blob:|^data:/.test(s)) push(s, v.tagName === 'SOURCE' ? v.type : 'video/unknown');
      });
      if (window.__SMITH_SNIFF_AUTOPLAY__ && v.tagName === 'VIDEO' && v.paused) { try { v.muted = true; v.play(); } catch (e) {} }
    });
    try { performance.getEntriesByType('resource').forEach(function (e) { push(e.name, null); }); } catch (e) {}
  }
  try { new PerformanceObserver(function (l) { l.getEntries().forEach(function (e) { push(e.name, null); }); }).observe({ type: 'resource', buffered: true }); } catch (e) {}
  if (navigator.requestMediaKeySystemAccess) {
    var ok = navigator.requestMediaKeySystemAccess.bind(navigator);
    navigator.requestMediaKeySystemAccess = function () { drm = true; schedule(); return ok.apply(null, arguments); };
  }
  setInterval(scan, 1500);
  document.addEventListener('DOMContentLoaded', scan);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(url: &str, kind: HitKind) -> Hit {
        Hit::new(url.into(), kind, String::new())
    }

    #[test]
    fn classifies_by_type_and_extension() {
        assert_eq!(
            classify("https://c/x/master.m3u8?t=1", None),
            Some(HitKind::Hls)
        );
        assert_eq!(
            classify("https://c/x/stream", Some("application/vnd.apple.mpegURL")),
            Some(HitKind::Hls)
        );
        assert_eq!(classify("https://c/m.mpd", None), Some(HitKind::Dash));
        assert_eq!(classify("https://c/seg-1.ts", None), Some(HitKind::Segment));
        assert_eq!(
            classify("https://c/chunk-1.m4s", None),
            Some(HitKind::Segment)
        );
        assert_eq!(classify("https://c/v.mp4", None), Some(HitKind::File));
        assert_eq!(
            classify("https://c/blob", Some("video/mp4; codecs=x")),
            Some(HitKind::File)
        );
        assert_eq!(classify("https://c/page.html", Some("text/html")), None);
        assert_eq!(
            classify("https://pubads.g.doubleclick.net/ad.mp4", None),
            Some(HitKind::File)
        );
        assert!(is_ad_host("https://pubads.g.doubleclick.net/ad.mp4"));
        assert!(is_ad_host("https://imasdk.googleapis.com/js/x.mp4"));
        assert!(!is_ad_host("https://notdoubleclick.net/v.mp4"));
        assert_eq!(classify("blob:https://x/1", None), None);
        assert_eq!(
            classify(
                "https://r1.googlevideo.com/videoplayback?range=0-100",
                Some("video/mp4")
            ),
            Some(HitKind::Segment)
        );
    }

    #[test]
    fn detects_drm_but_not_plain_aes() {
        assert!(manifest_drm("#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n"));
        assert!(manifest_drm("#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES-CTR,KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"\n"));
        assert!(manifest_drm("<MPD><AdaptationSet><ContentProtection schemeIdUri=\"urn:mpeg:dash:mp4protection:2011\"/></AdaptationSet></MPD>"));
        assert!(!manifest_drm(
            "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"https://k/key\"\n#EXTINF:4,\na.ts\n"
        ));
        assert!(!manifest_drm(
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n"
        ));
    }

    #[test]
    fn live_and_dash_height() {
        assert!(manifest_live("#EXTM3U\n#EXTINF:4,\na.ts\n"));
        assert!(!manifest_live(
            "#EXTM3U\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n"
        ));
        assert!(!manifest_live(
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n"
        ));
        assert!(manifest_live("<MPD type=\"dynamic\">"));
        assert!(!manifest_live("<MPD type=\"static\">"));
        assert_eq!(
            dash_max_height("<Representation height=\"720\"/><Representation height=\"1080\"/>"),
            1080
        );
    }

    const MB: u64 = 1024 * 1024;

    fn urls(v: &[Hit]) -> Vec<&str> {
        v.iter().map(|h| h.url.as_str()).collect()
    }

    #[test]
    fn collector_ranks_and_collapses() {
        let mut c = Collector::default();
        assert!(c.add(hit("https://c/v/master.m3u8", HitKind::Hls)));
        c.add(hit("https://c/v/720/index.m3u8", HitKind::Hls));
        assert!(!c.add(hit("https://c/v/720/seg1.ts", HitKind::Segment)));
        let mut small = hit("https://ads.example/pre.mp4", HitKind::File);
        small.size = Some(10_000);
        c.add(small);
        let mut big = hit("https://c/full.mp4", HitKind::File);
        big.size = Some(50 * MB);
        c.add(big);
        assert!(!c.add(hit("https://c/full.mp4?token=2", HitKind::File)));
        let v = c.candidates();
        // Known size first; the small one is kept but hidden, last.
        assert_eq!(
            urls(&v),
            [
                "https://c/full.mp4",
                "https://c/v/master.m3u8",
                "https://ads.example/pre.mp4"
            ]
        );
        assert_eq!(
            (v[2].hidden, v[2].hidden_reason.as_deref()),
            (true, Some("small"))
        );
        assert!(!v[0].hidden && !v[1].hidden);
        // A stream estimated larger than the file moves to the top.
        c.set_size(
            "https://c/v/master.m3u8",
            Some(400 * MB),
            true,
            Some(1800.0),
        );
        assert_eq!(c.candidates()[0].url, "https://c/v/master.m3u8");
        c.annotate("https://c/v/master.m3u8", true, false, 1080);
        assert_eq!(c.candidates()[0].url, "https://c/full.mp4");
        assert!(c.saw_media());
        c.clear();
        assert!(!c.saw_media());
    }

    #[test]
    fn filter_hides_small_short_and_ads_but_keeps_unknown() {
        let f = SniffFilter::from_mb(10);
        let mut a = hit("https://c/a.mp4", HitKind::File);
        a.size = Some(3 * MB);
        assert_eq!(f.reason(&a), Some("small"));
        a.size = Some(30 * MB);
        assert_eq!(f.reason(&a), None);
        a.duration = Some(15.0);
        assert_eq!(f.reason(&a), Some("short"));
        let unknown = hit("https://c/b.mp4", HitKind::File);
        assert_eq!(f.reason(&unknown), None, "unknown size is never hidden");
        let ad = hit("https://pubads.g.doubleclick.net/x.mp4", HitKind::File);
        assert!(ad.likely_ad);
        assert_eq!(f.reason(&ad), Some("ad"));
        let mut live = hit("https://c/live.m3u8", HitKind::Hls);
        live.live = true;
        live.duration = Some(6.0);
        assert_eq!(f.reason(&live), None, "live streams are never hidden");
        // Off: nothing hidden, not even ads.
        let off = SniffFilter::from_mb(0);
        assert_eq!(off.reason(&ad), None);
        a.size = Some(1);
        assert_eq!(off.reason(&a), None);
        assert_eq!(SniffFilter::default(), SniffFilter::from_mb(DEFAULT_MIN_MB));
        assert!(MIN_MB_PRESETS.contains(&DEFAULT_MIN_MB));
    }

    #[test]
    fn largest_first_and_resorts_on_updates() {
        let mut c = Collector::default();
        let f = SniffFilter::from_mb(0);
        let mut a = hit("https://c/a.mp4", HitKind::File);
        a.size = Some(20 * MB);
        c.add(a);
        let mut b = hit("https://c/b.mp4", HitKind::File);
        b.size = Some(5 * MB);
        c.add(b);
        c.add(hit("https://c/u.mp4", HitKind::File));
        assert_eq!(
            urls(&c.candidates_with(&f)),
            ["https://c/a.mp4", "https://c/b.mp4", "https://c/u.mp4"]
        );
        // A later, larger Content-Length re-ranks b and reports a change.
        let mut b2 = hit("https://c/b.mp4?range=x", HitKind::File);
        b2.size = Some(90 * MB);
        assert!(c.add(b2));
        assert_eq!(c.candidates_with(&f)[0].url, "https://c/b.mp4");
        // A smaller chunk doesn't shrink it.
        let mut b3 = hit("https://c/b.mp4", HitKind::File);
        b3.size = Some(MB);
        assert!(!c.add(b3));
        assert_eq!(c.candidates_with(&f)[0].size, Some(90 * MB));
        // Duration breaks a size tie.
        let mut c2 = Collector::default();
        for (u, d) in [("https://c/x.mp4", 40.0), ("https://c/y.mp4", 90.0)] {
            let mut h = hit(u, HitKind::File);
            h.size = Some(30 * MB);
            h.duration = Some(d);
            c2.add(h);
        }
        assert_eq!(c2.candidates_with(&f)[0].url, "https://c/y.mp4");
        // An estimate never overrides a real size.
        c.set_size("https://c/a.mp4", Some(999 * MB), true, None);
        assert_eq!(
            c.candidates_with(&f)
                .iter()
                .find(|h| h.url == "https://c/a.mp4")
                .unwrap()
                .size,
            Some(20 * MB)
        );
    }

    #[test]
    fn hls_and_dash_estimation() {
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10.0,\na.ts\n#EXTINF:9.5,\nb.ts\n#EXTINF:4.5,title\nc.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(hls_duration(media), Some(24.0));
        assert_eq!(
            hls_duration("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n"),
            None
        );
        // 5 Mbit/s for 10 minutes = 375 MB.
        assert_eq!(estimate_bytes(5_000_000, 600.0), Some(375_000_000));
        assert_eq!(estimate_bytes(0, 600.0), None);
        assert_eq!(estimate_bytes(5_000_000, 0.0), None);
        let br = "#EXTM3U\n#EXTINF:4,\n#EXT-X-BYTERANGE:1000@0\nv.mp4\n#EXTINF:4,\n#EXT-X-BYTERANGE:2500\nv.mp4\n";
        assert_eq!(hls_byterange_total(br), Some(3500));
        assert_eq!(hls_byterange_total(media), None);
        assert_eq!(iso_duration_secs("PT1H2M3.5S"), Some(3723.5));
        assert_eq!(iso_duration_secs("PT45S"), Some(45.0));
        assert_eq!(iso_duration_secs("P1DT1S"), Some(86_401.0));
        assert_eq!(iso_duration_secs("bogus"), None);
        let mpd = r#"<MPD mediaPresentationDuration="PT2M0S"><Period>
            <AdaptationSet mimeType="video/mp4"><Representation bandwidth="800000"/><Representation bandwidth="4000000" height="1080"/></AdaptationSet>
            <AdaptationSet mimeType="audio/mp4"><Representation bandwidth="128000"/></AdaptationSet></Period></MPD>"#;
        assert_eq!(dash_duration(mpd), Some(120.0));
        assert_eq!(dash_bandwidth(mpd), 4_128_000);
        assert_eq!(
            content_range_total("bytes 0-1023/52428800"),
            Some(52_428_800)
        );
        assert_eq!(content_range_total("bytes 0-1023/*"), None);
    }

    #[test]
    fn inject_script_reports_only_through_sniff_report() {
        assert!(INJECT_JS.contains("sniff_report"));
        assert!(INJECT_JS.contains("requestMediaKeySystemAccess"));
        assert!(!INJECT_JS.contains("eval("));
    }
}
