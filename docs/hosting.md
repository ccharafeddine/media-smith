# Hosting the web demos

Each Smith app has a standalone static build for its own subdomain root. All four
use only static files: no server, no CDN, no uploads.

| Site | Build command (in the MediaSmith repo) | Output | Also committed at |
|---|---|---|---|
| media.chafic.me | `SMITH_SITE=media npm run build -w @mediasmith/app` | `app/dist-site/media/` | MediaSmith `web/dist/` |
| clip.chafic.me | `SMITH_SITE=clip npm run build -w @mediasmith/smiths-web` | `apps/smiths-web/dist-site/clip/` | ClipSmith `web/dist/` |
| gif.chafic.me | `SMITH_SITE=gif npm run build -w @mediasmith/smiths-web` | `apps/smiths-web/dist-site/gif/` | GifSmith `web/dist/` |
| caption.chafic.me | `SMITH_SITE=caption npm run build -w @mediasmith/smiths-web` | `apps/smiths-web/dist-site/caption/` | CaptionSmith `web/dist/` |

On Windows PowerShell, run `$env:SMITH_SITE="clip"; npm run build -w @mediasmith/smiths-web`.

## What each build contains

- `index.html`, built with base `/`. It includes the canonical URL, description,
  Open Graph and Twitter card meta, and `og.png` (1200x630).
- `404.html`, a copy of `index.html`, for hosts like GitHub Pages.
- `_redirects` with `/* /index.html 200`, the SPA fallback for Cloudflare Pages.
- `_headers`, needed. See below.
- `ffmpeg/`, the video engine:
  - `ffmpeg-core.js`;
  - the wasm split into `ffmpeg-core.wasm.part0` (20 MiB) and `part1` (10.7 MiB);
  - `fonts/`;
  - `manifest.json`.
- `assets/`, the hashed JS, CSS, fonts and sample clip.

Every build is about 33 MB. The largest single file is 20 MiB, under Cloudflare
Pages' 25 MiB per-file limit.

## Required headers

```
/*
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Embedder-Policy: require-corp
  Cross-Origin-Resource-Policy: same-origin
```

These keep ffmpeg.wasm cross-origin isolated (`crossOriginIsolated === true`).
Cloudflare Pages applies them from `_headers` automatically; on other hosts,
configure them by hand. The builds load nothing from other origins. If a demo is
ever embedded in an iframe on another page, that page needs compatible
COOP/COEP headers too, so linking to the subdomain is simpler.

## Known limits

- The first export downloads the engine once (about 31 MB) and the browser caches it.
- Input is capped at 500 MB. Output is capped at 720p (960p in MediaSmith).
- GifSmith's web GIFs are capped at 15 s.
- Encoding uses the single-threaded wasm core, roughly 5 to 20 times slower than the desktop apps.
- Desktop only: link import, Whisper transcription, speaker labels, AI rewrite,
  hardware encoding and full-resolution export. Each page says so in its footer.
- The bundled sample clip ("Try the sample clip" or "Try the sample") works offline.

## Listing assets

| App | Blurb | Logo | Screenshots |
|---|---|---|---|
| MediaSmith | A local-first video editor that runs in the browser: templates, text, karaoke captions and MP4/GIF export. Nothing is uploaded. | `packages/ui/src/brand/mediasmith.svg` | `docs/screenshots/home.png`, `editor.png`, `export.png`, `packages/ui/og/mediasmith.png` |
| ClipSmith | Trim, reframe and export clips (MP4, MP3, GIF). Frame-accurate and fully offline on the desktop. | ClipSmith `docs/brand/clipsmith.svg` | ClipSmith `docs/screenshots/*.png`, `packages/ui/og/clipsmith.png` |
| GifSmith | Turn any moment of a video into a looping or boomerang GIF. | GifSmith `docs/brand/gifsmith.svg` | GifSmith `docs/screenshots/*.png`, `packages/ui/og/gifsmith.png` |
| CaptionSmith | Caption videos with on-device Whisper; style captions and burn them in. | CaptionSmith `docs/brand/captionsmith.svg` | CaptionSmith `docs/screenshots/*.png`, `packages/ui/og/captionsmith.png` |
