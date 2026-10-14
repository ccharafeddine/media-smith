import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Same bundle runs in Tauri and in a browser; the platform adapter picks the
// backend at runtime. COOP/COEP enable SharedArrayBuffer for multi-threaded
// ffmpeg.wasm in dev (prod: see docs/web.md and apps/web-demo/public/_headers).
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  build: { target: "es2021" },
});
