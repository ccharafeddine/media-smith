# MediaSmith conventions (for AI coding agents and humans)

- **Identity:** commits are authored as `ccharafeddine`. No `Co-authored-by` trailers.
  Before committing, check `git config user.name` and scan the diff and message for
  pseudonym strings; abort if any match.
- **Branches:** work on feature branches (`scaffold/phase-1`, `feat/...`). Never push
  to `main` without the owner's OK. No tags or releases unless asked.
- **Layering:** logic lives in `crates/*` with no Tauri dependency and unit tests next
  to it. `app/src-tauri` only wires commands to crates. The UI calls
  `@mediasmith/platform`, never `@tauri-apps/api` directly, so it runs on web too.
- **Porting:** when lifting code from ClipSmith/GifSmith/CaptionSmith, bring its tests,
  keep its doc comments, and note the source in the module doc.
- **Import:** yt-dlp first for every page URL; the scrape fallback stays generic with
  no site-specific code. Source quality by default (`bv*[acodec=none]+ba/b/bv*+ba`).
- **Design:** tokens live in `packages/ui/src/tokens.css`. Don't hardcode colours in
  components; add a token. Syne for display, the system UI font for dense chrome.
- **Sidecars:** fetched by `scripts/fetch-*.sh` into `app/src-tauri/binaries/`
  (gitignored). Never commit binaries.
- **Checks before pushing:** `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`,
  `cargo test --workspace`, `npm run typecheck`, `npm test`, `npm run build`.
- **Disk:** on the dev PC, use `CARGO_TARGET_DIR=C:\Users\mnc-9\.cargo-target-mediasmith`
  and `cargo clean` it when free space is under 10 GB.
- Keep `progress.txt` updated at the end of each work session.
