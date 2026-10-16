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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

const SEGMENT_EXTS: [&str; 5] = ["ts", "m4s", "m4v_seg", "aac", "m4a"];
const FILE_EXTS: [&str; 6] = ["mp4", "webm", "mov", "m4v", "mkv", "mp3"];

/// Hosts whose media requests are ads/trackers, never the video you want.
const AD_HOSTS: [&str; 10] = [
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
];

/// Classify a request by URL and (when known) response Content-Type.
#[must_use]
pub fn classify(url: &str, content_type: Option<&str>) -> Option<HitKind> {
    let u = url::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    if AD_HOSTS
        .iter()
        .any(|a| host == *a || host.ends_with(&format!(".{a}")))
    {
        return None;
    }
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
            h.size = h.size.or(hit.size);
            h.height = h.height.max(hit.height);
            if h.headers.is_empty() {
                h.headers = hit.headers;
            }
            return false;
        }
        if self.hits.len() >= MAX_HITS {
            return false;
        }
        let show = hit.kind != HitKind::Segment;
        self.hits.push(hit);
        show
    }

    /// Mark a hit as checked (DRM / live / height) after reading its manifest.
    pub fn annotate(&mut self, url: &str, drm: bool, live: bool, height: u32) {
        if let Some(h) = self.hits.iter_mut().find(|h| h.url == url) {
            h.drm = drm;
            h.live = live;
            h.height = h.height.max(height);
        }
    }

    /// Downloadable candidates, best first: manifests (they carry every
    /// rendition), then files by size/height. Segments are dropped; tiny
    /// files (< 200 KB: previews, ads, sprites) too.
    #[must_use]
    pub fn candidates(&self) -> Vec<Hit> {
        let mut v: Vec<Hit> = self
            .hits
            .iter()
            .filter(|h| h.kind != HitKind::Segment)
            .filter(|h| !(h.kind == HitKind::File && h.size.is_some_and(|s| s < 200 * 1024)))
            .filter(|h| !is_child_playlist(h, &self.hits))
            .cloned()
            .collect();
        v.sort_by_key(|h| {
            let kind = match h.kind {
                HitKind::Hls => 0,
                HitKind::Dash => 1,
                _ => 2,
            };
            (
                h.drm,
                kind,
                std::cmp::Reverse(h.height),
                std::cmp::Reverse(h.size.unwrap_or(0)),
            )
        });
        v
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
        Hit {
            url: url.into(),
            kind,
            page_url: String::new(),
            headers: vec![],
            size: None,
            height: 0,
            live: false,
            drm: false,
        }
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
            None
        );
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
        big.size = Some(50_000_000);
        c.add(big);
        assert!(!c.add(hit("https://c/full.mp4?token=2", HitKind::File)));
        let v = c.candidates();
        let urls: Vec<&str> = v.iter().map(|h| h.url.as_str()).collect();
        assert_eq!(urls, ["https://c/v/master.m3u8", "https://c/full.mp4"]);
        c.annotate("https://c/v/master.m3u8", true, false, 1080);
        assert_eq!(c.candidates()[0].url, "https://c/full.mp4");
        assert!(c.saw_media());
        c.clear();
        assert!(!c.saw_media());
    }

    #[test]
    fn inject_script_reports_only_through_sniff_report() {
        assert!(INJECT_JS.contains("sniff_report"));
        assert!(INJECT_JS.contains("requestMediaKeySystemAccess"));
        assert!(!INJECT_JS.contains("eval("));
    }
}
