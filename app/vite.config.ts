import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import ffmpegCore from "@mediasmith/platform/vite-ffmpeg";

// One bundle runs in Tauri and in a browser; the platform adapter picks the
// backend at runtime. The web build is fully static (base "./") and ships
// ffmpeg.wasm same-origin in <25 MiB parts (see docs/web.md).
// The desktop build (tauri sets TAURI_ENV_PLATFORM) uses native ffmpeg, so
// it doesn't carry the 31 MiB wasm core.
const desktop = !!process.env.TAURI_ENV_PLATFORM;

export default defineConfig({
  base: "./",
  plugins: [solid(), ...(desktop ? [] : [ffmpegCore()])],
  clearScreen: false,
  preview: { headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" } },
  server: {
    port: 1430,
    strictPort: true,
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  build: { target: "es2021" },
});
