import type { FFmpeg } from "@ffmpeg/ffmpeg";
import { gifArgs, type Platform } from "./types";

let ffmpeg: Promise<FFmpeg> | null = null;

/** Lazily load ffmpeg.wasm (single-threaded core from the same origin, so it
 *  works without COOP/COEP; the multi-threaded core needs those headers). */
export function loadFfmpeg(coreURL?: string, wasmURL?: string): Promise<FFmpeg> {
  ffmpeg ??= (async () => {
    const { FFmpeg } = await import("@ffmpeg/ffmpeg");
    const f = new FFmpeg();
    await f.load(coreURL && wasmURL ? { coreURL, wasmURL } : undefined);
    return f;
  })();
  return ffmpeg;
}

function emptyProject(name: string, width: number, height: number, fps: number): string {
  const tracks = [
    ["t1", "video", "Video 1"], ["t2", "video", "Video 2"], ["t3", "audio", "Audio"], ["t4", "caption", "Captions"],
  ].map(([id, kind, n]) => ({ id, kind, name: n, muted: false, items: [] }));
  return JSON.stringify({ version: 1, name, canvas: { width, height, fps }, tracks, next_id: 4 }, null, 2);
}

export const webPlatform: Platform = {
  kind: "web",
  caps: { urlImport: false, hardwareEncode: false, transcribe: "wasm", maxInputBytes: 500 * 1024 * 1024 },
  newProject: async (name, w, h, fps) => emptyProject(name, w, h, fps),
  classifyUrl: async () => {
    throw new Error("Link import needs the desktop app (browsers block cross-site video fetches).");
  },
  toGif: async (input, opts, onProgress) => {
    if (typeof input === "string") throw new Error("The web build takes a local File, not a path.");
    if (input.size > webPlatform.caps.maxInputBytes) throw new Error("File too large for in-browser processing.");
    const f = await loadFfmpeg();
    const { fetchFile } = await import("@ffmpeg/util");
    const handler = ({ progress }: { progress: number }) => onProgress?.(Math.min(1, Math.max(0, progress)));
    f.on("progress", handler);
    try {
      const inName = "in." + (input.name.split(".").pop() || "mp4");
      await f.writeFile(inName, await fetchFile(input));
      await f.exec(gifArgs(inName, "out.gif", opts));
      const data = (await f.readFile("out.gif")) as Uint8Array;
      await f.deleteFile(inName);
      await f.deleteFile("out.gif");
      return new Blob([data.slice()], { type: "image/gif" });
    } finally {
      f.off("progress", handler);
    }
  },
};
