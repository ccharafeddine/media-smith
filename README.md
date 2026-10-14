# MediaSmith

**An open-source, local-first CapCut / Canva for the desktop (and the browser).**
MediaSmith merges the Smith apps, [ClipSmith](https://github.com/ccharafeddine/ClipSmith),
GifSmith and CaptionSmith, into one editor: import from a file or a link, cut on a
multi-track timeline, auto-caption, reframe for vertical, design thumbnails, and
export to TikTok, Reels, Shorts, YouTube or GIF. No accounts, no watermark, no cloud.

Licensed GPL-3.0-or-later. Built with Tauri 2, Rust, SolidJS and FFmpeg.

## Layout

```
crates/
  ms-media/     ffprobe parsing, ffmpeg runner, hardware encoder selection (from ClipSmith)
  ms-import/    yt-dlp routing + generic any-site scrape fallback (from ClipSmith v2.1.0)
  ms-project/   timeline model: tracks, clips, text and caption layers, JSON save/load
app/            SolidJS editor UI (Vite)
  src-tauri/    thin Tauri 2 shell; each command delegates to a crate
packages/
  ui/           shared Smith design tokens (Syne, near-black, violet + cyan) and components
  platform/     platform adapter: Tauri invoke on desktop, ffmpeg.wasm in a browser
apps/
  web-demo/     GifSmith-style in-browser video-to-GIF demo (ffmpeg.wasm, static)
docs/           design mockups, web feasibility notes
scripts/        sidecar fetchers (ffmpeg, yt-dlp); binaries are gitignored
```

## Develop

```sh
npm install
npm run dev:web          # editor UI in a browser (web platform)
npm run tauri dev        # desktop app (fetch sidecars first: scripts/fetch-*.sh)
cargo test --workspace   # Rust unit tests
npm run typecheck && npm test && npm run build
```

## Roadmap

**Phase 1, scaffold (this branch).** Monorepo, ported ClipSmith core and import,
project model, an editor shell with mock timeline, platform adapter, web GIF demo, CI.

**Phase 2, a usable editor (v0.2).** Real playback from the project model; file and
link import; two video tracks plus audio and captions; trim, split, move and snapping;
export through ffmpeg with presets; save and open `.msproj`.

**Phase 3, tools.** Port ClipSmith reframe (face/person tracking), GifSmith palette
GIF export, CaptionSmith whisper captions with styles and word highlighting; crossfades.

**Phase 4, Design.** A Canva-style canvas: templates, text, shapes and stickers,
thumbnails and cover frames, brand kit.

**Later.** Plugin API so future Smith tools drop in as modules, signed installers and
updates, GPU preview compositor, macOS and Linux builds.

See [`docs/web.md`](docs/web.md) for what runs in the browser and
[`docs/mockups/`](docs/mockups) for the proposed look.
