/** Offline caption import: SubRip (.srt) and WebVTT (.vtt) into plain cues.
 *  Works the same in the browser and the desktop app; no model needed. */
export interface Cue { start_ms: number; end_ms: number; text: string }

const TIME = /(\d+):(\d{2}):(\d{2})[,.](\d{1,3})|(\d{2}):(\d{2})[,.](\d{1,3})/;

export function parseTime(s: string): number | null {
  const m = TIME.exec(s.trim());
  if (!m) return null;
  if (m[1] !== undefined) return ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + +m[4].padEnd(3, "0");
  return (+m[5] * 60 + +m[6]) * 1000 + +m[7].padEnd(3, "0");
}

export function parseSubtitles(src: string): Cue[] {
  const out: Cue[] = [];
  const blocks = src.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n").split(/\n{2,}/);
  for (const b of blocks) {
    const lines = b.split("\n").filter((l) => l.trim() !== "");
    const at = lines.findIndex((l) => l.includes("-->"));
    if (at < 0) continue;
    const [a, z] = lines[at].split("-->");
    const start = parseTime(a), end = parseTime(z.trim().split(/\s+/)[0]);
    if (start === null || end === null || end <= start) continue;
    const text = lines.slice(at + 1).join("\n").replace(/<[^>]+>/g, "").trim();
    if (text) out.push({ start_ms: start, end_ms: end, text });
  }
  out.sort((x, y) => x.start_ms - y.start_ms);
  // Captions can't overlap on one track: clip each cue at the next one's start.
  for (let i = 0; i + 1 < out.length; i++) out[i].end_ms = Math.min(out[i].end_ms, out[i + 1].start_ms);
  return out.filter((c) => c.end_ms > c.start_ms);
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");
function stamp(ms: number, sep: string): string {
  const h = Math.floor(ms / 3600000), m = Math.floor(ms / 60000) % 60, s = Math.floor(ms / 1000) % 60;
  return `${pad(h)}:${pad(m)}:${pad(s)}${sep}${pad(ms % 1000, 3)}`;
}

export function toSrt(cues: Cue[]): string {
  return cues.map((c, i) => `${i + 1}\n${stamp(c.start_ms, ",")} --> ${stamp(c.end_ms, ",")}\n${c.text}\n`).join("\n");
}

export function toVtt(cues: Cue[]): string {
  return "WEBVTT\n\n" + cues.map((c) => `${stamp(c.start_ms, ".")} --> ${stamp(c.end_ms, ".")}\n${c.text}\n`).join("\n");
}
