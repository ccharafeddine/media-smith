/** TypeScript port of crates/ms-media/src/ass.rs, so the browser burns in the
 *  same text and captions as the desktop export (libass in ffmpeg.wasm). */

export interface AssStyle {
  font: string; size_px: number; color: string; stroke?: string | null; highlight?: string | null;
  x: number; y: number; anim?: "none" | "fade" | "pop" | "slide-up";
}
interface Word { text: string; start_ms: number; end_ms: number }
interface AssItem { type: string; start_ms: number; duration_ms: number; text?: string; style?: AssStyle; words?: Word[] | null; fade?: { in_ms: number; out_ms: number } | null }
interface AssProject { canvas: { width: number; height: number }; tracks: { kind: string; muted?: boolean; items: AssItem[] }[] }

export function assColor(hex: string): string {
  const h = hex.trim().replace(/^#/, "");
  if (!/^[0-9a-fA-F]{6}$/.test(h)) return "&H00FFFFFF";
  return `&H00${h.slice(4, 6)}${h.slice(2, 4)}${h.slice(0, 2)}`.toUpperCase();
}

export function assTime(ms: number): string {
  const cs = Math.floor(Math.max(0, ms) / 10);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${Math.floor(cs / 360000)}:${p(Math.floor(cs / 6000) % 60)}:${p(Math.floor(cs / 100) % 60)}.${p(cs % 100)}`;
}

export function assEscape(t: string): string {
  return t.replace(/\\/g, "\u29F5").replace(/{/g, "(").replace(/}/g, ")").replace(/\r\n/g, "\n").replace(/\n/g, "\\N");
}

function overrides(s: AssStyle, scale: number, w: number, h: number, karaoke: boolean): string {
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  const x = Math.round(clamp(s.x) * w), y = Math.round(clamp(s.y) * h);
  const fs = Math.max(1, Math.round(s.size_px * scale));
  const font = s.font.replace(/[{}\\]/g, "");
  let pos = `\\pos(${x},${y})`;
  let o = karaoke && s.highlight
    ? `\\an5${pos}\\fn${font}\\fs${fs}\\1c${assColor(s.highlight)}\\2c${assColor(s.color)}`
    : `\\an5${pos}\\fn${font}\\fs${fs}\\1c${assColor(s.color)}`;
  o += s.stroke ? `\\3c${assColor(s.stroke)}\\bord${Math.max(1, Math.round(fs / 16))}` : "\\bord0";
  o += "\\shad0";
  switch (s.anim ?? "none") {
    case "fade": o += "\\fad(250,250)"; break;
    case "pop": o += "\\fscx70\\fscy70\\t(0,180,\\fscx100\\fscy100)"; break;
    case "slide-up": {
      const dy = Math.round(h * 0.04);
      o = o.replace(pos, `\\move(${x},${y + dy},${x},${y},0,250)`);
      pos = "";
      o += "\\fad(200,0)";
      break;
    }
  }
  return `{${o}}`;
}

export function karaokeText(words: Word[]): string {
  const cs = (ms: number) => Math.max(1, Math.ceil(ms / 10));
  let out = "";
  if (words[0] && words[0].start_ms >= 10) out += `{\\k${Math.floor(words[0].start_ms / 10)}}`;
  words.forEach((w, i) => {
    const until = words[i + 1] ? Math.max(words[i + 1].start_ms, w.start_ms) : w.end_ms;
    out += `${i > 0 ? " " : ""}{\\k${cs(Math.max(0, until - w.start_ms))}}${assEscape(w.text)}`;
  });
  return out;
}

export function buildAss(p: AssProject, w: number, h: number): string {
  const scale = h / Math.max(1, p.canvas.height);
  let out = `[Script Info]\nScriptType: v4.00+\nPlayResX: ${w}\nPlayResY: ${h}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n`
    + "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
    + "Style: Default,Inter,48,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,0,5,20,20,20,1\n\n"
    + "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";
  ([[0, "text"], [1, "caption"]] as const).forEach(([layer, kind]) => {
    for (const t of p.tracks.filter((t) => t.kind === kind && !t.muted)) {
      for (const it of t.items) {
        if ((it.type !== "text" && it.type !== "caption") || !it.style) continue;
        const words = it.type === "caption" ? it.words ?? [] : [];
        const karaoke = !!it.style.highlight && words.length > 0;
        const body = karaoke ? karaokeText(words) : assEscape(it.text ?? "");
        const fin = it.fade?.in_ms ?? 0, fout = it.fade?.out_ms ?? 0;
        const fade = fin || fout ? `{\\fad(${fin},${fout})}` : "";
        out += `Dialogue: ${layer},${assTime(it.start_ms)},${assTime(it.start_ms + it.duration_ms)},Default,,0,0,0,,${overrides(it.style, scale, w, h, karaoke)}${fade}${body}\n`;
      }
    }
  });
  return out;
}

export function hasOverlays(p: AssProject): boolean {
  return p.tracks.some((t) => (t.kind === "text" || t.kind === "caption") && !t.muted && t.items.length > 0);
}
