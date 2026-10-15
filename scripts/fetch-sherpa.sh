#!/usr/bin/env bash
# Fetch the prebuilt STATIC sherpa-onnx speaker-diarization CLI (no
# onnxruntime.dll to ship) as a Tauri sidecar. Windows archive and binary are
# pinned by SHA-256 (verified 2026-10-14); other platforms take the same
# release's static build.
set -euo pipefail
VERSION="${SHERPA_VERSION:-1.12.21}"
BIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/app/src-tauri/binaries"
TRIPLE="${TARGET_TRIPLE:-$(rustc -vV | sed -n 's/host: //p')}"
HF="https://huggingface.co/csukuangfj/sherpa-onnx-libs/resolve/main"
EXE=sherpa-onnx-offline-speaker-diarization
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
case "$TRIPLE" in
  *windows*) URL="$HF/win64/$VERSION/sherpa-onnx-v$VERSION-win-x64-static.tar.bz2"; EXT=".exe"
             ARCHIVE_SHA="fe00da604516b25013a18d63818db385b45f0e003d19ecf5e597dfa65f681b57"
             EXE_SHA="e1272eb65c1dad074f895a347693d7ab1cedde9c84d1e35232c5ebb7149a9790" ;;
  *linux*)   URL="$HF/linux/$VERSION/sherpa-onnx-v$VERSION-linux-x64-static.tar.bz2"; EXT=""; ARCHIVE_SHA=""; EXE_SHA="" ;;
  *apple*)   MV="${SHERPA_MAC_VERSION:-1.12.40}"
             URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v$MV/sherpa-onnx-v$MV-osx-universal2-static.tar.bz2"; EXT=""; ARCHIVE_SHA=""; EXE_SHA="" ;;
  *) echo "unsupported target $TRIPLE" >&2; exit 1 ;;
esac
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
echo "== Fetching $(basename "$URL") =="
curl -fL --retry 3 -o "$WORK/s.tar.bz2" "$URL"
[ -z "$ARCHIVE_SHA" ] || [ "$(sha "$WORK/s.tar.bz2")" = "$ARCHIVE_SHA" ] || { echo "archive checksum mismatch" >&2; exit 1; }
mkdir -p "$WORK/x" "$BIN_DIR"
tar xjf "$WORK/s.tar.bz2" -C "$WORK/x"
FOUND="$(find "$WORK/x" -type f -name "$EXE$EXT" | head -1)"
[ -n "$FOUND" ] || { echo "$EXE$EXT not in archive" >&2; exit 1; }
[ -z "$EXE_SHA" ] || [ "$(sha "$FOUND")" = "$EXE_SHA" ] || { echo "binary checksum mismatch" >&2; exit 1; }
cp "$FOUND" "$BIN_DIR/$EXE-$TRIPLE$EXT"; chmod +x "$BIN_DIR/$EXE-$TRIPLE$EXT"
echo "sherpa-onnx -> $BIN_DIR/$EXE-$TRIPLE$EXT"
