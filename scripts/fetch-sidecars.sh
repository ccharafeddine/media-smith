#!/usr/bin/env bash
# Fetch/build every Tauri externalBin sidecar: ffmpeg + ffprobe, yt-dlp and
# whisper-cli (built from source; needs CMake + a C++ toolchain).
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$DIR/fetch-ffmpeg.sh"
bash "$DIR/fetch-ytdlp.sh"
bash "$DIR/fetch-whisper.sh"
