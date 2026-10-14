# Bring-your-own-key AI (plan)

MediaSmith won't run a server or resell AI. Users plug in their own provider
keys, and the app calls the provider **directly from the client**. Status:
plan only. The UI has a disabled stub at **Settings > AI providers**.

## Settings > AI providers

| Provider | Text | Image generation / edit | Vision (frames in) | Notes |
|---|---|---|---|---|
| OpenAI | ✅ | ✅ `gpt-image-1` (edits take reference images) | ✅ | |
| Google Gemini | ✅ | ✅ Gemini image models (multi-image reference) | ✅ | |
| Anthropic | ✅ | ❌ | ✅ | text, captions and vision only |
| OpenAI-compatible | ✅ | depends on server | depends on model | base URL + optional key; covers **Ollama** (`http://localhost:11434/v1`), LM Studio, OpenRouter, vLLM |

For each provider: key (or base URL), default text model, default image model,
a **Test connection** button (lists models), and a per-feature provider choice
("Thumbnails: Gemini, Captions: Ollama"). An "AI off" master switch hides every
AI button.

## Key storage

- **Desktop:** keys go in the OS keychain through the Rust
  [`keyring`](https://crates.io/crates/keyring) crate (Windows Credential
  Manager, macOS Keychain, Secret Service on Linux), under service
  `com.ccharafeddine.mediasmith`, one entry per provider. Settings JSON stores
  only *which* providers are configured, never the secret. A new crate
  `ms-ai` owns `set_key` / `get_key` / `delete_key` and the request clients, so
  keys never cross into the webview. The UI asks Rust to make the call.
- Keys are never logged, never written to project files, never included in
  crash reports, and are redacted in error messages.

## Requests

- Desktop requests go from Rust (`reqwest`) straight to the provider. That gives
  no CORS issues, the key stays out of JS, and the user's own quota and billing apply.
- A small adapter trait (`TextModel`, `ImageModel`, `VisionModel`) with one
  implementation per provider family. OpenAI-compatible reuses the OpenAI client
  with a custom base URL.
- Every AI action shows the provider, the model and a rough cost before it runs,
  and can be cancelled.

## Features

1. **AI thumbnails (Design page).** The user marks 1 to 4 frames on the timeline
   (or the app suggests sharp, face-forward frames using the ClipSmith detector).
   Frames are grabbed with ffmpeg at source resolution and sent as **reference
   images** along with a prompt and style preset ("bold YouTube", "minimal",
   "meme"). Results land on the Design canvas as **editable layers**: the
   generated background as an image layer, and title text as real text layers
   (we ask the model for text-free art and set the words ourselves, so they are
   crisp and editable), plus an optional cut-out subject layer. Generate 4 and
   keep what you like.
2. **Title and description ideas.** Built from the transcript plus a few
   frames, giving 10 titles, 3 descriptions, hashtags, and chapter markers from caption
   timestamps. Platform-aware length limits (YouTube, TikTok, Reels).
3. **Caption rewrite.** Tighten, fix punctuation, change tone, translate, or add
   emoji. Rewrites the caption track's text only, and keeps every timestamp, so
   timing never drifts. Shown as a diff you accept per line.
4. **Best-moment clip picks.** Transcript + scene changes + audio energy go to a
   text model, which returns ranked `[start, end, reason]` spans. These show as
   markers on the timeline, and one click turns a span into a 9:16 clip with reframe.

## Web version caveats

- There's no keychain in a browser. Keys go in `localStorage`/IndexedDB on this
  device only, opt-in, with a clear warning and a "forget keys" button. Any XSS
  or browser extension could read them, so the web demo never ships with a
  key, and a strict CSP is required.
- **CORS:** requests come from the page. Gemini and OpenAI accept browser
  calls; Anthropic needs its `anthropic-dangerous-direct-browser-access` header;
  a local Ollama needs `OLLAMA_ORIGINS` set to the site's origin. Some
  OpenAI-compatible hosts block browsers entirely. The fix would be a proxy,
  which we won't run, so those providers are desktop only.
- Large reference images are downscaled in the browser before upload.

## Order of work

1. `ms-ai` crate: keychain storage + OpenAI-compatible text client + tests with
   a mock HTTP server. 2. Settings screen (real). 3. Caption rewrite (smallest
   user-visible win). 4. Title ideas. 5. Thumbnails on the Design page (needs the
   layer canvas). 6. Best-moment picks.
