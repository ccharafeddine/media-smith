/** One interface between the UI and whatever does the work: the Rust backend
 *  under Tauri, or the browser (ffmpeg.wasm, File APIs) with no server. */
export type PlatformKind = "desktop" | "web";

export interface Capabilities {
  urlImport: boolean; // yt-dlp + scraper: desktop only (CORS, no subprocesses)
  hardwareEncode: boolean;
  transcribe: "native" | "wasm" | "none";
  maxInputBytes: number; // wasm heap is ~2 GB; keep demos well under
  /** Export presets this platform can render. */
  exportPresets: ExportPreset[];
}

export type ExportPreset = "vertical" | "landscape" | "square" | "gif";

export const PRESET_SIZES: Record<ExportPreset, [number, number]> = {
  vertical: [1080, 1920],
  landscape: [1920, 1080],
  square: [1080, 1080],
  gif: [480, 480],
};

export interface MediaMeta {
  duration_secs: number;
  width: number;
  height: number;
  has_audio: boolean;
}

/** An imported file. `source` is what the project stores: an absolute path on
 *  desktop, an opaque `web:<n>` handle in the browser. */
export interface MediaRef {
  source: string;
  name: string;
  meta: MediaMeta;
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
  /** Let the user pick media files and probe them. Empty array = cancelled. */
  importMedia(): Promise<MediaRef[]>;
  /** A URL the <video> element can play for a stored source. */
  mediaUrl(source: string): string;
  /** Save project JSON. Returns where it went, or null if cancelled. */
  saveProject(json: string, name: string): Promise<string | null>;
  /** Open a project; returns its JSON, or null if cancelled. */
  openProject(): Promise<string | null>;
  /** Render the main video track. Returns the output path/name, or null if cancelled. */
  exportProject(json: string, preset: ExportPreset, onProgress?: (ratio: number) => void): Promise<string | null>;
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

export function safeName(name: string, ext: string): string {
  const base = name.trim().replace(/[\\/:*?"<>|]+/g, "_").replace(/\s+/g, " ") || "Untitled";
  return `${base}.${ext}`;
}
