/** Static UI catalogues (rail tools, presets, AI providers). */
import type { ExportPreset } from "@mediasmith/platform";

export const TOOLS = ["Home", "Media", "Text", "Captions", "Design", "Settings"] as const;
export type Tool = (typeof TOOLS)[number];

export const TOOL_ICONS: Record<Tool, string> = { Home: "⌂", Media: "▤", Text: "T", Captions: "CC", Design: "✦", Settings: "⚙" };

export const PRESETS: { id: ExportPreset; label: string; short: string; w: number; h: number }[] = [
  { id: "vertical", label: "TikTok / Reels / Shorts", short: "9:16", w: 1080, h: 1920 },
  { id: "portrait", label: "Feed portrait", short: "4:5", w: 1080, h: 1350 },
  { id: "square", label: "Square", short: "1:1", w: 1080, h: 1080 },
  { id: "landscape", label: "YouTube", short: "16:9", w: 1920, h: 1080 },
  { id: "gif", label: "GIF", short: "GIF", w: 480, h: 480 },
];

/** Bring-your-own-key AI providers (optional; see docs/ai.md). */
export const AI_PROVIDERS = [
  { id: "openai", label: "OpenAI", hint: "sk-…", defaultModel: "gpt-image-1 / gpt-4.1-mini" },
  { id: "gemini", label: "Google Gemini", hint: "AIza…", defaultModel: "gemini-2.5-flash-image" },
  { id: "anthropic", label: "Anthropic", hint: "sk-ant-…", defaultModel: "claude (text only)" },
  { id: "compatible", label: "OpenAI-compatible / Ollama", hint: "http://localhost:11434/v1", defaultModel: "llama3.2-vision" },
] as const;
