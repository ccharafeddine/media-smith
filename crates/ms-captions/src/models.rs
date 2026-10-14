//! Whisper model manager. Downloads are restricted to an allowlist of known
//! ggml filenames (never arbitrary URLs or paths) and every download is
//! hashed while it streams; a file whose SHA-256 doesn't match the table is
//! discarded, so a truncated, corrupted or swapped file is never installed.
//! Writes go to `<file>.part` and are renamed only after verification.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

/// The model used when the user hasn't picked one. MediaSmith doesn't bundle
/// it (unlike CaptionSmith); it is downloaded on first Auto-caption.
pub const DEFAULT_MODEL: &str = "ggml-base.en.bin";

/// Allowlisted models: (file, label, approx MB, SHA-256 from the Git LFS
/// pointers at https://huggingface.co/ggerganov/whisper.cpp/raw/main/<file>,
/// fetched 2026-10-14).
pub const MODELS: &[(&str, &str, u32, &str)] = &[
    (
        "ggml-base.en.bin",
        "Base · English",
        148,
        "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    ),
    (
        "ggml-base.bin",
        "Base · multilingual",
        148,
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    ),
    (
        "ggml-small.bin",
        "Small · multilingual",
        488,
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    ),
    (
        "ggml-medium.bin",
        "Medium · multilingual",
        1530,
        "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
    ),
    (
        "ggml-large-v3-turbo.bin",
        "Large v3 Turbo · multilingual",
        1620,
        "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unknown model file: {0}")]
    Unknown(String),
    #[error("the downloaded model failed its checksum (expected {expected}, got {actual}); it was discarded")]
    Checksum { expected: String, actual: String },
    #[error("download cancelled")]
    Cancelled,
    #[error("download failed: {0}")]
    Http(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Only exact allowlisted names pass, which also blocks path traversal.
pub fn is_allowed(file: &str) -> bool {
    MODELS.iter().any(|m| m.0 == file)
}

pub fn expected_sha256(file: &str) -> Option<&'static str> {
    MODELS.iter().find(|m| m.0 == file).map(|m| m.3)
}

pub fn model_url(file: &str) -> Result<String, ModelError> {
    if !is_allowed(file) {
        return Err(ModelError::Unknown(file.into()));
    }
    Ok(format!("{HF_BASE}{file}"))
}

pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

pub fn verify_digest(file: &str, digest: &[u8]) -> Result<(), ModelError> {
    let expected = expected_sha256(file).ok_or_else(|| ModelError::Unknown(file.into()))?;
    let actual = to_hex(digest);
    if actual == expected {
        Ok(())
    } else {
        Err(ModelError::Checksum {
            expected: expected.into(),
            actual,
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelInfo {
    pub file: String,
    pub label: String,
    pub size_mb: u32,
    pub multilingual: bool,
    pub installed: bool,
    pub is_default: bool,
}

/// Every allowlisted model and whether it's installed in `dir`.
pub fn list(dir: &Path) -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|(file, label, mb, _)| ModelInfo {
            file: (*file).into(),
            label: (*label).into(),
            size_mb: *mb,
            multilingual: !file.contains(".en."),
            installed: dir.join(file).is_file(),
            is_default: *file == DEFAULT_MODEL,
        })
        .collect()
}

/// Path of an installed allowlisted model.
pub fn resolve(dir: &Path, file: &str) -> Result<PathBuf, ModelError> {
    if !is_allowed(file) {
        return Err(ModelError::Unknown(file.into()));
    }
    let p = dir.join(file);
    if p.is_file() {
        Ok(p)
    } else {
        Err(ModelError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("model {file} is not downloaded yet"),
        )))
    }
}

pub fn delete(dir: &Path, file: &str) -> Result<(), ModelError> {
    if !is_allowed(file) {
        return Err(ModelError::Unknown(file.into()));
    }
    let p = dir.join(file);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

/// Stream `reader` into `dir/<file>.part`, hashing as it goes; verify, then
/// rename to `dir/<file>`. `progress(received, total)` is throttled to every
/// ~2 MB. On cancel, error or checksum mismatch the `.part` is removed.
pub fn install_from_reader(
    mut reader: impl Read,
    dir: &Path,
    file: &str,
    total: Option<u64>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, ModelError> {
    if !is_allowed(file) {
        return Err(ModelError::Unknown(file.into()));
    }
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{file}.part"));
    let fin = dir.join(file);
    let result = (|| {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&part)?);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let (mut received, mut last) = (0u64, 0u64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ModelError::Cancelled);
            }
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            received += n as u64;
            if received - last >= 2_000_000 {
                last = received;
                progress(received, total);
            }
        }
        out.flush()?;
        drop(out);
        verify_digest(file, &hasher.finalize())?;
        progress(received, total);
        Ok(())
    })();
    match result {
        Ok(()) => {
            std::fs::rename(&part, &fin)?;
            Ok(fin)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Download an allowlisted model from the whisper.cpp host into `dir`.
#[cfg(feature = "download")]
pub fn download(
    dir: &Path,
    file: &str,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, ModelError> {
    let url = model_url(file)?;
    let resp = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .and_then(|c| c.get(&url).send())
        .map_err(|e| ModelError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ModelError::Http(format!("HTTP {}", resp.status())));
    }
    let total = resp.content_length();
    install_from_reader(resp, dir, file, total, cancel, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ms-models-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn allowlist_accepts_known_and_rejects_traversal() {
        assert!(is_allowed("ggml-base.en.bin"));
        assert!(is_allowed("ggml-large-v3-turbo.bin"));
        for bad in [
            "ggml-huge.bin",
            "../../etc/passwd",
            "ggml-base.bin/../x",
            "",
        ] {
            assert!(!is_allowed(bad), "{bad}");
            assert!(model_url(bad).is_err());
        }
        assert_eq!(
            model_url("ggml-small.bin").unwrap(),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
        );
    }

    #[test]
    fn every_model_has_a_lowercase_sha256() {
        for (f, ..) in MODELS {
            let h = expected_sha256(f).unwrap();
            assert_eq!(h.len(), 64, "{f}");
            assert!(h
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        }
        assert!(is_allowed(DEFAULT_MODEL));
    }

    #[test]
    fn digest_checks() {
        let empty = Sha256::digest(b"");
        assert_eq!(
            to_hex(&empty),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(matches!(
            verify_digest("ggml-base.bin", &empty),
            Err(ModelError::Checksum { .. })
        ));
        let h = expected_sha256("ggml-small.bin").unwrap();
        let bytes: Vec<u8> = (0..32)
            .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        assert!(verify_digest("ggml-small.bin", &bytes).is_ok());
    }

    #[test]
    fn bad_download_is_discarded_and_never_installed() {
        let d = tmp("bad");
        let cancel = AtomicBool::new(false);
        let data = vec![7u8; 3_000_000];
        let mut calls = 0;
        let e = install_from_reader(
            &data[..],
            &d,
            "ggml-base.en.bin",
            Some(3_000_000),
            &cancel,
            |_, _| calls += 1,
        )
        .unwrap_err();
        assert!(matches!(e, ModelError::Checksum { .. }), "{e}");
        assert!(calls >= 1, "progress reported while streaming");
        assert!(!d.join("ggml-base.en.bin").exists());
        assert!(!d.join("ggml-base.en.bin.part").exists());
        assert!(!list(&d).iter().any(|m| m.installed));
        // Unknown names never touch the filesystem.
        assert!(install_from_reader(&b""[..], &d, "../x.bin", None, &cancel, |_, _| {}).is_err());
        assert!(delete(&d, "../x.bin").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cancel_removes_part_file() {
        let d = tmp("cancel");
        let cancel = AtomicBool::new(true);
        let e = install_from_reader(
            &[1u8; 10][..],
            &d,
            "ggml-base.bin",
            None,
            &cancel,
            |_, _| {},
        )
        .unwrap_err();
        assert!(matches!(e, ModelError::Cancelled));
        assert!(!d.join("ggml-base.bin.part").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_resolve_and_delete_installed() {
        let d = tmp("list");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("ggml-base.en.bin"), b"x").unwrap();
        let l = list(&d);
        assert_eq!(l.len(), MODELS.len());
        let base = l.iter().find(|m| m.file == "ggml-base.en.bin").unwrap();
        assert!(base.installed && base.is_default && !base.multilingual);
        assert!(
            l.iter()
                .find(|m| m.file == "ggml-small.bin")
                .unwrap()
                .multilingual
        );
        assert!(resolve(&d, "ggml-base.en.bin").is_ok());
        assert!(resolve(&d, "ggml-small.bin").is_err());
        delete(&d, "ggml-base.en.bin").unwrap();
        assert!(resolve(&d, "ggml-base.en.bin").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Real download + checksum of the default model. Opt-in (148 MB):
    /// MS_MODEL_DOWNLOAD_TEST=<dir>.
    #[cfg(feature = "download")]
    #[test]
    fn real_download_when_enabled() {
        let Ok(dir) = std::env::var("MS_MODEL_DOWNLOAD_TEST") else {
            return;
        };
        let cancel = AtomicBool::new(false);
        let p = download(Path::new(&dir), DEFAULT_MODEL, &cancel, |_, _| {}).unwrap();
        assert!(p.metadata().unwrap().len() > 100_000_000);
    }
}
