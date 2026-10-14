/** Phase-1 mock timeline (mirrors the ms-project JSON shape). */
export type Kind = "video" | "audio" | "text" | "caption";
export interface MockItem { id: string; start: number; dur: number; label: string }
export interface MockTrack { id: string; kind: Kind; name: string; items: MockItem[] }

export const TOOLS = ["Edit", "Clip", "GIF", "Captions", "Design", "Import"] as const;
export type Tool = (typeof TOOLS)[number];

export const PRESETS = [
  { id: "tiktok", label: "TikTok", w: 1080, h: 1920 },
  { id: "reels", label: "Reels", w: 1080, h: 1920 },
  { id: "shorts", label: "Shorts", w: 1080, h: 1920 },
  { id: "youtube", label: "YouTube", w: 1920, h: 1080 },
  { id: "gif", label: "GIF", w: 480, h: 480 },
];

export const TRACKS: MockTrack[] = [
  { id: "t4", kind: "caption", name: "Captions", items: [
    { id: "c1", start: 400, dur: 1800, label: "wait for it…" },
    { id: "c2", start: 2400, dur: 2100, label: "this is the part" },
    { id: "c3", start: 4700, dur: 2600, label: "nobody expected 😳" },
    { id: "c4", start: 9000, dur: 2400, label: "follow for part 2" },
  ] },
  { id: "t3", kind: "text", name: "Text", items: [{ id: "x1", start: 0, dur: 3000, label: "TITLE · Bold Pop" }] },
  { id: "t1", kind: "video", name: "Video 1", items: [
    { id: "v1", start: 0, dur: 5200, label: "skate_intro.mp4" },
    { id: "v2", start: 5200, dur: 6800, label: "trick_slowmo.mov" },
  ] },
  { id: "t2", kind: "video", name: "Overlay", items: [{ id: "v3", start: 6000, dur: 3000, label: "logo.png" }] },
  { id: "t5", kind: "audio", name: "Audio", items: [{ id: "a1", start: 0, dur: 12000, label: "lofi_beat.mp3" }] },
];

export const DURATION = 12000;
