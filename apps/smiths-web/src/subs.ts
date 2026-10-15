/** Subtitle cues for the CaptionSmith web demo: SRT/VTT in and out, plus an
 *  ASS script for burning the styled captions in with ffmpeg.wasm (libass). */
export interface Cue { start: number; end: number; text: string }
export interface CapStyle { font: "Inter" | "Syne"; size: number; color: string; box: boolean; position: "bottom" | "middle" | "top" }

export const DEFAULT_STYLE: CapStyle = { font: "Inter", size: 6, color: "#ffffff", box: true, position: "bottom" };

const TS = /(?:(\d+):)?(\d{1,2}):(\d{2})[.,](\d{1,3})/;

function parseTs(s: string): number {
  const m = TS.exec(s);
  if (!m) return NaN;
  return (Number(m[1] ?? 0) * 3600) + Number(m[2]) * 60 + Number(m[3]) + Number(m[4].padEnd(3, "0")) / 1000;
}

/** Parse SRT or WebVTT text into cues sorted by start time. */
export function parseSubs(src: string): Cue[] {
  const blocks = src.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n").split(/\n{2,}/);
  const out: Cue[] = [];
  for (const b of blocks) {
    const lines = b.split("\n").filter((l) => l.trim() !== "");
    const i = lines.findIndex((l) => l.includes("-->"));
    if (i < 0) continue;
    const [a, z] = lines[i].split("-->");
    const start = parseTs(a), end = parseTs(z);
    const text = lines.slice(i + 1).join("\n").replace(/<[^>]+>/g, "").trim();
    if (Number.isFinite(start) && Number.isFinite(end) && end > start && text) out.push({ start, end, text });
  }
  return out.sort((x, y) => x.start - y.start);
}

function stamp(t: number, sep: "," | "."): string {
  const ms = Math.max(0, Math.round(t * 1000));
  const h = Math.floor(ms / 3600000), m = Math.floor(ms / 60000) % 60, s = Math.floor(ms / 1000) % 60;
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(h)}:${p(m)}:${p(s)}${sep}${p(ms % 1000, 3)}`;
}

export const toSrt = (cues: Cue[]) => cues.map((c, i) => `${i + 1}\n${stamp(c.start, ",")} --> ${stamp(c.end, ",")}\n${c.text}\n`).join("\n");
export const toVtt = (cues: Cue[]) => "WEBVTT\n\n" + cues.map((c) => `${stamp(c.start, ".")} --> ${stamp(c.end, ".")}\n${c.text}\n`).join("\n");

/** "#rrggbb" to ASS "&HAABBGGRR". */
export function assColor(hex: string, alpha = 0): string {
  const h = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => h.slice(i, i + 2).toUpperCase());
  return `&H${alpha.toString(16).padStart(2, "0").toUpperCase()}${b}${g}${r}`;
}

function assTime(t: number): string {
  const cs = Math.max(0, Math.round(t * 100));
  const h = Math.floor(cs / 360000), m = Math.floor(cs / 6000) % 60, s = Math.floor(cs / 100) % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(cs % 100).padStart(2, "0")}`;
}

const esc = (t: string) => t.replace(/\\/g, "\\\\").replace(/[{}]/g, (c) => "\\" + c).replace(/\n/g, "\\N");

/** ASS script at the video's own resolution; `size` is % of frame height. */
export function toAss(cues: Cue[], st: CapStyle, w: number, h: number): string {
  const fs = Math.round((h * st.size) / 100);
  const align = st.position === "top" ? 8 : st.position === "middle" ? 5 : 2;
  const margin = Math.round(h * 0.07);
  const border = st.box ? 3 : 1;
  const style = `Style: Cap,${st.font},${fs},${assColor(st.color)},${assColor(st.color)},${assColor("#000000")},${assColor("#000000", st.box ? 0x60 : 0)},-1,0,0,0,100,100,0,0,${border},${st.box ? Math.round(fs * 0.25) : Math.max(2, Math.round(fs * 0.06))},0,${align},${Math.round(w * 0.06)},${Math.round(w * 0.06)},${margin},1`;
  return [
    "[Script Info]", "ScriptType: v4.00+", `PlayResX: ${w}`, `PlayResY: ${h}`, "WrapStyle: 0", "ScaledBorderAndShadow: yes", "",
    "[V4+ Styles]",
    "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding",
    style, "",
    "[Events]", "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text",
    ...cues.map((c) => `Dialogue: 0,${assTime(c.start)},${assTime(c.end)},Cap,,0,0,0,,${esc(c.text)}`),
    "",
  ].join("\n");
}

/** Centre crop to `ratio` (w/h) with even dimensions; "" for the original frame. */
export function cropFilter(ratio: number | null): string {
  if (!ratio) return "";
  const r = ratio.toFixed(6);
  return `crop='trunc(if(gt(iw/ih,${r}),ih*${r},iw)/2)*2':'trunc(if(gt(iw/ih,${r}),ih,iw/${r})/2)*2'`;
}
