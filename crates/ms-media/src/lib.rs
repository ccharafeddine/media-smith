//! MediaSmith media core (ported from ClipSmith): probe parsing, an ffmpeg
//! process runner, and hardware encoder selection. No Tauri dependency, so
//! every crate here is testable with plain `cargo test`.

pub mod ass;
pub mod encoders;
pub mod export;
pub mod filmstrip;
pub mod probe;
pub mod runner;
