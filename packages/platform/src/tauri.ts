import type { Platform } from "./types";

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import("@tauri-apps/api/core");
  return core.invoke<T>(cmd, args);
}

export const tauriPlatform: Platform = {
  kind: "desktop",
  caps: { urlImport: true, hardwareEncode: true, transcribe: "native", maxInputBytes: Number.MAX_SAFE_INTEGER },
  newProject: (name, width, height, fps) => invoke<string>("new_project", { name, width, height, fps }),
  classifyUrl: (url) => invoke<string>("classify_url", { url }),
  toGif: async () => {
    throw new Error("Desktop GIF export lands in phase 2 (port of GifSmith's encoder).");
  },
};
