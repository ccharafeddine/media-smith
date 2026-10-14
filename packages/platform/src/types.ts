/** One interface between the UI and whatever does the work: the Rust backend
 *  under Tauri, or ffmpeg.wasm in a plain browser tab (no server). */
export type PlatformKind = "desktop" | "web";

export interface Capabilities {
  urlImport: boolean; // yt-dlp + scraper: desktop only (CORS, no subprocesses)
  hardwareEncode: boolean;
  transcribe: "native" | "wasm" | "none";
  maxInputBytes: number; // wasm heap is ~2 GB; keep demos well under
}

export interface GifOptions {
  startSec: number;
  durationSec: number;
  width: number;
  fps: number;
}

export interface Platform {
  kind: PlatformKind;
  caps: Capabilities;
  newProject(name: string, width: number, height: number, fps: number): Promise<string>;
  classifyUrl(url: string): Promise<string>;
  /** Convert a video to GIF. Desktop takes a path, web takes a File/Blob. */
  toGif(input: File | string, opts: GifOptions, onProgress?: (ratio: number) => void): Promise<Blob | string>;
}

/** Palette-based GIF filter graph shared by both backends (same output). */
export function gifFilter(o: GifOptions): string {
  return `fps=${o.fps},scale=${o.width}:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle`;
}

export function gifArgs(input: string, output: string, o: GifOptions): string[] {
  return ["-ss", String(o.startSec), "-t", String(o.durationSec), "-i", input, "-filter_complex", gifFilter(o), "-loop", "0", output];
}
