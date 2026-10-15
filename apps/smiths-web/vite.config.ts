import { resolve } from "node:path";
import { defineConfig } from "vite";
import ffmpegCore from "@mediasmith/platform/vite-ffmpeg";
import smithSite from "@mediasmith/platform/vite-site";
import { SITES } from "@mediasmith/ui/sites";

const isolation = { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" };
const page = (n: string) => resolve(import.meta.dirname, n);

// Static, server-free web demos of ClipSmith, GifSmith and CaptionSmith. One
// shared copy of ffmpeg.wasm (split under Cloudflare Pages' 25 MiB per-file
// cap by the ffmpegCore plugin). Prod headers: public/_headers.
//
// `SMITH_SITE=clip|gif|caption vite build` instead produces a standalone site
// for that app's own subdomain root (base "/") in dist-site/<app>/.
const site = process.env.SMITH_SITE as "clip" | "gif" | "caption" | undefined;
if (site && !(site in SITES) || site === ("media" as string)) throw new Error(`Unknown SMITH_SITE ${site}`);

export default defineConfig({
  base: site ? "/" : "./",
  plugins: [
    ffmpegCore(),
    ...(site ? [smithSite({ ...SITES[site], page: `${site}.html`, og: resolve(import.meta.dirname, `../../packages/ui/og/${SITES[site].app}.png`) })] : []),
  ],
  server: { headers: isolation },
  preview: { headers: isolation },
  optimizeDeps: { exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"] },
  build: {
    target: "es2021",
    outDir: site ? `dist-site/${site}` : "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: site ? { [site]: page(`${site}.html`) } : { index: page("index.html"), clip: page("clip.html"), gif: page("gif.html"), caption: page("caption.html") },
    },
  },
});
