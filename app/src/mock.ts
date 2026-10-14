/** Static UI catalogues (tools, presets, AI providers). The timeline itself
 *  is real as of phase 2; see store/. */
import type { ExportPreset } from "@mediasmith/platform";

export const TOOLS = ["Edit", "Clip", "GIF", "Captions", "Design", "Import", "Settings"] as const;
export type Tool = (typeof TOOLS)[number];

export const PRESETS: { id: ExportPreset; label: string; w: number; h: number }[] = [
  { id: "vertical", label: "9:16 · TikTok / Reels / Shorts", w: 1080, h: 1920 },
  { id: "landscape", label: "16:9 · YouTube", w: 1920, h: 1080 },
  { id: "square", label: "1:1 · Square", w: 1080, h: 1080 },
  { id: "gif", label: "GIF", w: 480, h: 480 },
];

/** Bring-your-own-key AI providers (phase 4; see docs/ai.md). */
export const AI_PROVIDERS = [
  { id: "openai", label: "OpenAI", hint: "sk-…", defaultModel: "gpt-image-1 / gpt-4.1-mini" },
  { id: "gemini", label: "Google Gemini", hint: "AIza…", defaultModel: "gemini-2.5-flash-image" },
  { id: "anthropic", label: "Anthropic", hint: "sk-ant-…", defaultModel: "claude (text only)" },
  { id: "compatible", label: "OpenAI-compatible / Ollama", hint: "http://localhost:11434/v1", defaultModel: "llama3.2-vision" },
] as const;
