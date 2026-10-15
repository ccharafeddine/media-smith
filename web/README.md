# web/dist

A ready-to-host static build of the MediaSmith web demo for **media.chafic.me** (base `/`).
Rebuild it from the repo root with:

```sh
SMITH_SITE=media npm run build -w @mediasmith/app && rm -rf web/dist && cp -r app/dist-site/media web/dist
```

It is served by Workers static assets through the root `wrangler.jsonc` (`assets.directory: ./web/dist`, SPA fallback via `not_found_handling`). The COOP/COEP headers come from `_headers`. See `../docs/hosting.md`.
