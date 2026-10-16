# smith-fetch

The URL download engine shared by ClipSmith, GifSmith and MediaSmith.

For a pasted link it tries, in order:

1. **Direct links.** Video files are fetched over HTTP. HLS (`.m3u8`) and DASH (`.mpd`) manifests are remuxed by ffmpeg, which picks the best rendition and copies the streams without re-encoding.
2. **yt-dlp**, which supports about 1,800 sites:
   - It updates itself: checked once a day, plus a rate-limited check after a failure that an update might fix. Every update is verified against the release's SHA-256 and test-run before it replaces anything. The bundled copy stays as the fallback.
   - It retries with `--impersonate chrome` on HTTP 403 errors and bot walls.
   - It downloads Deno on first YouTube use, because YouTube's JavaScript challenges need it.
   - It accepts cookies from in-app sign-in, an imported `cookies.txt`, or a browser profile. Chrome and Edge cookies are flagged as unreliable on Windows.
3. **The generic page scraper.** It reads `<video>` tags, data attributes, meta tags and JSON-LD, plus player configs, base64-hidden links, packed JS, and embedded players (iframes, Brightcove, Wistia, Vimeo and YouTube markers).
4. **A hidden sniff pass.** The page loads in an invisible window of the app's own browser for up to 15 seconds, and the engine records the media the player requests.

**Browse mode** opens the same in-app browser visibly. You play the video and pick it from the list. It's also where you sign in to a site and save its cookies for that site only.

The Tauri glue (commands, browser window, cookie capture and an opt-in clipboard watcher) is behind the `tauri` feature.

## What it never does

- It never circumvents DRM. Widevine, PlayReady, FairPlay, SAMPLE-AES and protected DASH are detected and skipped.
- It never captures packets, installs a root certificate, or intercepts other apps' traffic.
- It has no site-specific logic for adult sites.

## Legal

Download only media you have the right to save: your own uploads, public-domain or Creative Commons works, or content whose owner permits downloading. Many websites' Terms of Service prohibit downloading. You are responsible for complying with them and with copyright law in your country. The apps do **not** bypass DRM or other technical protection measures; protected streams are detected and skipped. Sign-in cookies stay on your computer and are only sent to the site they came from.

yt-dlp (Unlicense) and Deno (MIT) are downloaded from their official GitHub releases at runtime.

## Smoke test

`cargo run -p smith-fetch --bin smith-fetch-smoke -- crates/smith-fetch/smoke/urls.txt <path to yt-dlp>` checks the public, safe, non-DRM URLs in `smoke/urls.txt`. It runs nightly in `.github/workflows/smith-fetch-smoke.yml`.