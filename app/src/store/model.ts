/** TypeScript mirror of the ms-project JSON schema plus the same edit rules
 *  (no overlaps, no empty items, left-trim shifts the source offset). The
 *  desktop shell re-validates with the Rust model on save and export. */

export type TrackKind = "video" | "audio" | "text" | "caption";

export type TextAnim = "none" | "fade" | "pop" | "slide-up";
export interface TextStyle { font: string; size_px: number; color: string; stroke: string | null; x: number; y: number; highlight?: string | null; anim?: TextAnim }

/** Edge fades (mirrors ms-project Fade). */
export interface Fade { in_ms: number; out_ms: number }

/** A spoken word, relative to its caption item's start (mirrors ms-project CaptionWord). */
export interface CaptionWord { start_ms: number; end_ms: number; text: string }

export type ItemKind =
  | { type: "clip"; source: string; source_in_ms: number; volume: number }
  | { type: "text"; text: string; style: TextStyle }
  | { type: "caption"; text: string; style: TextStyle; words?: CaptionWord[]; speaker?: number | null };

/** Placement of a clip on an overlay video track (mirrors ms-project Transform). */
export interface Transform { x: number; y: number; scale: number; opacity: number }

export type Item = { id: string; start_ms: number; duration_ms: number; transform?: Transform; fade?: Fade } & ItemKind;

export interface Track { id: string; kind: TrackKind; name: string; muted: boolean; items: Item[] }

export interface Project {
  version: number;
  name: string;
  canvas: { width: number; height: number; fps: number };
  tracks: Track[];
  next_id: number;
}

export const FORMAT_VERSION = 1;

export class EditError extends Error {}

export const defaultStyle = (): TextStyle => ({ font: "Syne", size_px: 64, color: "#ffffff", stroke: "#000000", x: 0.5, y: 0.82 });

export function newProject(name = "Untitled", width = 1080, height = 1920, fps = 30): Project {
  const p: Project = { version: FORMAT_VERSION, name, canvas: { width, height, fps }, tracks: [], next_id: 0 };
  addTrack(p, "video", "Video 1");
  addTrack(p, "audio", "Audio");
  addTrack(p, "caption", "Captions");
  return p;
}

const clone = (p: Project): Project => structuredClone(p);
const freshId = (p: Project, prefix: string) => `${prefix}${++p.next_id}`;
export const endMs = (i: Item) => i.start_ms + i.duration_ms;

export function addTrack(p: Project, kind: TrackKind, name: string): string {
  const id = freshId(p, "t");
  p.tracks.push({ id, kind, name, muted: false, items: [] });
  return id;
}

export function validate(p: Project): void {
  if (p.version > FORMAT_VERSION) throw new EditError(`Unsupported project version ${p.version}`);
  for (const t of p.tracks) {
    const items = [...t.items].sort((a, b) => a.start_ms - b.start_ms);
    for (const i of items) if (i.duration_ms <= 0) throw new EditError(`Item ${i.id} has no length`);
    for (let k = 1; k < items.length; k++)
      if (items[k].start_ms < endMs(items[k - 1])) throw new EditError(`Item ${items[k].id} overlaps on ${t.name}`);
  }
}

/** Apply an edit to a copy; return the copy only if it validates. */
function transact(p: Project, f: (q: Project) => void): Project {
  const q = clone(p);
  f(q);
  for (const t of q.tracks) t.items.sort((a, b) => a.start_ms - b.start_ms);
  validate(q);
  return q;
}

export function locate(p: Project, id: string): { ti: number; ii: number } {
  for (let ti = 0; ti < p.tracks.length; ti++) {
    const ii = p.tracks[ti].items.findIndex((i) => i.id === id);
    if (ii >= 0) return { ti, ii };
  }
  throw new EditError(`No item ${id}`);
}

export function findItem(p: Project, id: string): Item | undefined {
  for (const t of p.tracks) for (const i of t.items) if (i.id === id) return i;
  return undefined;
}

export function addItem(p: Project, trackId: string, start_ms: number, duration_ms: number, kind: ItemKind): { project: Project; id: string } {
  let id = "";
  const project = transact(p, (q) => {
    const t = q.tracks.find((x) => x.id === trackId);
    if (!t) throw new EditError(`No track ${trackId}`);
    id = freshId(q, "i");
    t.items.push({ id, start_ms: Math.round(start_ms), duration_ms: Math.round(duration_ms), ...kind } as Item);
  });
  return { project, id };
}

/** Append a clip at the end of a track (where imported media lands). */
export function appendClip(p: Project, trackId: string, source: string, duration_ms: number) {
  const t = p.tracks.find((x) => x.id === trackId);
  if (!t) throw new EditError(`No track ${trackId}`);
  const start = t.items.reduce((m, i) => Math.max(m, endMs(i)), 0);
  return addItem(p, trackId, start, duration_ms, { type: "clip", source, source_in_ms: 0, volume: 1 });
}

export function moveItem(p: Project, id: string, start_ms: number, toTrack?: string): Project {
  const { ti, ii } = locate(p, id);
  const dest = toTrack ? p.tracks.findIndex((t) => t.id === toTrack) : ti;
  if (dest < 0) throw new EditError(`No track ${toTrack}`);
  if (p.tracks[dest].kind !== p.tracks[ti].kind) throw new EditError("Can't move onto a different kind of track");
  return transact(p, (q) => {
    const [it] = q.tracks[ti].items.splice(ii, 1);
    it.start_ms = Math.max(0, Math.round(start_ms));
    q.tracks[dest].items.push(it);
  });
}

export function trimItem(p: Project, id: string, start_ms: number, duration_ms: number): Project {
  const { ti, ii } = locate(p, id);
  return transact(p, (q) => {
    const it = q.tracks[ti].items[ii];
    start_ms = Math.round(start_ms);
    if (it.type === "clip") {
      const shifted = it.source_in_ms + (start_ms - it.start_ms);
      if (shifted < 0) throw new EditError("Can't trim before the start of the source");
      it.source_in_ms = shifted;
    }
    it.start_ms = start_ms;
    it.duration_ms = Math.round(duration_ms);
  });
}

export function splitItem(p: Project, id: string, at_ms: number): { project: Project; id: string } {
  const { ti, ii } = locate(p, id);
  const src = p.tracks[ti].items[ii];
  if (at_ms <= src.start_ms || at_ms >= endMs(src)) throw new EditError("Split point is outside the item");
  let newId = "";
  const project = transact(p, (q) => {
    const it = q.tracks[ti].items[ii];
    const left = Math.round(at_ms) - it.start_ms;
    newId = freshId(q, "i");
    const right = { ...structuredClone(it), id: newId, start_ms: it.start_ms + left, duration_ms: it.duration_ms - left } as Item;
    if (right.type === "clip") right.source_in_ms += left;
    // Karaoke words go with the half they're spoken in (same rule as Rust).
    if (right.type === "caption" && right.words) {
      right.words = right.words.filter((w) => w.end_ms > left).map((w) => ({ ...w, start_ms: Math.max(0, w.start_ms - left), end_ms: w.end_ms - left }));
    }
    if (it.type === "caption" && it.words) {
      it.words = it.words.filter((w) => w.start_ms < left).map((w) => ({ ...w, end_ms: Math.min(w.end_ms, left) }));
    }
    // A cut is a hard edit: the fade-in stays left, the fade-out goes right.
    if (it.fade) {
      const f = it.fade;
      if (f.in_ms > 0) it.fade = { in_ms: f.in_ms, out_ms: 0 }; else delete it.fade;
      if (f.out_ms > 0) right.fade = { in_ms: 0, out_ms: f.out_ms }; else delete right.fade;
    }
    it.duration_ms = left;
    q.tracks[ti].items.splice(ii + 1, 0, right);
  });
  return { project, id: newId };
}

export function deleteItem(p: Project, id: string): Project {
  const { ti, ii } = locate(p, id);
  return transact(p, (q) => void q.tracks[ti].items.splice(ii, 1));
}

export function duration(p: Project): number {
  return p.tracks.flatMap((t) => t.items).reduce((m, i) => Math.max(m, endMs(i)), 0);
}

/** The clip on the first video track under time `t`, with its source time. */
export function clipAt(p: Project, t: number): { item: Item & { type: "clip" }; sourceMs: number } | null {
  const track = p.tracks.find((x) => x.kind === "video");
  const it = track?.items.find((i) => i.type === "clip" && t >= i.start_ms && t < endMs(i));
  if (!it || it.type !== "clip") return null;
  return { item: it, sourceMs: it.source_in_ms + (t - it.start_ms) };
}

export function toJson(p: Project): string {
  return JSON.stringify(p, null, 2);
}

export function fromJson(s: string): Project {
  let p: Project;
  try {
    p = JSON.parse(s) as Project;
  } catch {
    throw new EditError("That file isn't a valid MediaSmith project");
  }
  if (!p || typeof p !== "object" || !Array.isArray(p.tracks) || !p.canvas) throw new EditError("That file isn't a valid MediaSmith project");
  p.next_id ??= 0;
  validate(p);
  return p;
}

/** Replace the text of caption items by id (timing untouched). */
export function setTexts(p: Project, texts: Record<string, string>): Project {
  return transact(p, (q) => {
    for (const t of q.tracks) for (const i of t.items) if (i.id in texts && (i.type === "caption" || i.type === "text")) i.text = texts[i.id];
  });
}

/** Caption items in time order. */
export function captions(p: Project): (Item & { type: "caption" })[] {
  return p.tracks
    .filter((t) => t.kind === "caption")
    .flatMap((t) => t.items)
    .filter((i): i is Item & { type: "caption" } => i.type === "caption")
    .sort((a, b) => a.start_ms - b.start_ms);
}

// ---- Inline text/caption editing and overlays (phase 4) ----

export const DEFAULT_TRANSFORM: Transform = { x: 0.78, y: 0.78, scale: 0.35, opacity: 1 };

const clamp = (v: number, lo: number, hi: number) => (Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : lo);

export function clampTransform(t: Transform): Transform {
  return { x: clamp(t.x, 0, 1), y: clamp(t.y, 0, 1), scale: clamp(t.scale, 0.05, 1), opacity: clamp(t.opacity, 0, 1) };
}

/** One-click looks for captions and titles (font, size, colours). */
export const STYLE_PRESETS: { id: string; label: string; style: Partial<TextStyle> }[] = [
  { id: "classic", label: "Classic", style: { font: "Syne", size_px: 64, color: "#ffffff", stroke: "#000000" } },
  { id: "bold", label: "Bold yellow", style: { font: "Syne", size_px: 84, color: "#ffd400", stroke: "#000000" } },
  { id: "clean", label: "Clean", style: { font: "Syne", size_px: 56, color: "#ffffff", stroke: null } },
  { id: "title", label: "Title", style: { font: "Syne", size_px: 110, color: "#ffffff", stroke: "#1a1a1a" } },
];

/** Normalised centre positions. */
export const POSITION_PRESETS: { id: string; label: string; x: number; y: number }[] = [
  { id: "top", label: "Top", x: 0.5, y: 0.12 },
  { id: "middle", label: "Middle", x: 0.5, y: 0.5 },
  { id: "lower", label: "Lower third", x: 0.5, y: 0.7 },
  { id: "bottom", label: "Bottom", x: 0.5, y: 0.82 },
];

/** Overlay (picture-in-picture) corners at the default scale. */
export const PIP_PRESETS: { id: string; label: string; x: number; y: number }[] = [
  { id: "tl", label: "↖", x: 0.22, y: 0.18 },
  { id: "tr", label: "↗", x: 0.78, y: 0.18 },
  { id: "bl", label: "↙", x: 0.22, y: 0.78 },
  { id: "br", label: "↘", x: 0.78, y: 0.78 },
  { id: "full", label: "Full", x: 0.5, y: 0.5 },
];

export interface TextPatch { text?: string; style?: Partial<TextStyle>; start_ms?: number; duration_ms?: number }

/** Edit a text/caption item's text, style or timing in one validated step. */
export function updateText(p: Project, id: string, patch: TextPatch): Project {
  const { ti, ii } = locate(p, id);
  const it = p.tracks[ti].items[ii];
  if (it.type !== "text" && it.type !== "caption") throw new EditError("Only text and captions have text");
  if (patch.text !== undefined && !patch.text.trim()) throw new EditError("Text can't be empty");
  return transact(p, (q) => {
    const x = q.tracks[ti].items[ii] as Item & { type: "text" | "caption" };
    if (patch.text !== undefined) x.text = patch.text;
    if (patch.style) {
      const st = { ...x.style, ...patch.style };
      st.x = clamp(st.x, 0, 1);
      st.y = clamp(st.y, 0, 1);
      st.size_px = Math.round(clamp(st.size_px, 8, 400));
      x.style = st;
    }
    if (patch.start_ms !== undefined) x.start_ms = Math.max(0, Math.round(patch.start_ms));
    if (patch.duration_ms !== undefined) x.duration_ms = Math.round(patch.duration_ms);
  });
}

/** Set (or clear with null) a clip's overlay transform. */
export function setTransform(p: Project, id: string, t: Transform | null): Project {
  const { ti, ii } = locate(p, id);
  return transact(p, (q) => {
    const it = q.tracks[ti].items[ii];
    if (t) it.transform = clampTransform(t);
    else delete it.transform;
  });
}

/** Id of the first track of `kind` (index `nth` among them), creating tracks as needed. */
export function ensureTrack(p: Project, kind: TrackKind, name: string, nth = 0): { project: Project; id: string } {
  const q = clone(p);
  let of = q.tracks.filter((t) => t.kind === kind);
  while (of.length <= nth) {
    addTrack(q, kind, name);
    of = q.tracks.filter((t) => t.kind === kind);
  }
  return { project: q, id: of[nth].id };
}

/** Is this item on an overlay (second or later) video track? */
export function isOverlayItem(p: Project, id: string): boolean {
  const videos = p.tracks.filter((t) => t.kind === "video");
  return videos.slice(1).some((t) => t.items.some((i) => i.id === id));
}

/** Text/caption items on screen at `t`. */
export function textsAt(p: Project, t: number): (Item & { type: "text" | "caption" })[] {
  return p.tracks
    .filter((tr) => (tr.kind === "text" || tr.kind === "caption") && !tr.muted)
    .flatMap((tr) => tr.items)
    .filter((i): i is Item & { type: "text" | "caption" } => (i.type === "text" || i.type === "caption") && t >= i.start_ms && t < endMs(i));
}

/** Overlay clips on screen at `t`, bottom to top, with source time and transform. */
export function overlaysAt(p: Project, t: number): { item: Item & { type: "clip" }; sourceMs: number; transform: Transform }[] {
  return p.tracks
    .filter((tr) => tr.kind === "video")
    .slice(1)
    .filter((tr) => !tr.muted)
    .flatMap((tr) => tr.items)
    .filter((i): i is Item & { type: "clip" } => i.type === "clip" && t >= i.start_ms && t < endMs(i))
    .map((item) => ({ item, sourceMs: item.source_in_ms + (t - item.start_ms), transform: item.transform ?? DEFAULT_TRANSFORM }));
}

// ---- Phase 5: canvas dragging, karaoke, speakers ----

/** Snap guides (normalised) and their pull distance in canvas px. */
export const SNAP_GUIDES = [0.5];
export const SNAP_PX = 6;

/** New normalised centre for a text layer dragged by (dx, dy) canvas px on a
 *  w×h canvas. Snaps to the centre lines within SNAP_PX unless `free`
 *  (Alt held); returns which guides are active for drawing. */
export function dragPosition(
  start: { x: number; y: number }, dx: number, dy: number, w: number, h: number, free = false,
): { x: number; y: number; snapX: boolean; snapY: boolean } {
  let x = clamp(start.x + dx / Math.max(1, w), 0, 1);
  let y = clamp(start.y + dy / Math.max(1, h), 0, 1);
  let snapX = false, snapY = false;
  if (!free) {
    for (const g of SNAP_GUIDES) {
      if (Math.abs(x - g) * w <= SNAP_PX) { x = g; snapX = true; }
      if (Math.abs(y - g) * h <= SNAP_PX) { y = g; snapY = true; }
    }
  }
  return { x, y, snapX, snapY };
}

/** Karaoke pieces of a caption at timeline time `t`: each word and whether
 *  it has been reached (matches libass \\k: a word lights at its start and
 *  stays lit). Null when the item has no word timings. */
export function karaokeWords(it: Item, t: number): { text: string; lit: boolean }[] | null {
  if (it.type !== "caption" || !it.words?.length) return null;
  const rel = t - it.start_ms;
  return it.words.map((w) => ({ text: w.text, lit: rel >= w.start_ms }));
}

/** Per-speaker colours; MUST match ms-captions diarize::SPEAKER_PALETTE. */
export const SPEAKER_PALETTE = ["#45f2f2", "#a974ff", "#ffd24d", "#6ee27a", "#ff7a7a", "#7ab8ff", "#ff9de0", "#ffa94d"];
export const speakerColor = (i: number) => SPEAKER_PALETTE[((i % SPEAKER_PALETTE.length) + SPEAKER_PALETTE.length) % SPEAKER_PALETTE.length];

/** Distinct speakers present on caption items, ascending. */
export function speakers(p: Project): number[] {
  const set = new Set<number>();
  for (const c of captions(p)) if (c.speaker != null) set.add(c.speaker);
  return [...set].sort((a, b) => a - b);
}

/** Apply a style patch to every caption (e.g. turn karaoke on/off). */
export function styleAllCaptions(p: Project, patch: Partial<TextStyle>): Project {
  if (!captions(p).length) throw new EditError("No captions to style");
  return transact(p, (q) => {
    for (const t of q.tracks) for (const i of t.items) if (i.type === "caption") i.style = { ...i.style, ...patch };
  });
}

/** Colour each diarized caption by its speaker (unlabelled ones untouched). */
export function colorBySpeaker(p: Project): Project {
  if (!speakers(p).length) throw new EditError("No speakers detected yet");
  return transact(p, (q) => {
    for (const t of q.tracks) for (const i of t.items) if (i.type === "caption" && i.speaker != null) i.style = { ...i.style, color: speakerColor(i.speaker) };
  });
}

/** Reassign one caption to another speaker (or clear with null). */
export function setSpeaker(p: Project, id: string, speaker: number | null): Project {
  const { ti, ii } = locate(p, id);
  if (p.tracks[ti].items[ii].type !== "caption") throw new EditError("Only captions have speakers");
  return transact(p, (q) => {
    const it = q.tracks[ti].items[ii] as Item & { type: "caption" };
    if (speaker == null) delete it.speaker; else it.speaker = Math.max(0, Math.round(speaker));
  });
}

// ---- Phase 6: fades, animation, templates, duplicate ----

export const ANIMATIONS: { id: TextAnim; label: string }[] = [
  { id: "none", label: "None" }, { id: "fade", label: "Fade" }, { id: "pop", label: "Pop" }, { id: "slide-up", label: "Slide up" },
];

/** Set (or clear) an item's edge fades, clamped to its length. */
export function setFade(p: Project, id: string, f: Fade | null): Project {
  const { ti, ii } = locate(p, id);
  return transact(p, (q) => {
    const it = q.tracks[ti].items[ii];
    const fin = Math.round(clamp(f?.in_ms ?? 0, 0, it.duration_ms));
    const fout = Math.round(clamp(f?.out_ms ?? 0, 0, it.duration_ms - fin));
    if (fin || fout) it.fade = { in_ms: fin, out_ms: fout }; else delete it.fade;
  });
}

/** Opacity (0..1) of an item at timeline time `t` from its fades. */
export function fadeOpacity(it: Item, t: number): number {
  const f = it.fade;
  if (!f) return 1;
  const rel = t - it.start_ms, left = endMs(it) - t;
  let o = 1;
  if (f.in_ms > 0 && rel < f.in_ms) o = Math.min(o, Math.max(0, rel / f.in_ms));
  if (f.out_ms > 0 && left < f.out_ms) o = Math.min(o, Math.max(0, left / f.out_ms));
  return o;
}

/** Copy an item to right after itself on the same track (or the next free
 *  spot after it). Returns the new id. */
export function duplicateItem(p: Project, id: string): { project: Project; id: string } {
  const { ti, ii } = locate(p, id);
  const src = p.tracks[ti].items[ii];
  const track = p.tracks[ti];
  let start = endMs(src);
  for (const o of [...track.items].sort((a, b) => a.start_ms - b.start_ms)) {
    if (o.start_ms < start + src.duration_ms && endMs(o) > start) start = Math.max(start, endMs(o));
  }
  let newId = "";
  const project = transact(p, (q) => {
    newId = freshId(q, "i");
    q.tracks[ti].items.push({ ...structuredClone(src), id: newId, start_ms: start } as Item);
  });
  return { project, id: newId };
}

/** One-click starting points: canvas size + a styled title and caption look. */
export interface Template { id: string; label: string; w: number; h: number; title?: { text: string; style: Partial<TextStyle> }; caption: Partial<TextStyle> }
export const TEMPLATES: Template[] = [
  { id: "tiktok", label: "TikTok / Reels captions", w: 1080, h: 1920, caption: { size_px: 84, color: "#ffffff", stroke: "#000000", highlight: "#ffd400", y: 0.7, anim: "pop" } },
  { id: "shorts-hook", label: "Shorts hook title", w: 1080, h: 1920, title: { text: "WAIT FOR IT…", style: { size_px: 120, color: "#ffd400", stroke: "#000000", y: 0.15, anim: "slide-up" } }, caption: { size_px: 72, y: 0.75 } },
  { id: "youtube", label: "YouTube lower third", w: 1920, h: 1080, title: { text: "Your Name · Title", style: { size_px: 56, color: "#ffffff", stroke: "#1a1a1a", x: 0.3, y: 0.85, anim: "fade" } }, caption: { size_px: 48, y: 0.9 } },
  { id: "insta", label: "Instagram feed 4:5", w: 1080, h: 1350, caption: { size_px: 64, color: "#ffffff", stroke: "#000000", y: 0.82, anim: "fade" } },
  { id: "square-quote", label: "Square quote card", w: 1080, h: 1080, title: { text: "“Make it simple.”", style: { size_px: 96, color: "#ffffff", stroke: null, y: 0.5, anim: "fade" } }, caption: { size_px: 56, y: 0.85 } },
];

/** Apply a template: resize the canvas, restyle every caption, and add its
 *  title (if any) at `at_ms` on a text track. One undoable step. */
export function applyTemplate(p: Project, id: string, at_ms = 0): Project {
  const t = TEMPLATES.find((x) => x.id === id);
  if (!t) throw new EditError(`Unknown template ${id}`);
  let q: Project = { ...clone(p), canvas: { ...p.canvas, width: t.w, height: t.h } };
  for (const tr of q.tracks) for (const i of tr.items) if (i.type === "caption") i.style = { ...i.style, ...t.caption };
  if (t.title) {
    const tt = ensureTrack(q, "text", "Text");
    q = addItem(tt.project, tt.id, Math.max(0, Math.round(at_ms)), 3000, { type: "text", text: t.title.text, style: { ...defaultStyle(), ...t.title.style } }).project;
  }
  return q;
}

/** Replace the caption track with imported cues (SRT/VTT): the offline way to
 *  get captions without a speech model. Keeps the current caption style. */
export function setCaptionsFromCues(p: Project, cues: { start_ms: number; end_ms: number; text: string }[]): Project {
  const r = ensureTrack(p, "caption", "Captions");
  return transact(r.project, (q) => {
    const t = q.tracks.find((x) => x.id === r.id)!;
    const style = (t.items.find((i) => i.type === "caption") as (Item & { type: "caption" }) | undefined)?.style ?? defaultStyle();
    t.items = cues.map((c) => ({ id: freshId(q, "i"), start_ms: Math.round(c.start_ms), duration_ms: Math.round(c.end_ms - c.start_ms), type: "caption" as const, text: c.text, style: { ...style } }));
  });
}
