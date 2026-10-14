//! Auto-captions for MediaSmith, ported from CaptionSmith (hardening/2026-10):
//!
//! - [`whisper`]: run the bundled `whisper-cli` sidecar on a 16 kHz WAV with
//!   streamed progress and cancellation, and parse its JSON into segments.
//! - [`models`]: an allowlisted, SHA-256-verified ggml model manager.
//! - [`place`]: turn segments into timeline caption cues.
//!
//! No Tauri dependency: the shell wires these into commands and events.

pub mod models;
pub mod place;
pub mod whisper;
