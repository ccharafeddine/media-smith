import { gifArgs, PRESET_SIZES, safeName, type MediaRef, type Platform } from "./types";
import { loadFfmpeg } from "./ffmpegLoader";
import { planRender, renderMp4, webSize } from "./webRender";

const AI_WEB = "AI rewrite is optional and runs in the desktop app (local Ollama or your own key). Everything else works without it.";

/** Imported Files live only for this tab; projects reference them as web:<n>. */
const files = new Map<string, { file: File; url: string }>();
let nextHandle = 0;

export function registerFile(file: File): string {
  const source = `web:${++nextHandle}`;
  files.set(source, { file, url: URL.createObjectURL(file) });
  return source;
}

function pickFiles(accept: string, multiple: boolean): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.multiple = multiple;
    input.onchange = () => resolve(Array.from(input.files ?? []));
    input.addEventListener("cancel", () => resolve([]));
    input.click();
  });
}

function probeInBrowser(url: string): Promise<{ duration_secs: number; width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const v = document.createElement("video");
    v.preload = "metadata";
    v.onloadedmetadata = () => resolve({ duration_secs: v.duration, width: v.videoWidth, height: v.videoHeight });
    v.onerror = () => reject(new Error("This browser can't read that video"));
    v.src = url;
  });
}

function download(blob: Blob, name: string) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

function emptyProject(name: string, width: number, height: number, fps: number): string {
  const tracks = [
    ["t1", "video", "Video 1"], ["t2", "audio", "Audio"], ["t3", "caption", "Captions"],
  ].map(([id, kind, n]) => ({ id, kind, name: n, muted: false, items: [] }));
  return JSON.stringify({ version: 1, name, canvas: { width, height, fps }, tracks, next_id: 3 }, null, 2);
}

const CAP_WEB = "Auto-captions run in the desktop app with a local whisper model. In the browser, add captions by hand or import an .srt/.vtt file.";

async function adopt(file: File): Promise<MediaRef> {
  if (file.size > webPlatform.caps.maxInputBytes) throw new Error(`${file.name} is over 500 MB`);
  const source = registerFile(file);
  const m = await probeInBrowser(files.get(source)!.url);
  return { source, name: file.name, meta: { ...m, has_audio: true } };
}

export const webPlatform: Platform = {
  kind: "web",
  caps: {
    urlImport: false, hardwareEncode: false, transcribe: "none", maxInputBytes: 500 * 1024 * 1024,
    exportPresets: ["vertical", "landscape", "square", "portrait", "gif"],
    exportNote: "Rendered in this tab with ffmpeg.wasm at up to 960p. Nothing is uploaded.",
  },
  newProject: async (name, w, h, fps) => emptyProject(name, w, h, fps),
  classifyUrl: async () => {
    throw new Error("Link import needs the desktop app (browsers block cross-site video fetches).");
  },
  async importMedia() {
    const out: MediaRef[] = [];
    for (const file of await pickFiles("video/*", true)) out.push(await adopt(file));
    return out;
  },
  importFile: (file) => adopt(file),
  sourceName: (source) => files.get(source)?.file.name ?? source,
  mediaUrl: (source) => files.get(source)?.url ?? "",
  async saveProject(json, name) {
    const fname = safeName(name, "msproj");
    download(new Blob([json], { type: "application/json" }), fname);
    return fname;
  },
  async openProject() {
    const [f] = await pickFiles(".msproj,.json", false);
    return f ? f.text() : null;
  },
  async exportProject(json, preset, onProgress) {
    const p = JSON.parse(json) as { name: string; canvas: { width: number; height: number }; tracks: { kind: string; items: { type: string; source: string; source_in_ms: number; duration_ms: number }[] }[] };
    if (preset === "gif") {
      const clip = p.tracks.find((t) => t.kind === "video")?.items.find((i) => i.type === "clip");
      const entry = clip && files.get(clip.source);
      if (!clip || !entry) throw new Error("Import a clip first (web projects can't reopen files from a past session).");
      const blob = (await webPlatform.toGif(entry.file, {
        startSec: clip.source_in_ms / 1000, durationSec: Math.min(15, clip.duration_ms / 1000), width: 480, fps: 15,
      }, onProgress)) as Blob;
      const name = safeName(`${p.name}-gif`, "gif");
      download(blob, name);
      return name;
    }
    const [w, h] = webSize(...PRESET_SIZES[preset]);
    const plan = planRender(p, w, h);
    const f = await loadFfmpeg();
    const data = await renderMp4(f, plan, (s) => files.get(s)?.file, onProgress);
    const name = safeName(`${p.name}-${preset}`, "mp4");
    download(new Blob([data.slice()], { type: "video/mp4" }), name);
    return name;
  },
  cancelExport: async () => undefined,
  filmstrip: async () => null,
  ai: {
    available: false,
    settings: async () => ({ providers: [], text_provider: null }),
    saveProvider: async () => { throw new Error(AI_WEB); },
    deleteProvider: async () => { throw new Error(AI_WEB); },
    testProvider: async () => { throw new Error(AI_WEB); },
    rewriteCaptions: async () => { throw new Error(AI_WEB); },
  },
  captions: {
    available: false,
    models: async () => [],
    downloadModel: async () => { throw new Error(CAP_WEB); },
    cancelDownload: async () => undefined,
    deleteModel: async () => { throw new Error(CAP_WEB); },
    autoCaption: async () => { throw new Error(CAP_WEB); },
    cancel: async () => undefined,
    diarizationStatus: async () => ({ models: false, engine: false }),
    downloadDiarization: async () => { throw new Error(CAP_WEB); },
    importModel: async () => { throw new Error(CAP_WEB); },
    importDiarization: async () => { throw new Error(CAP_WEB); },
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
