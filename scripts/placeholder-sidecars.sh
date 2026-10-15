#!/usr/bin/env bash
# Create empty stand-ins for every externalBin sidecar that is missing, so
# `cargo build`/`clippy`/`test` pass where the real binaries aren't fetched
# (CI). tauri-build only checks that the files exist. Never ship these:
# release builds must run scripts/fetch-sidecars.sh first.
set -euo pipefail
BIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/app/src-tauri/binaries"
TRIPLE="${TARGET_TRIPLE:-$(rustc -vV | sed -n 's/host: //p')}"
EXT=""; [[ "$TRIPLE" == *windows* ]] && EXT=".exe"
mkdir -p "$BIN_DIR"
for n in ffmpeg ffprobe yt-dlp whisper-cli sherpa-onnx-offline-speaker-diarization; do
  f="$BIN_DIR/$n-$TRIPLE$EXT"
  [ -e "$f" ] || { : > "$f"; chmod +x "$f"; echo "placeholder: $f"; }
done
