#!/usr/bin/env bash
# Build a STATIC, self-contained whisper-cli from source (CMake) for the Tauri
# externalBin sidecar (ported from CaptionSmith). Static + GGML_NATIVE=OFF =>
# one portable binary with no sibling DLLs. Models are NOT bundled: the in-app
# model manager downloads them and verifies their SHA-256.
#
# Output: app/src-tauri/binaries/whisper-cli-<target-triple>[.exe]
# Prereqs: CMake + a C/C++ toolchain (MSVC on Windows, clang on macOS).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$REPO_ROOT/app/src-tauri/binaries"
WORK="${WHISPER_BUILD_DIR:-$REPO_ROOT/.whisper-build}"
WHISPER_TAG="${WHISPER_TAG:-v1.9.1}"
mkdir -p "$BIN_DIR" "$WORK"

# On Windows, CMake ships inside the VS Build Tools; use it if not on PATH.
if ! command -v cmake >/dev/null 2>&1; then
  for c in "/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin" \
           "/c/Program Files/Microsoft Visual Studio/2022/Community/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin"; do
    [ -x "$c/cmake.exe" ] && export PATH="$c:$PATH" && break
  done
fi

SRC="$WORK/whisper.cpp"
if [ ! -d "$SRC" ]; then
  git clone --depth 1 -b "$WHISPER_TAG" https://github.com/ggml-org/whisper.cpp "$SRC"
fi

COMMON=(
  -DCMAKE_BUILD_TYPE=Release
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_NATIVE=OFF
  -DWHISPER_BUILD_TESTS=OFF
  -DWHISPER_BUILD_SERVER=OFF
  -DWHISPER_SDL2=OFF
)

echo "== Building static whisper-cli ($WHISPER_TAG) =="
case "$(uname -s)" in
  Darwin)
    cmake -S "$SRC" -B "$SRC/build" "${COMMON[@]}" -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON \
      -DCMAKE_OSX_ARCHITECTURES="arm64;x86_64"
    cmake --build "$SRC/build" --config Release -j --target whisper-cli
    for t in aarch64-apple-darwin x86_64-apple-darwin universal-apple-darwin; do
      cp "$SRC/build/bin/whisper-cli" "$BIN_DIR/whisper-cli-$t"
    done
    chmod +x "$BIN_DIR"/whisper-cli-*-apple-darwin
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT)
    # Static MSVC runtime too, so the exe needs no VC++ redistributable.
    cmake -S "$SRC" -B "$SRC/build" "${COMMON[@]}" -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
    cmake --build "$SRC/build" --config Release -j --target whisper-cli
    CLI="$SRC/build/bin/Release/whisper-cli.exe"
    [ -f "$CLI" ] || CLI="$SRC/build/bin/whisper-cli.exe"
    cp "$CLI" "$BIN_DIR/whisper-cli-x86_64-pc-windows-msvc.exe"
    ;;
  Linux)
    cmake -S "$SRC" -B "$SRC/build" "${COMMON[@]}"
    cmake --build "$SRC/build" -j --target whisper-cli
    cp "$SRC/build/bin/whisper-cli" "$BIN_DIR/whisper-cli-x86_64-unknown-linux-gnu"
    chmod +x "$BIN_DIR/whisper-cli-x86_64-unknown-linux-gnu"
    ;;
  *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
ls -la "$BIN_DIR"/whisper-cli-*
echo "Done. (Set WHISPER_BUILD_DIR or delete $WORK to reclaim the build tree.)"
