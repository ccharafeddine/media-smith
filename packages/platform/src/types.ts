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
  /** Stop a running export (no-op if none). */
  cancelExport(): Promise<void>;
  /** Thumbnail strip for a clip range as an image URL, or null if unsupported. */
  filmstrip(source: string, sourceInMs: number, durationMs: number, count: number): Promise<string | null>;
  ai: AiApi;
  captions: CaptionsApi;
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

export type ProviderKind = "openai" | "gemini" | "anthropic" | "compatible";

export interface ProviderConfig { id: string; kind: ProviderKind; label: string; base_url: string; model: string }
export interface ProviderView extends ProviderConfig { has_key: boolean }
export interface AiView { providers: ProviderView[]; text_provider: string | null }

export const PROVIDER_DEFAULTS: Record<ProviderKind, { label: string; base_url: string; model: string; needsKey: boolean }> = {
  openai: { label: "OpenAI", base_url: "https://api.openai.com/v1", model: "gpt-4.1-mini", needsKey: true },
  gemini: { label: "Google Gemini", base_url: "https://generativelanguage.googleapis.com/v1beta/openai", model: "gemini-2.5-flash", needsKey: true },
  anthropic: { label: "Anthropic", base_url: "https://api.anthropic.com/v1", model: "claude-sonnet-4-5", needsKey: true },
  compatible: { label: "Ollama / compatible", base_url: "http://localhost:11434/v1", model: "llama3.2", needsKey: false },
};

/** Bring-your-own-key AI. Keys go to the OS keychain on desktop. */
export interface AiApi {
  available: boolean;
  settings(): Promise<AiView>;
  /** key: undefined keeps the stored key, "" deletes it. */
  saveProvider(config: ProviderConfig, key: string | undefined, makeDefault: boolean): Promise<AiView>;
  deleteProvider(id: string): Promise<AiView>;
  testProvider(id: string): Promise<string[]>;
  rewriteCaptions(lines: string[], instruction: string): Promise<string[]>;
}

/** A whisper model the manager knows about (allowlisted, SHA-256 checked). */
export interface CaptionModel { file: string; label: string; size_mb: number; multilingual: boolean; installed: boolean; is_default: boolean }

/** Speech-to-text auto-captions (whisper-cli sidecar on desktop). */
export interface CaptionsApi {
  available: boolean;
  models(): Promise<CaptionModel[]>;
  /** Download + verify a model; progress is 0..1 when the size is known. */
  downloadModel(file: string, onProgress?: (ratio: number | null) => void): Promise<void>;
  cancelDownload(): Promise<void>;
  deleteModel(file: string): Promise<void>;
  /** Transcribe the main track and return the project JSON with its caption track replaced. */
  autoCaption(json: string, model: string | null, language: string | null, onProgress?: (ratio: number) => void, speakers?: boolean): Promise<string>;
  cancel(): Promise<void>;
  /** Speaker detection: are the models downloaded and the engine bundled? */
  diarizationStatus(): Promise<{ models: boolean; engine: boolean }>;
  /** Download + verify the two speaker-detection models (~46 MB). */
  downloadDiarization(onProgress?: (ratio: number | null, label: string) => void): Promise<void>;
}

/** "DIARIZE_MODELS_MISSING|<dir>" means the speaker models need downloading. */
export function missingDiarization(err: unknown): boolean {
  return String(err instanceof Error ? err.message : err).startsWith("DIARIZE_MODELS_MISSING|");
}

/** "MODEL_NOT_FOUND|<file>" from the shell means the model needs downloading. */
export function missingModel(err: unknown): string | null {
  const m = /MODEL_NOT_FOUND\|(\S+)/.exec(String(err instanceof Error ? err.message : err));
  return m ? m[1] : null;
}
