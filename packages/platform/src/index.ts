import { tauriPlatform } from "./tauri";
import { webPlatform } from "./web";
import type { Platform } from "./types";

export * from "./types";
export { loadFfmpeg, webPlatform } from "./web";

/** True inside the Tauri webview (Tauri 2 injects __TAURI_INTERNALS__). */
export function isTauri(w: unknown = globalThis): boolean {
  return typeof w === "object" && w !== null && "__TAURI_INTERNALS__" in w;
}

export function getPlatform(w: unknown = globalThis): Platform {
  return isTauri(w) ? tauriPlatform : webPlatform;
}
