# MediaSmith on the web: feasibility

The UI talks to `@mediasmith/platform`, never to Tauri directly. `getPlatform()`
returns the Tauri backend inside the desktop webview (`__TAURI_INTERNALS__`) and
the web backend in a normal tab. The web backend uses **ffmpeg.wasm**
(`@ffmpeg/ffmpeg`) entirely client-side: no server, no uploads.

## 1. GifSmith web demo (`apps/web-demo`)

- **Flow:** local file in (picker or drag and drop) → ffmpeg.wasm palette GIF
  (`palettegen`/`paletteuse`, same filter the desktop uses) → GIF out as a Blob
  download. Built with `npm run build -w @mediasmith/web-demo` to static files in
  `apps/web-demo/dist`, deployable to Cloudflare Pages as is.
- **Size limits:** wasm memory tops out around 2 GB, and the input, the decoded
  frames and the output all live in it. The demo caps input at 500 MB and GIF
  length at 15 s. Long or 4K sources should be trimmed first.
- **Speed:** the single-threaded core is roughly 5 to 20 times slower than native
  ffmpeg. Expect about 5 to 30 s for a 4 s 480p GIF on a modern laptop, plus a
  one-time ~30 MB core download (cached). The multi-threaded core (`@ffmpeg/core-mt`)
  gives about 2 to 3 times that speed but needs SharedArrayBuffer.
- **Headers (Cloudflare Pages):** SharedArrayBuffer requires cross-origin
  isolation. `apps/web-demo/public/_headers` ships:
  ```
  /*
    Cross-Origin-Opener-Policy: same-origin
    Cross-Origin-Embedder-Policy: require-corp
    Cross-Origin-Resource-Policy: same-origin
  ```
  With COEP on, every subresource must be same-origin or send CORP/CORS headers,
  so the ffmpeg core is bundled from `node_modules`, not loaded from a CDN. If the
  demo is embedded in another page (an iframe on a portfolio), that parent page
  needs the same headers too.
- **Cloudflare Pages file-size limit:** Pages rejects any single asset over 25 MiB,
  and the ffmpeg.wasm core is about 31 MiB (32.2 MB in the build output). To deploy,
  either serve `ffmpeg-core.wasm` from an R2 bucket on the *same* custom domain
  (a route like `/wasm/*`, sent with `Cross-Origin-Resource-Policy: same-origin`), or
  build a slimmer custom ffmpeg.wasm with only the GIF filters and decoders.
  The current build works with `vite preview` and any host without that limit.
- **URL import can't work in a browser.** Fetching another site's page or video
  is blocked by CORS, and yt-dlp is a Python program that can't run in a tab. Making
  it work would need a server-side proxy, which goes against "no server" and
  carries abuse and legal risk. Link import stays desktop only, and the web UI says so.

## 2. Feature split

| Feature | Desktop | Web |
|---|---|---|
| Open local video, trim, split, timeline | ✅ | ✅ (memory-limited) |
| GIF export | ✅ native ffmpeg | ✅ ffmpeg.wasm |
| MP4 export with presets | ✅ hardware encoders | ⚠️ libx264 in wasm, slow; short clips only |
| Captions styling / burn-in | ✅ | ✅ (canvas preview, wasm burn-in) |
| Auto-captions (whisper) | ✅ whisper.cpp sidecar, GPU | ⚠️ possible via transformers.js / whisper.wasm, but 40 to 250 MB models and slow on CPU; WebGPU helps |
| Auto-reframe (face/person) | ✅ tract ONNX | ⚠️ possible with onnxruntime-web; heavy |
| Link import (yt-dlp + scraper) | ✅ | ❌ CORS, no subprocesses |
| Hardware encode, large/4K files | ✅ | ❌ |
| Design canvas, templates | ✅ | ✅ |

Nothing in this repo deploys anywhere. Hosting the demo is a separate, manual step.
