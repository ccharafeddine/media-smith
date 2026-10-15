import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { resolve } from "node:path";
import { existsSync } from "node:fs";
import ffmpegCore from "@mediasmith/platform/vite-ffmpeg";
import smithSite from "@mediasmith/platform/vite-site";
import copyAssets from "@mediasmith/platform/vite-copy";
import { SITES } from "@mediasmith/ui/sites";

// One bundle runs in Tauri and in a browser; the platform adapter picks the
// backend at runtime. The web build is fully static (base "./") and ships
// ffmpeg.wasm same-origin in <25 MiB parts (see docs/web.md).
// The desktop build (tauri sets TAURI_ENV_PLATFORM) uses native ffmpeg, so
// it doesn't carry the 31 MiB wasm core, nor the browser Whisper model
// (fetch it with scripts/fetch-whisper-web.sh before a web build).
const desktop = !!process.env.TAURI_ENV_PLATFORM;
// `SMITH_SITE=media vite build`: standalone site for media.chafic.me (base "/",
// SPA fallback, OG meta, _headers) in dist-site/media/.
const site = !desktop && process.env.SMITH_SITE === "media";
const WHISPER = ["config.json", "generation_config.json", "preprocessor_config.json", "tokenizer.json", "tokenizer_config.json",
  "onnx/encoder_model_quantized.onnx", "onnx/decoder_model_merged_quantized.onnx"];

export default defineConfig({
  base: site ? "/" : "./",
  plugins: [
    solid(),
    ...(desktop ? [] : [ffmpegCore()]),
    // Browser auto-captions: the site build (and `dev:web` once fetched) serves Whisper same-origin.
    ...(site || (!desktop && existsSync(resolve(import.meta.dirname, ".whisper-web"))) ? [copyAssets(WHISPER.map((f) => [`.whisper-web/whisper-base_timestamped/${f}`, `models/whisper-base_timestamped/${f}`] as [string, string]))] : []),
    ...(site ? [smithSite({ ...SITES.media, og: resolve(import.meta.dirname, "../packages/ui/og/mediasmith.png") })] : []),
  ],
  resolve: desktop ? { alias: [{ find: /^\.\/webCaptions$/, replacement: resolve(import.meta.dirname, "../packages/platform/src/webCaptions.stub.ts") }] } : undefined,
  clearScreen: false,
  preview: { headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" } },
  server: {
    port: 1430,
    strictPort: true,
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  worker: { format: "es" },
  build: { target: "es2021", outDir: site ? "dist-site/media" : "dist", emptyOutDir: true },
});
