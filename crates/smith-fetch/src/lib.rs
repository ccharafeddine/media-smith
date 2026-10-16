//! smith-fetch: the URL download engine shared by ClipSmith, GifSmith and
//! MediaSmith.
//!
//! Order of attack for a URL (see [`fetcher::Fetcher::download`]):
//! 1. direct media file → plain HTTP; direct HLS/DASH manifest → ffmpeg remux;
//! 2. yt-dlp (kept current by [`update`]), with cookies / referer / headers,
//!    an `--impersonate` retry on 403 / bot walls, and an update-and-retry on
//!    "unsupported" or 403;
//! 3. the generic page scraper ([`scrape`]) and its embedded players;
//! 4. optionally a hidden sniff pass in a real webview (Tauri glue).
//!
//! Exclusions by design: no DRM circumvention (protected streams are detected
//! by [`sniff::manifest_drm`] and skipped), no packet capture or root
//! certificates, no site-specific logic for adult sites.

pub mod cookies;
pub mod fetcher;
pub mod http;
pub mod scrape;
pub mod sniff;
pub mod stream;
pub mod update;
pub mod util;
pub mod ytdlp;

#[cfg(feature = "tauri")]
pub mod tauri_glue;

pub use fetcher::{DownloadOptions, Fetcher, Hooks, MediaInfo, Tools};
