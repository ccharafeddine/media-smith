//! Live end-to-end check of the engine (network; run by hand):
//! `cargo run -p smith-fetch --example e2e -- <bundled yt-dlp> <ffmpeg> <work dir> <url> [--probe] [--playlist] [--update]`
//! Prints status lines and the downloaded files with their sizes.

use std::path::PathBuf;
use std::sync::Arc;

use smith_fetch::cookies::Store;
use smith_fetch::update::Updater;
use smith_fetch::{DownloadOptions, Fetcher, Hooks, Tools};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (ytdlp, ffmpeg, work, url) = (
        PathBuf::from(&a[1]),
        PathBuf::from(&a[2]),
        PathBuf::from(&a[3]),
        a[4].clone(),
    );
    let flag = |f: &str| a.iter().any(|x| x == f);
    let updater = Arc::new(Updater::new(work.join("bin"), ytdlp));
    if flag("--update") {
        println!("update: {:?}", updater.update_ytdlp().await);
    }
    let last = Arc::new(std::sync::Mutex::new(-1i64));
    let hooks = Hooks {
        progress: Arc::new(move |p| {
            #[allow(clippy::cast_possible_truncation)]
            let pct = (p * 100.0) as i64 / 25;
            let mut l = last.lock().unwrap();
            if pct != *l {
                *l = pct;
                println!("progress {:.0}%", p * 100.0);
            }
        }),
        status: Arc::new(|s| println!("status: {s}")),
    };
    let f = Fetcher::new(
        "SmithTest",
        Tools { ffmpeg, updater },
        Store::new(work.join("cookies")),
        hooks,
    );
    let o = DownloadOptions {
        playlist: flag("--playlist"),
        ..Default::default()
    };
    if flag("--probe") {
        match f.probe(&url, &o).await {
            Ok(i) => println!(
                "probe: {} live={} formats={:?}",
                i.title,
                i.is_live,
                i.formats.iter().map(|c| &c.label).collect::<Vec<_>>()
            ),
            Err(e) => println!("probe error: {e}"),
        }
        return;
    }
    let t = std::time::Instant::now();
    match f.download(&url, &o, &work.join("dl")).await {
        Ok(files) => {
            for p in files {
                println!(
                    "OK {} ({} bytes) in {:.1}s",
                    p.display(),
                    std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
                    t.elapsed().as_secs_f64()
                );
            }
        }
        Err(e) => println!("ERR {e}"),
    }
}
