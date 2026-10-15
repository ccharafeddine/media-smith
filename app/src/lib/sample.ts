import type { Project } from "../store/model";
import { FORMAT_VERSION } from "../store/model";

/** The bundled demo project: an original 8 s sunset clip (made for this repo)
 *  with a title and karaoke captions, so the editor is never blank when
 *  someone lands from a portfolio link. Fully offline. */
export const SAMPLE_NAME = "Sunset skate edit";

const LINES: [number, number, string][] = [
  [400, 2100, "wait for it"],
  [2300, 4600, "this is the part"],
  [4800, 7600, "golden hour hits different"],
];

function words(text: string, dur: number) {
  const ws = text.split(" ");
  const step = Math.floor(dur / ws.length);
  return ws.map((w, i) => ({ start_ms: i * step, end_ms: (i + 1) * step - 40, text: w }));
}

export function sampleProject(source: string, durationMs: number): Project {
  const dur = Math.max(1000, Math.round(durationMs));
  const cap = { font: "Syne", size_px: 84, color: "#ffffff", stroke: "#000000", x: 0.5, y: 0.74, highlight: "#a974ff", anim: "pop" as const };
  return {
    version: FORMAT_VERSION,
    name: SAMPLE_NAME,
    canvas: { width: 1080, height: 1920, fps: 30 },
    next_id: 20,
    tracks: [
      { id: "t1", kind: "video", name: "Video", muted: false, items: [
        { id: "i1", start_ms: 0, duration_ms: dur, type: "clip", source, source_in_ms: 0, volume: 1, fade: { in_ms: 300, out_ms: 500 } },
      ] },
      { id: "t2", kind: "audio", name: "Music", muted: false, items: [] },
      { id: "t3", kind: "text", name: "Text", muted: false, items: [
        { id: "i2", start_ms: 0, duration_ms: Math.min(dur, 3000), type: "text", text: "GOLDEN HOUR",
          style: { font: "Syne", size_px: 120, color: "#ffffff", stroke: "#1a1a1a", x: 0.5, y: 0.14, anim: "slide-up" } },
      ] },
      { id: "t4", kind: "caption", name: "Captions", muted: false, items: LINES.filter(([s]) => s < dur).map(([s, e, t], k) => ({
        id: `i${3 + k}`, start_ms: s, duration_ms: Math.min(e, dur) - s, type: "caption" as const, text: t, style: cap, words: words(t, Math.min(e, dur) - s),
      })) },
    ],
  };
}
