import { tauriPlatform } from "./tauri";
import { webPlatform } from "./web";
import type { Platform } from "./types";

export * from "./types";
export { registerFile, webPlatform } from "./web";
export { configureFfmpeg, loadFfmpeg, partUrls, type CoreManifest } from "./ffmpegLoader";
export { assColor, assEscape, assTime, buildAss, hasOverlays, karaokeText } from "./ass";
export { clipArgs, gapArgs, planRender, renderMp4, silentClipArgs, webSize, WEB_MAX_H } from "./webRender";

/** True inside the Tauri webview (Tauri 2 injects __TAURI_INTERNALS__). */
export function isTauri(w: unknown = globalThis): boolean {
  return typeof w === "object" && w !== null && "__TAURI_INTERNALS__" in w;
}

export function getPlatform(w: unknown = globalThis): Platform {
  return isTauri(w) ? tauriPlatform : webPlatform;
}
