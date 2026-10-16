//! Plain HTTP for the scraper and direct links: browser UA, Referer/Origin,
//! optional extra headers, size caps, and cancel racing.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::util::{
    cancelled, origin_of, BROWSER_USER_AGENT, DOWNLOAD_CANCELLED, MAX_DOWNLOAD_BYTES,
    UNSUPPORTED_SITE,
};

/// Cap on how much page HTML the scraper reads.
pub const MAX_HTML_BYTES: usize = 5 * 1024 * 1024;
/// Cap on a manifest read (they're a few KB in practice).
pub const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;

const UNREACHABLE: &str = "Couldn't reach that link. Check the URL and your connection.";

/// Request context: who referred us and any headers captured from a real
/// browser request (sniff mode).
#[derive(Debug, Clone, Default)]
pub struct Ctx<'a> {
    pub referer: Option<&'a str>,
    pub headers: &'a [(String, String)],
    /// A `Cookie:` header value for this host (from the per-site store).
    pub cookie: Option<&'a str>,
}

/// Build a client for `ctx`. `timeout` bounds the whole request when set.
///
/// # Errors
/// If the TLS backend can't initialise.
pub fn client(ctx: &Ctx<'_>, timeout: Option<u64>) -> Result<reqwest::Client, String> {
    let mut h = HeaderMap::new();
    for (k, v) in ctx.headers {
        let lk = k.to_ascii_lowercase();
        if matches!(
            lk.as_str(),
            "host" | "content-length" | "connection" | "range" | "accept-encoding"
        ) {
            continue;
        }
        if let (Ok(n), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            h.insert(n, val);
        }
    }
    if let Some(r) = ctx.referer {
        if let Ok(v) = HeaderValue::from_str(r) {
            h.entry(reqwest::header::REFERER).or_insert(v);
        }
        if let Some(o) = origin_of(r).and_then(|o| HeaderValue::from_str(&o).ok()) {
            h.entry(reqwest::header::ORIGIN).or_insert(o);
        }
    }
    if let Some(c) = ctx.cookie.and_then(|c| HeaderValue::from_str(c).ok()) {
        h.insert(reqwest::header::COOKIE, c);
    }
    let mut b = reqwest::Client::builder()
        .user_agent(BROWSER_USER_AGENT)
        .default_headers(h)
        .connect_timeout(std::time::Duration::from_secs(20));
    if let Some(t) = timeout {
        b = b.timeout(std::time::Duration::from_secs(t));
    }
    b.build()
        .map_err(|e| format!("could not start the page fetch: {e}"))
}

/// Fetch a page/manifest as text (20 s timeout, `cap` bytes; excess is
/// truncated, not an error).
///
/// # Errors
/// On cancel, an unreachable host, or a non-success status.
pub async fn fetch_text(
    url: &str,
    ctx: &Ctx<'_>,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let c = client(ctx, Some(20))?;
    let mut resp = tokio::select! {
        biased;
        () = cancelled(cancel) => return Err(DOWNLOAD_CANCELLED.to_string()),
        r = c.get(url).header("Accept", "text/html,application/xhtml+xml,application/json,*/*;q=0.8").send() => r.map_err(|_| UNREACHABLE.to_string())?,
    };
    if !resp.status().is_success() {
        return Err(UNSUPPORTED_SITE.to_string());
    }
    let mut body: Vec<u8> = Vec::new();
    loop {
        let chunk = tokio::select! {
            biased;
            () = cancelled(cancel) => return Err(DOWNLOAD_CANCELLED.to_string()),
            c = resp.chunk() => c.map_err(|_| UNSUPPORTED_SITE.to_string())?,
        };
        let Some(chunk) = chunk else { break };
        body.extend_from_slice(&chunk);
        if body.len() >= cap {
            body.truncate(cap);
            break;
        }
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Stream `url` into `out`, capped at [`MAX_DOWNLOAD_BYTES`]. Calls
/// `on_progress` (0.0-1.0) only when Content-Length is known.
///
/// # Errors
/// On cancel, a network failure, a non-success status, or a write failure.
pub async fn fetch_to_file(
    url: &str,
    ctx: &Ctx<'_>,
    out: &Path,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(f64) + Send + Sync),
) -> Result<(), String> {
    use std::io::Write;
    let c = client(ctx, None)?;
    let mut resp = tokio::select! {
        biased;
        () = cancelled(cancel) => return Err(DOWNLOAD_CANCELLED.to_string()),
        r = c.get(url).send() => r.map_err(|_| UNREACHABLE.to_string())?,
    };
    if !resp.status().is_success() {
        return Err(format!(
            "the server returned {} for that link",
            resp.status()
        ));
    }
    let total = resp.content_length();
    let mut downloaded: u64 = 0;
    let mut file =
        std::fs::File::create(out).map_err(|e| format!("could not save the download: {e}"))?;
    loop {
        let chunk = tokio::select! {
            biased;
            () = cancelled(cancel) => return Err(DOWNLOAD_CANCELLED.to_string()),
            chunk = resp.chunk() => chunk.map_err(|_| "the download was interrupted before it finished".to_string())?,
        };
        let Some(chunk) = chunk else { break };
        file.write_all(&chunk)
            .map_err(|e| format!("could not write the download: {e}"))?;
        downloaded += chunk.len() as u64;
        if downloaded > MAX_DOWNLOAD_BYTES {
            return Err("that download is too large".to_string());
        }
        if let Some(total) = total.filter(|t| *t > 0) {
            #[allow(clippy::cast_precision_loss)]
            on_progress((downloaded as f64 / total as f64).clamp(0.0, 1.0));
        }
    }
    Ok(())
}
