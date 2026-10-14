import type { ExportPreset, MediaMeta, MediaRef, Platform } from "./types";
import { safeName } from "./types";

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import("@tauri-apps/api/core");
  return core.invoke<T>(cmd, args);
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
    exportPresets: ["vertical", "landscape", "square", "gif"],
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
  async exportProject(json, preset: ExportPreset) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const ext = preset === "gif" ? "gif" : "mp4";
    const name = (JSON.parse(json) as { name?: string }).name ?? "export";
    const output = await save({ defaultPath: safeName(`${name}-${preset}`, ext), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!output) return null;
    return invoke<string>("export_project", { json, preset, output });
  },
  toGif: async () => {
    throw new Error("On desktop, use Export with the GIF preset.");
  },
};
