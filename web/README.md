# web/dist

A ready-to-host static build of the MediaSmith web demo for **media.chafic.me** (base `/`).
Rebuild it from the repo root with:

```sh
SMITH_SITE=media npm run build -w @mediasmith/app && rm -rf web/dist && cp -r app/dist-site/media web/dist
```

It needs the COOP/COEP headers in `_headers` and the SPA fallback in `_redirects`. See `../docs/hosting.md`.
