//! Minimal synchronous ffmpeg/ffprobe runner. The desktop shell resolves the
//! bundled sidecar path and hands it in; nothing here assumes a location.

use std::path::{Path, PathBuf};
use std::process::Command;

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
}
