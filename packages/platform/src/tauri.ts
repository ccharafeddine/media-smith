import type { AiView, CaptionModel, ExportPreset, MediaMeta, MediaRef, Platform, ProviderConfig } from "./types";
import { safeName } from "./types";

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import("@tauri-apps/api/core");
  return core.invoke<T>(cmd, args);
}

async function listen<T>(event: string, f: (e: { payload: T }) => void): Promise<() => void> {
  const ev = await import("@tauri-apps/api/event");
  return ev.listen<T>(event, f);
}

const VIDEO_EXT = ["mp4", "mov", "mkv", "webm", "avi", "m4v"];

let convert: ((path: string) => string) | null = null;
void import("@tauri-apps/api/core").then((c) => (convert = c.convertFileSrc)).catch(() => undefined);

export const tauriPlatform: Platform = {
  kind: "desktop",
  caps: {
    urlImport: true,
    hardwareEncode: true,
    transcribe: "native",
    maxInputBytes: Number.MAX_SAFE_INTEGER,
    exportPresets: ["vertical", "landscape", "square", "portrait", "gif"],
    exportNote: "Rendered on this computer with native ffmpeg at full size.",
  },
  newProject: (name, width, height, fps) => invoke<string>("new_project", { name, width, height, fps }),
  classifyUrl: (url) => invoke<string>("classify_url", { url }),
  async importMedia() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ multiple: true, filters: [{ name: "Video", extensions: VIDEO_EXT }] });
    const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    const out: MediaRef[] = [];
    for (const path of paths) {
      const meta = await invoke<MediaMeta>("probe_file", { path });
      out.push({ source: path, name: path.split(/[\\/]/).pop() ?? path, meta });
    }
    return out;
  },
  async importFile(file) {
    // Bundled sample or a dropped file: stash a copy on disk so the native
    // renderer can read it like any imported path.
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    const path = await invoke<string>("stash_media", { name: file.name, bytes });
    const meta = await invoke<MediaMeta>("probe_file", { path });
    return { source: path, name: file.name, meta };
  },
  sourceName: (source) => source.split(/[\\/]/).pop() ?? source,
  mediaUrl: (source) => (convert ? convert(source) : source),
  async saveProject(json, name) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: safeName(name, "msproj"), filters: [{ name: "MediaSmith project", extensions: ["msproj"] }] });
    if (!path) return null;
    await invoke("save_project", { path, json });
    return path;
  },
  async openProject() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ multiple: false, filters: [{ name: "MediaSmith project", extensions: ["msproj", "json"] }] });
    if (!path || Array.isArray(path)) return null;
    return invoke<string>("load_project", { path });
  },
  async exportProject(json, preset: ExportPreset, onProgress) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const ext = preset === "gif" ? "gif" : "mp4";
    const name = (JSON.parse(json) as { name?: string }).name ?? "export";
    const output = await save({ defaultPath: safeName(`${name}-${preset}`, ext), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!output) return null;
    const unlisten = await listen<{ ratio: number }>("export-progress", (e) => onProgress?.(e.payload.ratio));
    try {
      return await invoke<string>("export_project", { json, preset, output });
    } finally {
      unlisten();
    }
  },
  cancelExport: () => invoke<void>("cancel_export"),
  filmstrip: (path, sourceInMs, durationMs, count) =>
    invoke<string>("filmstrip", { path, sourceInMs, durationMs, count }),
  ai: {
    available: true,
    settings: () => invoke<AiView>("ai_settings"),
    saveProvider: (config: ProviderConfig, key, makeDefault) => invoke<AiView>("ai_save_provider", { config, key: key ?? null, makeDefault }),
    deleteProvider: (id) => invoke<AiView>("ai_delete_provider", { id }),
    testProvider: (id) => invoke<string[]>("ai_test_provider", { id }),
    rewriteCaptions: (lines, instruction) => invoke<string[]>("ai_rewrite_captions", { lines, instruction }),
  },
  captions: {
    available: true,
    models: () => invoke<CaptionModel[]>("caption_models"),
    async downloadModel(file, onProgress) {
      const unlisten = await listen<{ file: string; received: number; total: number | null }>("model-download-progress", (e) =>
        onProgress?.(e.payload.total ? e.payload.received / e.payload.total : null));
      try {
        await invoke<void>("download_caption_model", { file });
      } finally {
        unlisten();
      }
    },
    cancelDownload: () => invoke<void>("cancel_caption_model"),
    deleteModel: (file) => invoke<void>("delete_caption_model", { file }),
    async autoCaption(json, model, language, onProgress, speakers = false) {
      const unlisten = await listen<{ ratio: number }>("caption-progress", (e) => onProgress?.(e.payload.ratio));
      try {
        return await invoke<string>("auto_caption", { json, model, language, speakers });
      } finally {
        unlisten();
      }
    },
    cancel: () => invoke<void>("cancel_auto_caption"),
    async diarizationStatus() {
      const [models, engine] = await invoke<[boolean, boolean]>("diarization_status");
      return { models, engine };
    },
    async downloadDiarization(onProgress) {
      const unlisten = await listen<{ step: number; label: string; received: number; total: number | null }>("diarize-download-progress", (e) =>
        onProgress?.(e.payload.total ? (e.payload.step - 1 + e.payload.received / e.payload.total) / 2 : null, e.payload.label));
      try {
        await invoke<void>("download_diarization_models");
      } finally {
        unlisten();
      }
    },
    async importModel() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "Whisper model", extensions: ["bin"] }] });
      if (!path || Array.isArray(path)) return null;
      return invoke<string>("import_caption_model", { path });
    },
    async importDiarization() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "Speaker model", extensions: ["onnx", "bz2"] }] });
      if (!path || Array.isArray(path)) return null;
      return invoke<string>("import_diarization_model", { path });
    },
  },
  toGif: async () => {
    throw new Error("On desktop, use Export with the GIF preset.");
  },
};
