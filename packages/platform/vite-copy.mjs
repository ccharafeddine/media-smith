// Copy extra same-origin static files into the build (and serve them in dev),
// e.g. onnxruntime-web's wasm and the detector models. Paths are relative to
// the repo root; each entry is [from, to-in-dist].
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";

// Workers static assets cap files at 25 MiB; larger files ship as <name>.partN
// plus <name>.parts (JSON list) and are joined at runtime by the app.
const PART = 20 * 1024 * 1024;
import { dirname, join, resolve } from "node:path";

/** @param {[string, string][]} entries Paths relative to the Vite root. */
export default function copyAssets(entries) {
  let root = process.cwd();
  let outDir = "dist";
  return {
    name: "smith-copy-assets",
    configResolved(c) { root = c.root; outDir = resolve(c.root, c.build.outDir); },
    buildStart() {
      for (const [from] of entries) if (!existsSync(join(root, from))) this.error(`missing ${from} (run the fetch script named in the vite config)`);
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = (req.url ?? "").split("?")[0];
        const hit = entries.find(([, to]) => url === "/" + to);
        if (!hit) return next();
        res.setHeader("Cross-Origin-Resource-Policy", "same-origin");
        res.setHeader("Content-Type", hit[1].endsWith(".wasm") ? "application/wasm" : hit[1].endsWith(".mjs") ? "text/javascript" : "application/octet-stream");
        res.end(readFileSync(join(root, hit[0])));
      });
    },
    writeBundle() {
      for (const [from, to] of entries) {
        const dest = join(outDir, to);
        mkdirSync(dirname(dest), { recursive: true });
        const src = join(root, from);
        if (statSync(src).size <= 24 * 1024 * 1024) { copyFileSync(src, dest); continue; }
        const buf = readFileSync(src);
        const names = [];
        for (let i = 0, o = 0; o < buf.length; i++, o += PART) {
          const n = `${dest.split(/[\\/]/).pop()}.part${i}`;
          writeFileSync(join(dirname(dest), n), buf.subarray(o, o + PART));
          names.push(n);
        }
        writeFileSync(dest + ".parts", JSON.stringify({ parts: names, bytes: buf.length }));
      }
    },
  };
}
