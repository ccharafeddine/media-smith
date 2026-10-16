//! Runtime updates for the download helpers, so site support doesn't wait
//! for an app release:
//! - yt-dlp: the bundled copy is the fallback; a verified newer copy lives in
//!   the app's data folder (`bin/`). Checked at most once a day, plus a
//!   forced check (rate-limited) when a download fails in a way an update can
//!   fix (unsupported site / HTTP 403).
//! - Deno: yt-dlp's JavaScript runtime for YouTube. Not bundled (it's large);
//!   fetched on first YouTube use and refreshed on the same daily cadence.
//!
//! Every file is checked against the SHA-256 published with its GitHub
//! release before it replaces anything, and smoke-run (`--version`) first.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::util;
use crate::ytdlp;

const YTDLP_REPO: &str = "yt-dlp/yt-dlp";
const DENO_REPO: &str = "denoland/deno";
const DAY: u64 = 24 * 3600;
/// Minimum gap between forced (failure-triggered) checks.
const FORCED_GAP: u64 = 10 * 60;

/// Persisted check times and preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub ytdlp_checked: u64,
    pub ytdlp_forced: u64,
    pub deno_checked: u64,
    /// The user's "keep the downloader up to date" switch (default on).
    pub auto_update: Option<bool>,
}

/// Status for the Settings panel.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub ytdlp_version: Option<String>,
    /// "bundled" or "updated".
    pub ytdlp_source: String,
    pub ytdlp_checked: u64,
    pub deno_version: Option<String>,
    pub auto_update: bool,
}

/// What an update attempt did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub updated: bool,
    pub version: Option<String>,
}

/// Manages the updated helper binaries under `bin_dir`.
#[derive(Debug)]
pub struct Updater {
    bin_dir: PathBuf,
    bundled_ytdlp: PathBuf,
    lock: Mutex<()>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The yt-dlp release asset for this platform.
#[must_use]
pub fn ytdlp_asset() -> Option<&'static str> {
    if cfg!(windows) {
        Some("yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        Some("yt-dlp_macos")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("yt-dlp_linux")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("yt-dlp_linux_aarch64")
    } else {
        None
    }
}

/// The Deno release zip for this platform.
#[must_use]
pub fn deno_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("deno-x86_64-pc-windows-msvc.zip")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("deno-aarch64-apple-darwin.zip")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("deno-x86_64-apple-darwin.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("deno-x86_64-unknown-linux-gnu.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("deno-aarch64-unknown-linux-gnu.zip")
    } else {
        None
    }
}

/// Find the SHA-256 for `asset` in a checksum listing (`<hex>  <name>` lines,
/// or a single bare hash for one-file `.sha256sum` files).
#[must_use]
pub fn find_sha256(listing: &str, asset: &str) -> Option<String> {
    let is_hex = |t: &str| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit());
    for line in listing.lines() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.iter().any(|t| t.trim_start_matches('*').trim_start_matches("./") == asset) {
            if let Some(h) = toks.iter().find(|t| is_hex(t)) {
                return Some(h.to_ascii_lowercase());
            }
        }
    }
    let hashes: Vec<&str> = listing.split_whitespace().filter(|t| is_hex(t)).collect();
    (hashes.len() == 1).then(|| hashes[0].to_ascii_lowercase())
}

/// Hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compare a yt-dlp version (`2026.09.30`, maybe `.123` nightly suffix)
/// with a release tag; true when the tag is newer.
#[must_use]
pub fn tag_is_newer(current: Option<&str>, tag: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches(['v', 'V'])
            .split(['.', '-'])
            .map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0))
            .collect()
    };
    match current {
        None => true,
        Some(c) => parse(tag) > parse(c),
    }
}

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("smith-fetch-updater")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())
}

/// Latest release tag of a GitHub repo, read from the `releases/latest`
/// redirect (no API rate limit).
async fn latest_tag(c: &reqwest::Client, repo: &str) -> Result<String, String> {
    let r = c
        .get(format!("https://github.com/{repo}/releases/latest"))
        .send()
        .await
        .map_err(|e| format!("couldn't check for updates: {e}"))?;
    let path = r.url().path().to_string();
    path.rsplit_once("/tag/")
        .map(|(_, t)| t.trim_end_matches('/').to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "couldn't read the latest release".to_string())
}

async fn get_bytes(c: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let r = c.get(url).send().await.map_err(|e| format!("download failed: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("download failed: HTTP {}", r.status()));
    }
    r.bytes().await.map(|b| b.to_vec()).map_err(|e| format!("download failed: {e}"))
}

fn make_executable(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = p;
    Ok(())
}

impl Updater {
    #[must_use]
    pub fn new(bin_dir: PathBuf, bundled_ytdlp: PathBuf) -> Self {
        Self { bin_dir, bundled_ytdlp, lock: Mutex::new(()) }
    }

    fn state_path(&self) -> PathBuf {
        self.bin_dir.join("state.json")
    }

    #[must_use]
    pub fn state(&self) -> State {
        std::fs::read_to_string(self.state_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save_state(&self, s: &State) {
        let _ = std::fs::create_dir_all(&self.bin_dir);
        if let Ok(t) = serde_json::to_string_pretty(s) {
            let _ = std::fs::write(self.state_path(), t);
        }
    }

    pub fn set_auto_update(&self, on: bool) {
        let mut s = self.state();
        s.auto_update = Some(on);
        self.save_state(&s);
    }

    #[must_use]
    pub fn auto_update(&self) -> bool {
        self.state().auto_update.unwrap_or(true)
    }

    fn updated_ytdlp(&self) -> PathBuf {
        self.bin_dir.join(exe("yt-dlp"))
    }

    /// The yt-dlp to run: the verified updated copy when present, else bundled.
    #[must_use]
    pub fn ytdlp_path(&self) -> PathBuf {
        let u = self.updated_ytdlp();
        if std::fs::metadata(&u).map(|m| m.len() > 1_000_000).unwrap_or(false) {
            u
        } else {
            self.bundled_ytdlp.clone()
        }
    }

    /// The downloaded Deno, if present.
    #[must_use]
    pub fn deno_path(&self) -> Option<PathBuf> {
        let p = self.bin_dir.join(exe("deno"));
        std::fs::metadata(&p).map(|m| m.len() > 1_000_000).unwrap_or(false).then_some(p)
    }

    pub async fn status(&self) -> Status {
        let st = self.state();
        let yt = self.ytdlp_path();
        let deno_version = match self.deno_path() {
            Some(d) => deno_version(&d).await,
            None => None,
        };
        Status {
            ytdlp_version: ytdlp::version(&yt).await,
            ytdlp_source: if yt == self.bundled_ytdlp { "bundled".into() } else { "updated".into() },
            ytdlp_checked: st.ytdlp_checked,
            deno_version,
            auto_update: st.auto_update.unwrap_or(true),
        }
    }

    /// Daily background check (no-op when auto-update is off or checked < 24 h ago).
    pub async fn maybe_update(&self) {
        if !self.auto_update() {
            return;
        }
        let st = self.state();
        if now().saturating_sub(st.ytdlp_checked) >= DAY {
            let _ = self.update_ytdlp().await;
        }
        if self.deno_path().is_some() && now().saturating_sub(st.deno_checked) >= DAY {
            let _ = self.update_deno().await;
        }
    }

    /// Failure-triggered check: at most once per 10 minutes, honours the
    /// auto-update switch. Returns true when a newer yt-dlp was installed.
    pub async fn update_after_failure(&self) -> bool {
        if !self.auto_update() {
            return false;
        }
        let mut st = self.state();
        if now().saturating_sub(st.ytdlp_forced) < FORCED_GAP {
            return false;
        }
        st.ytdlp_forced = now();
        self.save_state(&st);
        matches!(self.update_ytdlp().await, Ok(Outcome { updated: true, .. }))
    }

    /// Check GitHub for a newer yt-dlp and install it (verified).
    ///
    /// # Errors
    /// On network failure, checksum mismatch, or a binary that won't run.
    pub async fn update_ytdlp(&self) -> Result<Outcome, String> {
        let _g = self.lock.lock().await;
        let asset = ytdlp_asset().ok_or("no yt-dlp build for this platform")?;
        let c = http()?;
        let tag = latest_tag(&c, YTDLP_REPO).await?;
        let mut st = self.state();
        st.ytdlp_checked = now();
        self.save_state(&st);
        let current = ytdlp::version(&self.ytdlp_path()).await;
        if !tag_is_newer(current.as_deref(), &tag) {
            return Ok(Outcome { updated: false, version: current });
        }
        let base = format!("https://github.com/{YTDLP_REPO}/releases/download/{tag}");
        let sums = String::from_utf8_lossy(&get_bytes(&c, &format!("{base}/SHA2-256SUMS")).await?).into_owned();
        let want = find_sha256(&sums, asset).ok_or("the release has no checksum for this platform")?;
        let bytes = get_bytes(&c, &format!("{base}/{asset}")).await?;
        if sha256_hex(&bytes) != want {
            return Err("the downloaded yt-dlp failed its checksum; keeping the current one".into());
        }
        let version = self.install(&bytes, "yt-dlp", |p| async move { ytdlp::version(&p).await }).await?;
        Ok(Outcome { updated: true, version: Some(version) })
    }

    /// Download Deno if it isn't here yet (first YouTube use).
    ///
    /// # Errors
    /// On network failure, checksum mismatch, or a binary that won't run.
    pub async fn ensure_deno(&self) -> Result<PathBuf, String> {
        if let Some(p) = self.deno_path() {
            return Ok(p);
        }
        self.update_deno().await?;
        self.deno_path().ok_or_else(|| "Deno wasn't installed".to_string())
    }

    /// Install or refresh Deno from its latest GitHub release (verified).
    ///
    /// # Errors
    /// On network failure, checksum mismatch, or a binary that won't run.
    pub async fn update_deno(&self) -> Result<Outcome, String> {
        let _g = self.lock.lock().await;
        let asset = deno_asset().ok_or("no Deno build for this platform")?;
        let c = http()?;
        let tag = latest_tag(&c, DENO_REPO).await?;
        let mut st = self.state();
        st.deno_checked = now();
        self.save_state(&st);
        let current = match self.deno_path() {
            Some(p) => deno_version(&p).await,
            None => None,
        };
        if current.is_some() && !tag_is_newer(current.as_deref(), &tag) {
            return Ok(Outcome { updated: false, version: current });
        }
        let base = format!("https://github.com/{DENO_REPO}/releases/download/{tag}");
        let sums = String::from_utf8_lossy(&get_bytes(&c, &format!("{base}/{asset}.sha256sum")).await?).into_owned();
        let want = find_sha256(&sums, asset).ok_or("the Deno release has no checksum")?;
        let zip = get_bytes(&c, &format!("{base}/{asset}")).await?;
        if sha256_hex(&zip) != want {
            return Err("the downloaded Deno failed its checksum".into());
        }
        let bin = unzip_one(&zip, &exe("deno"))?;
        let version = self.install(&bin, "deno", |p| async move { deno_version(&p).await }).await?;
        Ok(Outcome { updated: true, version: Some(version) })
    }

    /// Write `bytes` as `<bin_dir>/<name>.new`, smoke-run it, then swap it in.
    async fn install<F, Fut>(&self, bytes: &[u8], name: &str, check: F) -> Result<String, String>
    where
        F: FnOnce(PathBuf) -> Fut,
        Fut: std::future::Future<Output = Option<String>>,
    {
        std::fs::create_dir_all(&self.bin_dir).map_err(|e| e.to_string())?;
        let final_path = self.bin_dir.join(exe(name));
        let staged = self.bin_dir.join(format!("{}.new", exe(name)));
        std::fs::write(&staged, bytes).map_err(|e| format!("couldn't save the update: {e}"))?;
        make_executable(&staged).map_err(|e| e.to_string())?;
        // The smoke run must use a runnable name on Windows (.exe last).
        let probe = self.bin_dir.join(exe(&format!("{name}-check")));
        std::fs::copy(&staged, &probe).map_err(|e| e.to_string())?;
        make_executable(&probe).map_err(|e| e.to_string())?;
        let version = check(probe.clone()).await;
        let _ = std::fs::remove_file(&probe);
        let Some(version) = version else {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("the new {name} wouldn't run; keeping the current one"));
        };
        let _ = std::fs::remove_file(&final_path);
        std::fs::rename(&staged, &final_path).map_err(|e| format!("couldn't install the update: {e}"))?;
        Ok(version)
    }
}

/// `deno --version` → `2.5.1`.
pub async fn deno_version(p: &Path) -> Option<String> {
    let o = util::command(p).arg("--version").output().await.ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    let first = s.lines().next()?;
    first.split_whitespace().nth(1).map(str::to_string)
}

/// Extract the file named `name` from a zip archive in memory (stored or
/// deflated entries, read via the central directory).
///
/// # Errors
/// If the archive is unreadable or lacks the file.
pub fn unzip_one(z: &[u8], name: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let u16_at = |i: usize| -> Option<usize> { z.get(i..i + 2).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]]))) };
    let u32_at = |i: usize| -> Option<usize> { z.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) };
    let bad = || "bad archive".to_string();
    // End of central directory: last "PK\x05\x06".
    let eocd = (0..z.len().saturating_sub(21)).rev().find(|&i| z[i..].starts_with(b"PK\x05\x06")).ok_or_else(bad)?;
    let count = u16_at(eocd + 10).ok_or_else(bad)?;
    let mut p = u32_at(eocd + 16).ok_or_else(bad)?;
    for _ in 0..count {
        if !z.get(p..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) {
            return Err(bad());
        }
        let method = u16_at(p + 10).ok_or_else(bad)?;
        let csize = u32_at(p + 20).ok_or_else(bad)?;
        let usize_ = u32_at(p + 24).ok_or_else(bad)?;
        let nlen = u16_at(p + 28).ok_or_else(bad)?;
        let elen = u16_at(p + 30).ok_or_else(bad)?;
        let clen = u16_at(p + 32).ok_or_else(bad)?;
        let local = u32_at(p + 42).ok_or_else(bad)?;
        let fname = String::from_utf8_lossy(z.get(p + 46..p + 46 + nlen).ok_or_else(bad)?).into_owned();
        p += 46 + nlen + elen + clen;
        if fname.rsplit('/').next() != Some(name) {
            continue;
        }
        let lnlen = u16_at(local + 26).ok_or_else(bad)?;
        let lelen = u16_at(local + 28).ok_or_else(bad)?;
        let start = local + 30 + lnlen + lelen;
        let data = z.get(start..start + csize).ok_or_else(bad)?;
        return match method {
            0 => Ok(data.to_vec()),
            8 => {
                let mut v = Vec::with_capacity(usize_);
                flate2::read::DeflateDecoder::new(data).read_to_end(&mut v).map_err(|e| e.to_string())?;
                Ok(v)
            }
            _ => Err("unsupported compression in archive".to_string()),
        };
    }
    Err(format!("{name} not found in the archive"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_checksums() {
        let h = "a".repeat(64);
        let listing = format!("{}  yt-dlp\n{h}  yt-dlp.exe\n{}  yt-dlp_macos\n", "b".repeat(64), "c".repeat(64));
        assert_eq!(find_sha256(&listing, "yt-dlp.exe"), Some(h.clone()));
        assert_eq!(find_sha256(&listing, "yt-dlp"), Some("b".repeat(64)));
        assert_eq!(find_sha256(&listing, "nope"), None);
        let single = format!("{}\n", "D".repeat(64));
        assert_eq!(find_sha256(&single, "deno.zip"), Some("d".repeat(64)));
        let ps = format!("Algorithm Hash Path\nSHA256 {} C:\\a\\deno-x86_64-pc-windows-msvc.zip", "e".repeat(64));
        assert_eq!(find_sha256(&ps, "deno-x86_64-pc-windows-msvc.zip"), Some("e".repeat(64)));
    }

    #[test]
    fn sha_and_versions() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(tag_is_newer(Some("2026.01.01"), "2026.02.03"));
        assert!(!tag_is_newer(Some("2026.02.03"), "2026.02.03"));
        assert!(!tag_is_newer(Some("2026.02.03.123"), "2026.02.03"));
        assert!(tag_is_newer(None, "2026.02.03"));
        assert!(tag_is_newer(Some("2.4.1"), "v2.5.0"));
        assert!(!tag_is_newer(Some("2.5.0"), "v2.5.0"));
    }

    #[test]
    fn prefers_updated_copy_only_when_real() {
        let dir = std::env::temp_dir().join(format!("smith-upd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let u = Updater::new(dir.clone(), PathBuf::from("/bundled/yt-dlp"));
        assert_eq!(u.ytdlp_path(), PathBuf::from("/bundled/yt-dlp"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(exe("yt-dlp")), b"tiny").unwrap();
        assert_eq!(u.ytdlp_path(), PathBuf::from("/bundled/yt-dlp"));
        std::fs::write(dir.join(exe("yt-dlp")), vec![0u8; 1_100_000]).unwrap();
        assert_eq!(u.ytdlp_path(), dir.join(exe("yt-dlp")));
        assert!(u.auto_update());
        u.set_auto_update(false);
        assert!(!u.auto_update());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A one-entry deflated zip built by hand (what `zip -9` produces).
    fn make_zip(name: &str, body: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        e.write_all(body).unwrap();
        let comp = e.finish().unwrap();
        let (n, c, u) = (name.len() as u16, comp.len() as u32, body.len() as u32);
        let mut z = Vec::new();
        z.extend(b"PK\x03\x04"); z.extend([20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(c.to_le_bytes()); z.extend(u.to_le_bytes()); z.extend(n.to_le_bytes()); z.extend([0, 0]);
        z.extend(name.as_bytes()); z.extend(&comp);
        let cd = z.len() as u32;
        z.extend(b"PK\x01\x02"); z.extend([20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(c.to_le_bytes()); z.extend(u.to_le_bytes()); z.extend(n.to_le_bytes());
        z.extend([0u8; 12]); z.extend(0u32.to_le_bytes()); z.extend(name.as_bytes());
        let cdlen = z.len() as u32 - cd;
        z.extend(b"PK\x05\x06"); z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend(cdlen.to_le_bytes()); z.extend(cd.to_le_bytes()); z.extend([0, 0]);
        z
    }

    #[test]
    fn unzips_named_file() {
        let z = make_zip("deno", b"binary binary binary");
        assert_eq!(unzip_one(&z, "deno").unwrap(), b"binary binary binary");
        assert!(unzip_one(&z, "x").is_err());
        assert!(unzip_one(b"not a zip at all, definitely not", "deno").is_err());
    }
}
