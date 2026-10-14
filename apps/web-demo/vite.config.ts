import { defineConfig } from "vite";

const isolation = { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" };

// Static site, no server. ffmpeg.wasm core is bundled from node_modules so it
// is same-origin (works under COEP). Prod headers: public/_headers.
export default defineConfig({
  base: "./",
  server: { headers: isolation },
  preview: { headers: isolation },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  build: { target: "es2021" },
});
