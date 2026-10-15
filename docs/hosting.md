# Hosting the web demos

Each Smith app has a standalone static build for its own subdomain root. All four
use only static files: no server, no CDN, no uploads.

| Site | Repo | Build command | Output (committed) |
|---|---|---|---|
| media.chafic.me | MediaSmith | `sh scripts/fetch-whisper-web.sh && SMITH_SITE=media npm run build -w @mediasmith/app`, then copy `app/dist-site/media/` | `web/dist/` |
| clip.chafic.me | ClipSmith | `npm run build:web` | `web/dist/` |
| gif.chafic.me | GifSmith | `npm run build:web` | `web/dist/` |
| caption.chafic.me | CaptionSmith | `npm run build:web` | `web/dist/` |

Each demo is built from its app's own SolidJS frontend, so it has the same interface as the desktop app. MediaSmith picks its platform adapter at runtime (`packages/platform/src/web.ts`). ClipSmith, GifSmith and CaptionSmith alias `@tauri-apps/*` to a `web/shim/` that runs their Rust commands in the browser (`web/commands.ts`). `apps/smiths-web`, the earlier simplified single-page demos, is superseded and no longer deployed.

On Windows PowerShell, run `$env:SMITH_SITE="media"; npm run build -w @mediasmith/app`.

## What each build contains

- `index.html`, built with base `/`. It includes the canonical URL, description,
  Open Graph and Twitter card meta, and `og.png` (1200x630).
- `404.html`, a copy of `index.html`, for hosts like GitHub Pages.
- `_headers`, needed. See below.
- `ffmpeg/`, the video engine:
  - `ffmpeg-core.js`;
  - the wasm split into `ffmpeg-core.wasm.part0` (20 MiB) and `part1` (10.7 MiB);
  - `fonts/`;
  - `manifest.json`.
- `assets/`, the hashed JS, CSS and fonts.
- `models/`, only where the demo runs a model: Whisper base for MediaSmith and
  CaptionSmith (the decoder is split into `.partN` files listed in `.parts`), and
  the Auto-Reframe detectors for ClipSmith (`ort/` holds their runtime).

Builds are 32 MB (GifSmith), 48 MB (ClipSmith) and about 130 MB (CaptionSmith, MediaSmith).
No single file is over 21 MiB, under the Cloudflare 25 MiB per-file limit.

## Required headers

```
/*
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Embedder-Policy: require-corp
  Cross-Origin-Resource-Policy: same-origin
```

These keep ffmpeg.wasm cross-origin isolated (`crossOriginIsolated === true`).
Workers static assets applies them from `_headers` automatically (including to the
SPA fallback, because rules match the request URL); on other hosts,
configure them by hand. The builds load nothing from other origins. If a demo is
ever embedded in an iframe on another page, that page needs compatible
COOP/COEP headers too, so linking to the subdomain is simpler.

## Building for another domain

The only place a build names a domain is the absolute meta in `index.html` and `404.html`:
`<link rel="canonical">`, `og:url`, `og:image` and `twitter:image`. By default they point at
the chafic.me subdomain (`packages/ui/src/sites.mjs` here, `web/site.json` in the sibling repos) (for example `https://clip.chafic.me/`
and `https://clip.chafic.me/og.png`), and the committed `web/dist` builds use that default.

To host a build somewhere else, set `SMITH_SITE_URL` to the site's public URL when you build:

```sh
SMITH_SITE=media SMITH_SITE_URL=https://video.example.com/ npm run build -w @mediasmith/app
SMITH_SITE_URL=https://clips.example.com npm run build:web   # in ClipSmith, GifSmith or CaptionSmith
```

On Windows PowerShell: `$env:SMITH_SITE_URL="https://clips.example.com"; npm run build:web`
(then `Remove-Item Env:SMITH_SITE_URL` to go back to the default).

- It must be an absolute `http(s)` URL with no query or fragment; a trailing `/` is added if missing.
  Anything else stops the build with an error.
- Leave it unset (or empty) for the default chafic.me URL.
- It changes only those four tags. The asset paths stay root-relative (base `/`), so the build still
  has to be served from the root of its domain or subdomain.
- Everything else (`_headers`, `wrangler.jsonc`, the app itself) is domain-independent. Rename the
  Worker in `wrangler.jsonc` if you deploy a second copy under the same account.

## Deploying with Workers static assets

Each repo has a root `wrangler.jsonc` with no routes. It serves `./web/dist` as static assets:

```jsonc
{
  "name": "mediasmith", // clipsmith, gifsmith, captionsmith in the other repos
  "compatibility_date": "2026-10-01",
  "assets": { "directory": "./web/dist", "not_found_handling": "single-page-application" }
}
```

- `not_found_handling: single-page-application` serves `index.html` (status 200) for unknown
  paths, so deep links work. This replaces the old `_redirects` file, which the builds no longer ship.
- `_headers` is read by Workers static assets and is not served itself. It only applies to asset
  responses; there is no Worker script, so every response gets the COOP/COEP headers.
- Attach the custom domain (for example `media.chafic.me`) to the Worker in the dashboard or by adding
  `routes` later. The config deliberately has none.
- Static assets allow 25 MiB per file, which the split engine fits.

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
