// @ts-check
/** Standalone web builds, one per subdomain (see docs/web.md). */
export const SITES = {
  media: { app: "mediasmith", url: "https://media.chafic.me/", title: "MediaSmith: edit, caption and export short video in your browser", description: "A local-first video editor that runs entirely in your browser. Templates, text, karaoke captions and MP4 or GIF export with ffmpeg.wasm. Nothing is uploaded." },
  clip: { app: "clipsmith", url: "https://clip.chafic.me/", title: "ClipSmith: trim and reframe video in your browser", description: "Trim, reframe to 9:16, 1:1 or 16:9 and export MP4, MP3 or GIF, entirely in your browser. Nothing is uploaded." },
  gif: { app: "gifsmith", url: "https://gif.chafic.me/", title: "GifSmith: turn video into a GIF in your browser", description: "Pick a moment, crop it and export a looping or boomerang GIF, entirely in your browser. Nothing is uploaded." },
  caption: { app: "captionsmith", url: "https://caption.chafic.me/", title: "CaptionSmith: style and burn in captions in your browser", description: "Import SRT or VTT or type captions, style them over a live preview and burn them into an MP4, entirely in your browser. Nothing is uploaded." },
};
