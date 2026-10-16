//! HLS/DASH: pick the best rendition, refuse DRM, remux with ffmpeg (stream
//! copy, network-only protocols), and record live streams until stopped.

use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;

use tokio::io::BufReader;

use crate::http::{self, Ctx, MAX_MANIFEST_BYTES};
use crate::scrape;
use crate::sniff;
use crate::util::{
    self, cancelled, is_http_url, origin_of, BROWSER_USER_AGENT, DOWNLOAD_CANCELLED,
    MAX_DOWNLOAD_BYTES, UNSUPPORTED_SITE,
};

/// Protocols ffmpeg may open: network only, so a crafted playlist can't make
/// it read local files or spawn anything.
pub const STREAM_PROTOCOLS: &str = "http,https,tcp,tls,crypto";

/// Message for a DRM-protected stream.
pub const DRM_MESSAGE: &str = "That video is DRM-protected, so it can't be saved.";

/// What ffmpeg should read for a stream.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Inputs {
    /// One input (a manifest or a chosen variant) or two (variant + audio).
    pub inputs: Vec<String>,
    /// True when `inputs[0]` is an already-chosen single variant.
    pub pinned: bool,
    /// The stream is live: record until stopped.
    pub live: bool,
}

/// HLS master → best variant (highest resolution, then bandwidth) and its
/// audio. DASH passes through whole (ffmpeg's default selection already takes
/// the highest-resolution video and best audio). DRM → `Err(DRM_MESSAGE)`.
///
/// # Errors
/// On cancel or DRM.
pub async fn resolve_inputs(
    url: &str,
    hls: bool,
    ctx: &Ctx<'_>,
    cancel: &AtomicBool,
) -> Result<Inputs, String> {
    match http::fetch_text(url, ctx, MAX_MANIFEST_BYTES, cancel).await {
        Ok(text) => {
            if sniff::manifest_drm(&text) {
                return Err(DRM_MESSAGE.to_string());
            }
            let mut live = sniff::manifest_live(&text);
            if hls {
                if let Some(c) = scrape::best_hls_variant(&text, url) {
                    // A master playlist never has #EXTINF; check the variant for liveness.
                    if let Ok(var) =
                        http::fetch_text(&c.video, ctx, MAX_MANIFEST_BYTES, cancel).await
                    {
                        if sniff::manifest_drm(&var) {
                            return Err(DRM_MESSAGE.to_string());
                        }
                        live = sniff::manifest_live(&var);
                    }
                    let mut inputs = vec![c.video];
                    inputs.extend(c.audio);
                    return Ok(Inputs {
                        inputs,
                        pinned: true,
                        live,
                    });
                }
            }
            Ok(Inputs {
                inputs: vec![url.to_string()],
                pinned: false,
                live,
            })
        }
        Err(e) if e == DOWNLOAD_CANCELLED => Err(e),
        Err(_) => Ok(Inputs {
            inputs: vec![url.to_string()],
            pinned: false,
            live: false,
        }),
    }
}

/// Build the ffmpeg remux command. Live recordings go to MPEG-TS (playable
/// even when stopped abruptly) without `+faststart`.
#[must_use]
pub fn ffmpeg_args(s: &Inputs, ctx: &Ctx<'_>, out: &str, max_secs: Option<f64>) -> Vec<String> {
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into()];
    let mut extra = String::new();
    if let Some(o) = ctx.referer.filter(|r| is_http_url(r)).and_then(origin_of) {
        extra.push_str(&format!("Origin: {o}\r\n"));
    }
    if let Some(c) = ctx.cookie.filter(|c| !c.contains(['\r', '\n'])) {
        extra.push_str(&format!("Cookie: {c}\r\n"));
    }
    let mut ua = BROWSER_USER_AGENT.to_string();
    for (k, v) in ctx.headers {
        let lk = k.to_ascii_lowercase();
        if v.contains(['\r', '\n']) || k.contains([':', '\r', '\n']) {
            continue;
        }
        match lk.as_str() {
            "user-agent" => ua = v.clone(),
            "referer" | "origin" | "cookie" | "host" | "range" | "content-length"
            | "connection" | "accept-encoding" => {}
            _ if lk.starts_with("sec-") => {}
            _ => extra.push_str(&format!("{k}: {v}\r\n")),
        }
    }
    for input in &s.inputs {
        args.extend([
            "-protocol_whitelist".to_string(),
            STREAM_PROTOCOLS.to_string(),
            "-user_agent".to_string(),
            ua.clone(),
        ]);
        if let Some(r) = ctx.referer.filter(|r| is_http_url(r)) {
            args.extend(["-referer".to_string(), r.to_string()]);
        }
        if !extra.is_empty() {
            args.extend(["-headers".to_string(), extra.clone()]);
        }
        args.extend(["-i".to_string(), input.clone()]);
    }
    if s.inputs.len() > 1 {
        args.extend(["-map", "0:v:0", "-map", "1:a:0"].map(String::from));
    } else if s.pinned {
        args.extend(["-map", "0:v:0", "-map", "0:a:0?"].map(String::from));
    }
    args.extend(["-c".to_string(), "copy".to_string()]);
    if let Some(t) = max_secs {
        args.extend(["-t".to_string(), format!("{t}")]);
    }
    args.extend(["-fs".to_string(), MAX_DOWNLOAD_BYTES.to_string()]);
    if s.live {
        args.extend(["-f".to_string(), "mpegts".to_string()]);
    } else {
        args.extend(["-movflags".to_string(), "+faststart".to_string()]);
    }
    args.push(out.to_string());
    args
}

/// Parse ffmpeg's `Duration: HH:MM:SS.xx,` header into seconds (`N/A` → None).
#[must_use]
pub fn parse_duration(s: &str) -> Option<f64> {
    let idx = s.find("Duration:")?;
    let rest = s[idx + "Duration:".len()..].trim_start();
    let field = rest.split([',', ' ', '\n', '\r']).next()?;
    let mut parts = field.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    let total = h * 3600.0 + m * 60.0 + sec;
    (total > 0.0).then_some(total)
}

/// Seconds from an ffmpeg progress line's `time=HH:MM:SS.xx`.
#[must_use]
pub fn parse_time(s: &str) -> Option<f64> {
    let idx = s.rfind("time=")?;
    let token: String = s[idx + 5..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    let mut parts = token.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some((h * 3600.0 + m * 60.0 + sec).max(0.0))
}

/// Run ffmpeg with `args`, reporting progress against the input duration
/// (or elapsed seconds via `status` for live). `stop` ends a live recording
/// and keeps the file. Removes `out` on failure or cancel.
///
/// # Errors
/// On cancel or ffmpeg failure.
pub async fn run_ffmpeg(
    ffmpeg: &Path,
    args: Vec<String>,
    out: &Path,
    cancel: &AtomicBool,
    stop: Option<&AtomicBool>,
    progress: &(dyn Fn(f64) + Send + Sync),
    status: &(dyn Fn(&str) + Send + Sync),
) -> Result<(), String> {
    let mut child = util::command(ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg failed to start: {e}"))?;
    // ffmpeg separates progress updates with '\r'; split on both.
    let mut err = BufReader::new(child.stderr.take().expect("piped"));
    let mut duration: Option<f64> = None;
    let mut tail = String::new();
    let never = AtomicBool::new(false);
    let stop = stop.unwrap_or(&never);
    let fail = |msg: String| {
        let _ = std::fs::remove_file(out);
        Err(msg)
    };
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = tokio::select! {
            biased;
            () = cancelled(cancel) => {
                let _ = child.kill().await;
                return fail(DOWNLOAD_CANCELLED.to_string());
            }
            () = cancelled(stop) => {
                let _ = child.kill().await;
                return match std::fs::metadata(out) {
                    Ok(m) if m.len() > 0 => Ok(()),
                    _ => fail("nothing was recorded yet".to_string()),
                };
            }
            r = read_until_cr_or_lf(&mut err, &mut buf) => r.unwrap_or(0),
        };
        if n == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&buf);
        if duration.is_none() {
            duration = parse_duration(&text);
        }
        if let Some(t) = parse_time(&text) {
            match duration {
                Some(total) => progress((t / total).clamp(0.0, 1.0)),
                None => status(&format!("Recording… {}", fmt_secs(t))),
            }
        }
        tail.push_str(&text);
        tail.push('\n');
        util::keep_tail(&mut tail, 4096);
    }
    let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
    if !ok {
        let summary = util::last_line(&tail);
        return fail(if summary.is_empty() {
            "Download failed.".to_string()
        } else {
            format!("Download failed: {summary}")
        });
    }
    match std::fs::metadata(out) {
        Ok(m) if m.len() > 0 => Ok(()),
        _ => fail("the download produced no file".to_string()),
    }
}

async fn read_until_cr_or_lf<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    use tokio::io::AsyncReadExt;
    let mut byte = [0u8; 1];
    let mut n = 0;
    loop {
        let k = r.read(&mut byte).await?;
        if k == 0 {
            return Ok(n);
        }
        n += 1;
        if byte[0] == b'\n' || byte[0] == b'\r' {
            if n > 1 {
                return Ok(n);
            }
            continue;
        }
        buf.push(byte[0]);
    }
}

/// `1:02:03` / `2:03`.
#[must_use]
pub fn fmt_secs(t: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = t.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Remux a finished MPEG-TS recording to MP4 (stream copy). Keeps the `.ts`
/// if the remux fails.
pub async fn ts_to_mp4(ffmpeg: &Path, ts: &Path) -> std::path::PathBuf {
    let mp4 = ts.with_extension("mp4");
    let ok = util::command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-y", "-v", "error", "-i"])
        .arg(ts)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&mp4)
        .stdin(Stdio::null())
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok
        && std::fs::metadata(&mp4)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    {
        let _ = std::fs::remove_file(ts);
        mp4
    } else {
        ts.to_path_buf()
    }
}

/// Download (or record, when live) a stream into `dir`; returns the file.
///
/// # Errors
/// On cancel, DRM, a non-http input, or ffmpeg failure.
#[allow(clippy::too_many_arguments)]
pub async fn download(
    ffmpeg: &Path,
    url: &str,
    hls: bool,
    ctx: &Ctx<'_>,
    dir: &Path,
    cancel: &AtomicBool,
    stop: Option<&AtomicBool>,
    progress: &(dyn Fn(f64) + Send + Sync),
    status: &(dyn Fn(&str) + Send + Sync),
) -> Result<std::path::PathBuf, String> {
    if !is_http_url(url) {
        return Err(UNSUPPORTED_SITE.to_string());
    }
    let inputs = resolve_inputs(url, hls, ctx, cancel).await?;
    if inputs.inputs.iter().any(|i| !is_http_url(i)) {
        return Err(UNSUPPORTED_SITE.to_string());
    }
    let out = dir.join(if inputs.live {
        "source.ts"
    } else {
        "source.mp4"
    });
    if inputs.live {
        status("Live stream: recording until you press Stop");
    }
    let args = ffmpeg_args(&inputs, ctx, &out.to_string_lossy(), None);
    run_ffmpeg(ffmpeg, args, &out, cancel, stop, progress, status).await?;
    if inputs.live {
        status("Finishing recording…");
        return Ok(ts_to_mp4(ffmpeg, &out).await);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_map_variant_audio_and_headers() {
        let split = Inputs {
            inputs: vec![
                "https://c.example/v1080.m3u8".into(),
                "https://c.example/a.m3u8".into(),
            ],
            pinned: true,
            live: false,
        };
        let headers = vec![
            ("Authorization".to_string(), "Bearer t".to_string()),
            ("Sec-Fetch-Mode".to_string(), "cors".to_string()),
            ("User-Agent".to_string(), "MyUA".to_string()),
        ];
        let ctx = Ctx {
            referer: Some("https://page.example/w/1"),
            headers: &headers,
            cookie: Some("a=b"),
        };
        let a = ffmpeg_args(&split, &ctx, "out.mp4", None);
        let s = a.join(" ");
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 2);
        assert_eq!(a.iter().filter(|x| *x == "-protocol_whitelist").count(), 2);
        assert!(s.contains("http,https,tcp,tls,crypto"));
        assert!(s.contains("-referer https://page.example/w/1"));
        assert!(s.contains("Origin: https://page.example\r\n"));
        assert!(s.contains("Cookie: a=b\r\n"));
        assert!(s.contains("Authorization: Bearer t\r\n"));
        assert!(!s.contains("Sec-Fetch"));
        assert!(s.contains("-user_agent MyUA"));
        assert!(s.contains("-map 0:v:0 -map 1:a:0 -c copy"));
        assert!(s.contains("+faststart"));
        assert_eq!(a.last().map(String::as_str), Some("out.mp4"));

        let whole = Inputs {
            inputs: vec!["https://c.example/m.mpd".into()],
            pinned: false,
            live: false,
        };
        let b = ffmpeg_args(&whole, &Ctx::default(), "o.mp4", Some(10.0)).join(" ");
        assert!(!b.contains("-map"));
        assert!(!b.contains("-referer"));
        assert!(b.contains("-c copy -t 10"));

        let live = Inputs {
            inputs: vec!["https://c.example/live.m3u8".into()],
            pinned: true,
            live: true,
        };
        let l = ffmpeg_args(&live, &Ctx::default(), "o.ts", None).join(" ");
        assert!(l.contains("-map 0:v:0 -map 0:a:0?"));
        assert!(l.contains("-f mpegts") && !l.contains("faststart"));
    }

    #[test]
    fn parses_duration_and_time() {
        let s = "Input #0, hls, from 'https://x/i.m3u8':\n  Duration: 00:01:30.50, start: 0.000, bitrate: 0 kb/s";
        assert_eq!(parse_duration(s), Some(90.5));
        assert_eq!(parse_duration("  Duration: N/A, start: 0.0"), None);
        assert_eq!(parse_time("frame=1 time=00:00:01.50 bitrate"), Some(1.5));
        assert_eq!(parse_time("time=N/A"), None);
        assert_eq!(fmt_secs(3723.0), "1:02:03");
        assert_eq!(fmt_secs(65.0), "1:05");
    }
}
