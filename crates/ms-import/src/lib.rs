//! MediaSmith URL import, ported from ClipSmith v2.1.0. yt-dlp is tried first
//! for every page URL; when it reports the site unsupported, the generic
//! [`scrape`] fallback pulls stream links out of the page HTML. No
//! site-specific logic. Network I/O lives in the app shell; everything here is
//! pure and unit tested.

/// The generic page scraper now lives in the shared `smith-fetch` crate.
pub use smith_fetch::scrape;
pub mod ytdlp;
