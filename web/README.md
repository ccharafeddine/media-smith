# web/dist

A ready-to-host static build of the MediaSmith web demo for **media.chafic.me** (base `/`).
Rebuild it from the repo root with:

```sh
SMITH_SITE=media npm run build -w @mediasmith/app && rm -rf web/dist && cp -r app/dist-site/media web/dist
```

To build for another domain, set `SMITH_SITE_URL` (for example `SMITH_SITE_URL=https://video.example.com`); it only changes the canonical, `og:url`, `og:image` and `twitter:image` URLs, which default to https://media.chafic.me/. See "Building for another domain" in `../docs/hosting.md`.

It is served by Workers static assets through the root `wrangler.jsonc` (`assets.directory: ./web/dist`, SPA fallback via `not_found_handling`). The COOP/COEP headers come from `_headers`. See `../docs/hosting.md`.
