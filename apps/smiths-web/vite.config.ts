import { resolve } from "node:path";
import { defineConfig } from "vite";
import ffmpegCore from "@mediasmith/platform/vite-ffmpeg";

const isolation = { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" };
const page = (n: string) => resolve(import.meta.dirname, n);

// Static, server-free web demos of ClipSmith, GifSmith and CaptionSmith. One
// shared copy of ffmpeg.wasm (split under Cloudflare Pages' 25 MiB per-file
// cap by the ffmpegCore plugin). Prod headers: public/_headers.
export default defineConfig({
  base: "./",
  plugins: [ffmpegCore()],
  server: { headers: isolation },
  preview: { headers: isolation },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  build: {
    target: "es2021",
    rollupOptions: { input: { index: page("index.html"), clip: page("clip.html"), gif: page("gif.html"), caption: page("caption.html") } },
  },
});
