import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { resolve } from "node:path";
import ffmpegCore from "@mediasmith/platform/vite-ffmpeg";
import smithSite from "@mediasmith/platform/vite-site";
import { SITES } from "@mediasmith/ui/sites";

// One bundle runs in Tauri and in a browser; the platform adapter picks the
// backend at runtime. The web build is fully static (base "./") and ships
// ffmpeg.wasm same-origin in <25 MiB parts (see docs/web.md).
// The desktop build (tauri sets TAURI_ENV_PLATFORM) uses native ffmpeg, so
// it doesn't carry the 31 MiB wasm core.
const desktop = !!process.env.TAURI_ENV_PLATFORM;
// `SMITH_SITE=media vite build`: standalone site for media.chafic.me (base "/",
// SPA fallback, OG meta, _headers) in dist-site/media/.
const site = !desktop && process.env.SMITH_SITE === "media";

export default defineConfig({
  base: site ? "/" : "./",
  plugins: [
    solid(),
    ...(desktop ? [] : [ffmpegCore()]),
    ...(site ? [smithSite({ ...SITES.media, og: resolve(import.meta.dirname, "../packages/ui/og/mediasmith.png") })] : []),
  ],
  clearScreen: false,
  preview: { headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" } },
  server: {
    port: 1430,
    strictPort: true,
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  build: { target: "es2021", outDir: site ? "dist-site/media" : "dist", emptyOutDir: true },
});
