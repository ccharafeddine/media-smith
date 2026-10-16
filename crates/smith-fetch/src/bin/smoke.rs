//! Nightly smoke test: checks that each URL in `smoke/urls.txt` still
//! resolves to something downloadable, without downloading whole files.
//!
//! Usage: `smith-fetch-smoke <urls.txt> [yt-dlp path]` (yt-dlp defaults to
//! `yt-dlp` on PATH). Writes a Markdown table to stdout (and to
//! `$GITHUB_STEP_SUMMARY` when set). Exits 1 if a required (`!`) check fails.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use smith_fetch::http::{self, Ctx, MAX_HTML_BYTES, MAX_MANIFEST_BYTES};
use smith_fetch::{scrape, sniff, ytdlp};

async fn check(kind: &str, url: &str, ytdlp_exe: &PathBuf) -> Result<String, String> {
    let never = AtomicBool::new(false);
    match kind {
        "ytdlp" => {
            let out = tokio::time::timeout(
                std::time::Duration::from_secs(120),
                tokio::process::Command::new(ytdlp_exe)
                    .args([
                        "--ignore-config",
                        "-J",
                        "--no-playlist",
                        "--socket-timeout",
                        "20",
                        "--",
                        url,
                    ])
                    .output(),
            )
            .await
            .map_err(|_| "timeout".to_string())?
            .map_err(|e| e.to_string())?;
            let info = ytdlp::parse_probe(&String::from_utf8_lossy(&out.stdout));
            match info {
                Some(i) if out.status.success() => Ok(format!(
                    "{} · {} choices{}",
                    i.extractor,
                    i.formats.len(),
                    if i.is_live { " · live" } else { "" }
                )),
                _ => Err(
                    smith_fetch::util::last_line(&String::from_utf8_lossy(&out.stderr))
                        .chars()
                        .take(160)
                        .collect(),
                ),
            }
        }
        "manifest" => {
            let t = http::fetch_text(url, &Ctx::default(), MAX_MANIFEST_BYTES, &never).await?;
            if sniff::manifest_drm(&t) {
                return Err("DRM (should not be in this list)".into());
            }
            if url.contains(".mpd") {
                let h = sniff::dash_max_height(&t);
                (h > 0)
                    .then(|| format!("DASH · max {h}p"))
                    .ok_or_else(|| "no video representations".into())
            } else {
                scrape::best_hls_variant(&t, url)
                    .map(|v| format!("HLS · best {}p", v.height))
                    .or_else(|| {
                        t.contains("#EXTINF")
                            .then(|| "HLS media playlist".to_string())
                    })
                    .ok_or_else(|| "not a playlist".into())
            }
        }
        "direct" => {
            let c = http::client(&Ctx::default(), Some(30))?;
            let r = c
                .get(url)
                .header("Range", "bytes=0-1023")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let ct = r
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            (r.status().is_success() && (ct.starts_with("video/") || ct.contains("octet-stream")))
                .then(|| format!("{} {ct}", r.status().as_u16()))
                .ok_or_else(|| format!("{} {ct}", r.status()))
        }
        "scrape" => {
            let html = http::fetch_text(url, &Ctx::default(), MAX_HTML_BYTES, &never).await?;
            let c = scrape::extract_candidates(&html, url);
            c.first()
                .map(|f| format!("{} candidates · first {:?}", c.len(), f.kind))
                .ok_or_else(|| "no candidates".into())
        }
        other => Err(format!("unknown check {other}")),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let list = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "smoke/urls.txt".into());
    let ytdlp_exe = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "yt-dlp".into()));
    let text = std::fs::read_to_string(&list).expect("read url list");
    let mut report = String::from("| | check | url | result |\n|---|---|---|---|\n");
    let (mut failed_required, mut passed, mut total) = (0, 0, 0);
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let Some((k, url)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let required = k.starts_with('!');
        let kind = k.trim_start_matches('!');
        total += 1;
        let r = check(kind, url.trim(), &ytdlp_exe).await;
        let (mark, msg) = match &r {
            Ok(m) => {
                passed += 1;
                ("✅", m.clone())
            }
            Err(e) if required => {
                failed_required += 1;
                ("❌", e.clone())
            }
            Err(e) => ("⚠️", e.clone()),
        };
        report.push_str(&format!(
            "| {mark} | {kind}{} | {url} | {} |\n",
            if required { " (required)" } else { "" },
            msg.replace('|', "/")
        ));
    }
    report.push_str(&format!(
        "\n{passed}/{total} passed, {failed_required} required failure(s).\n"
    ));
    print!("{report}");
    if let Ok(p) = std::env::var("GITHUB_STEP_SUMMARY") {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(p) {
            let _ = writeln!(f, "## smith-fetch smoke\n\n{report}");
        }
    }
    std::process::exit(i32::from(failed_required > 0));
}
