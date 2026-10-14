//! Minimal synchronous ffmpeg/ffprobe runner. The desktop shell resolves the
//! bundled sidecar path and hands it in; nothing here assumes a location.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("failed to start {0}: {1}")]
    Spawn(String, std::io::Error),
    #[error("{tool} exited with {code:?}: {stderr}")]
    Failed {
        tool: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("cancelled")]
    Cancelled,
    #[error("refusing flag-like path argument: {0}")]
    FlagLike(String),
}

/// Reject user-supplied paths that ffmpeg would parse as an option.
pub fn reject_flaglike(path: &str) -> Result<(), RunError> {
    if path.starts_with('-') {
        Err(RunError::FlagLike(path.to_string()))
    } else {
        Ok(())
    }
}

/// Resolve a sidecar binary next to the current executable (Tauri's layout
/// both in dev `target/` and in bundles).
pub fn sidecar_path(name: &str) -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "exe has no parent"))?;
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    Ok(dir.join(file))
}

/// Run a tool with args, returning stdout on success.
pub fn run_tool(bin: &Path, args: &[String]) -> Result<String, RunError> {
    let tool = bin.display().to_string();
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| RunError::Spawn(tool.clone(), e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(RunError::Failed {
            tool,
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Run ffmpeg with args.
pub fn run_ffmpeg(ffmpeg: &Path, args: &[String]) -> Result<String, RunError> {
    run_tool(ffmpeg, args)
}

/// Run a tool and return raw stdout bytes (binary-safe; used for PNG pipes).
pub fn run_tool_bytes(bin: &Path, args: &[String]) -> Result<Vec<u8>, RunError> {
    let tool = bin.display().to_string();
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| RunError::Spawn(tool.clone(), e))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(RunError::Failed {
            tool,
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Parse one line of ffmpeg `-progress pipe:1` output into microseconds of
/// output written. ffmpeg emits `out_time_us=` (and the misnamed
/// `out_time_ms=`, also microseconds); `N/A` before the first frame.
pub fn parse_progress_line(line: &str) -> Option<u64> {
    let (k, v) = line.trim().split_once('=')?;
    match k {
        "out_time_us" | "out_time_ms" => v.trim().parse::<i64>().ok().map(|n| n.max(0) as u64),
        _ => None,
    }
}

/// Progress as a 0..=1 ratio of `total_ms`.
pub fn progress_ratio(out_us: u64, total_ms: u64) -> f64 {
    if total_ms == 0 {
        return 0.0;
    }
    (out_us as f64 / 1000.0 / total_ms as f64).clamp(0.0, 1.0)
}

/// Run ffmpeg with `-progress pipe:1 -nostats`, reporting a 0..=1 ratio of
/// `total_ms` and killing the process as soon as `cancel` is set.
pub fn run_ffmpeg_progress(
    ffmpeg: &Path,
    args: &[String],
    total_ms: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f64),
) -> Result<(), RunError> {
    let tool = ffmpeg.display().to_string();
    let mut full: Vec<String> = vec!["-progress".into(), "pipe:1".into(), "-nostats".into()];
    full.extend_from_slice(args);
    let mut child = Command::new(ffmpeg)
        .args(&full)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::Spawn(tool.clone(), e))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Drain stderr on a thread so a chatty ffmpeg can't block on a full pipe.
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let out_thread = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_thread.join();
            return Err(RunError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if let Some(us) = parse_progress_line(&line) {
                    on_progress(progress_ratio(us, total_ms));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait().map_err(|e| RunError::Spawn(tool.clone(), e))?;
    let _ = out_thread.join();
    let stderr = err_thread.join().unwrap_or_default();
    if status.success() {
        on_progress(1.0);
        Ok(())
    } else {
        Err(RunError::Failed {
            tool,
            code: status.code(),
            stderr,
        })
    }
}

/// Standard ffprobe args for [`crate::probe::parse_meta`].
pub fn ffprobe_args(input: &str) -> Result<Vec<String>, RunError> {
    reject_flaglike(input)?;
    Ok([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_streams",
        "-show_format",
        input,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flaglike_paths_are_rejected() {
        assert!(reject_flaglike("-rf").is_err());
        assert!(reject_flaglike("--output=/tmp/x").is_err());
        assert!(reject_flaglike("/home/u/clip.mp4").is_ok());
        assert!(reject_flaglike("C:/videos/clip.mp4").is_ok());
    }

    #[test]
    fn ffprobe_args_end_with_input() {
        let a = ffprobe_args("in.mp4").unwrap();
        assert_eq!(a.last().unwrap(), "in.mp4");
        assert!(a.contains(&"-show_streams".to_string()));
        assert!(ffprobe_args("-x").is_err());
    }

    #[test]
    fn missing_binary_is_a_spawn_error() {
        let r = run_tool(Path::new("definitely-not-a-real-binary-ms"), &[]);
        assert!(matches!(r, Err(RunError::Spawn(..))));
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(parse_progress_line("out_time_us=1500000"), Some(1_500_000));
        assert_eq!(
            parse_progress_line("out_time_ms=2000000\r"),
            Some(2_000_000)
        );
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("out_time_us=-5"), Some(0));
        assert_eq!(parse_progress_line("frame=12"), None);
        assert_eq!(parse_progress_line("progress=end"), None);
    }

    #[test]
    fn progress_ratio_clamps() {
        assert_eq!(progress_ratio(1_000_000, 2000), 0.5);
        assert_eq!(progress_ratio(9_000_000, 2000), 1.0);
        assert_eq!(progress_ratio(5, 0), 0.0);
    }

    #[test]
    fn cancel_before_start_kills_promptly() {
        // Any long-running program works; use ffmpeg if provided, else skip.
        let Ok(ff) = std::env::var("MS_FFMPEG") else {
            return;
        };
        let args: Vec<String> = [
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=600",
            "-f",
            "null",
            "-",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let cancel = AtomicBool::new(true);
        let t = std::time::Instant::now();
        let r = run_ffmpeg_progress(Path::new(&ff), &args, 600_000, &cancel, |_| {});
        assert!(matches!(r, Err(RunError::Cancelled)));
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn reports_progress_to_completion() {
        let Ok(ff) = std::env::var("MS_FFMPEG") else {
            return;
        };
        let args: Vec<String> = [
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:rate=25",
            "-f",
            "null",
            "-",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut seen = Vec::new();
        run_ffmpeg_progress(Path::new(&ff), &args, 2000, &AtomicBool::new(false), |r| {
            seen.push(r)
        })
        .unwrap();
        assert_eq!(seen.last(), Some(&1.0));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
    }
}
