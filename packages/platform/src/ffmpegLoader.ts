import type { FFmpeg } from "@ffmpeg/ffmpeg";

/** ffmpeg.wasm, loaded from this site (never a CDN, so it works offline once
 *  cached and under COEP). The ~31 MiB core wasm is served in parts below
 *  Cloudflare Pages' 25 MiB per-file limit and joined in memory; see
 *  `vite-ffmpeg.mjs`, which writes `<base>ffmpeg/manifest.json`. */
export interface CoreManifest { core: string; parts: string[]; bytes: number; fonts?: string[] }

let base = "./";
let ffmpeg: Promise<FFmpeg> | null = null;

/** Where the app's static root is (Vite's BASE_URL). Call before first use. */
export function configureFfmpeg(baseUrl: string): void {
  base = baseUrl.endsWith("/") ? baseUrl : baseUrl + "/";
}

export function partUrls(m: CoreManifest, root: string): { core: string; parts: string[] } {
  return { core: root + "ffmpeg/" + m.core, parts: m.parts.map((p) => root + "ffmpeg/" + p) };
}

async function joinParts(urls: string[], onBytes?: (n: number) => void): Promise<Blob> {
  const blobs: Blob[] = [];
  let n = 0;
  for (const u of urls) {
    const r = await fetch(u);
    if (!r.ok) throw new Error(`Couldn't load the video engine (${u}: HTTP ${r.status})`);
    const b = await r.blob();
    n += b.size;
    onBytes?.(n);
    blobs.push(b);
  }
  return new Blob(blobs, { type: "application/wasm" });
}

/** Lazily load ffmpeg.wasm. `onLoad(ratio)` reports the one-time download. */
export function loadFfmpeg(onLoad?: (ratio: number) => void): Promise<FFmpeg> {
  ffmpeg ??= (async () => {
    const res = await fetch(base + "ffmpeg/manifest.json");
    if (!res.ok) throw new Error("The video engine isn't part of this build (ffmpeg/manifest.json missing).");
    const m = (await res.json()) as CoreManifest;
    const urls = partUrls(m, new URL(base, location.href).href);
    const wasm = await joinParts(urls.parts, (n) => onLoad?.(Math.min(1, n / m.bytes)));
    const { FFmpeg } = await import("@ffmpeg/ffmpeg");
    const f = new FFmpeg();
    await f.load({ coreURL: urls.core, wasmURL: URL.createObjectURL(wasm) });
    // Fonts for libass burn-in (text + captions), bundled so export works offline.
    await f.createDir("/fonts").catch(() => undefined);
    for (const name of m.fonts ?? []) {
      const r = await fetch(new URL(base, location.href).href + "ffmpeg/fonts/" + name);
      if (r.ok) await f.writeFile("/fonts/" + name, new Uint8Array(await r.arrayBuffer()));
    }
    return f;
  })();
  ffmpeg.catch(() => (ffmpeg = null));
  return ffmpeg;
}
