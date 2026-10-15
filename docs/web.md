# The Smith suite on the web

The UI talks to `@mediasmith/platform`, never to Tauri directly. `getPlatform()`
returns the Tauri backend inside the desktop webview (`__TAURI_INTERNALS__`) and
the web backend in a normal tab. The web backend runs **ffmpeg.wasm**
(`@ffmpeg/core` 0.12.10, single-threaded, with libx264, libmp3lame, libvpx,
libass and the palette filters) entirely client-side: local files only, no
server, no uploads, no account.

## 1. Static builds

For standalone per-subdomain builds (media, clip, gif and caption .chafic.me), see [`hosting.md`](hosting.md).

Both builds are plain static folders. Nothing in this repo deploys them; hosting
is a separate, manual step.

| Build | Command | Output | Size | Largest file |
|---|---|---|---|---|
| MediaSmith editor | `npm run build -w @mediasmith/app` | `app/dist/` | ~33 MB | 20 MiB (`ffmpeg/ffmpeg-core.wasm.part0`) |
| ClipSmith, GifSmith, CaptionSmith demos + hub | `npm run build -w @mediasmith/smiths-web` | `apps/smiths-web/dist/` | ~33 MB | 20 MiB (same) |

`apps/smiths-web/dist` holds `index.html` (a hub page linking the three demos),
`clip.html`, `gif.html` and `caption.html`. They share one copy of the engine.
Both builds use relative paths (`base: "./"`), so they work from any sub-folder.

Preview locally with `npx vite preview` in `app/` or `apps/smiths-web/`. It sends the
isolation headers below. Opening `index.html` from `file://` does not work,
because the engine has to be fetched.

## 2. The 25 MiB Cloudflare static-asset limit

Cloudflare (Workers static assets and Pages) rejects any single file over 25 MiB, and the ffmpeg.wasm core is 32,232,419
bytes (about 30.7 MiB). The `ffmpegCore()` Vite plugin
(`packages/platform/vite-ffmpeg.mjs`) handles this at build time. It:

- writes `ffmpeg/ffmpeg-core.js` (110 KB);
- splits the wasm into `ffmpeg-core.wasm.part0` (20 MiB) and `.part1` (10.7 MiB);
- adds `fonts/Inter.ttf` and `fonts/Syne.ttf`, used for caption and text burn-in;
- writes a `manifest.json` that lists the parts.

At runtime, `loadFfmpeg()` fetches the parts, joins them into one Blob URL and
loads the core from it. Every file is served from the same origin, so no CDN, R2
bucket or alternate host is needed, and the build works under COEP.

The desktop build skips the plugin (`TAURI_ENV_PLATFORM` is set during
`tauri build`), so the installers don't carry the wasm.

## 3. Headers

`public/_headers` (Cloudflare `_headers` format, read by Workers static assets) is copied into both builds:

```
/*
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Embedder-Policy: require-corp
  Cross-Origin-Resource-Policy: same-origin
```

The single-threaded core doesn't need SharedArrayBuffer, so these headers aren't
strictly required. They are kept so the multi-threaded core can be a drop-in
later. If a demo is embedded in another page through an iframe, that parent page
needs compatible headers.

## 4. What works in the browser and what is desktop only

Measured in headless Chromium on the bundled 8 s 720x1280 sample. The first export
also downloads the engine once (about 31 MB, cached after that).

| App | Works in the browser | Desktop only |
|---|---|---|
| **MediaSmith** | Home, templates, bundled sample project, timeline editing, text, captions (manual, SRT/VTT import and export), design templates, save/open project files, export of all 5 presets (9:16, 4:5, 16:9, 1:1 MP4 up to 960p, and GIF) with text and karaoke captions burned in via libass. The sample renders in about 10 s. | Native/hardware encoding at full resolution, Whisper auto-captions, speaker labels, AI rewrite (Ollama), link import, filmstrips |
| **ClipSmith** (`clip.html`) | Open or drop a local file, trim (typed or from the playhead), reframe (original, 9:16, 1:1, 16:9 centre crop), export MP4 (H.264 + AAC, up to 720p), MP3 or GIF. 8 s clip in about 7 s. | Link import (yt-dlp), any length or resolution, native-speed encoding |
| **GifSmith** (`gif.html`) | Open or drop a file, pick start and length (up to 15 s), crop, width, fps, loop or boomerang, palette GIF. 3 s boomerang in about 2 s. | Frame-accurate trim handles, size-target shrink, gallery, longer GIFs, link import |
| **CaptionSmith** (`caption.html`) | Import .srt/.vtt or type captions, edit and jump, style (font, size, colour, box/outline, position) with a live preview that matches the burn, export SRT, VTT or an MP4 with captions burned in (libass). 8 s clip in about 5 s. | Whisper transcription (local model), speaker labels, batch mode, link import |

Limits that apply to every web build:

- Input is capped at 500 MB, because wasm memory tops out around 2 GB and holds the input, the frames and the output.
- Output is capped at 720p (960p in MediaSmith).
- The single-threaded core is roughly 5 to 20 times slower than native ffmpeg.

## 5. Why some features stay on the desktop

- **Link import:** fetching another site's video is blocked by CORS, and yt-dlp
  can't run in a tab. Making it work would need a proxy server. Link import is
  optional everywhere and is labelled "optional, needs internet".
- **Whisper in the browser** is possible (whisper.wasm or transformers.js), but
  the 40 to 250 MB models and CPU-only speed make it a poor demo. The web demos
  ask for an SRT/VTT import or typed captions instead.
