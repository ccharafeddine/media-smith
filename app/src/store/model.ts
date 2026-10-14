/** TypeScript mirror of the ms-project JSON schema plus the same edit rules
 *  (no overlaps, no empty items, left-trim shifts the source offset). The
 *  desktop shell re-validates with the Rust model on save and export. */

export type TrackKind = "video" | "audio" | "text" | "caption";

export interface TextStyle { font: string; size_px: number; color: string; stroke: string | null; x: number; y: number }

export type ItemKind =
  | { type: "clip"; source: string; source_in_ms: number; volume: number }
  | { type: "text"; text: string; style: TextStyle }
  | { type: "caption"; text: string; style: TextStyle };

export type Item = { id: string; start_ms: number; duration_ms: number } & ItemKind;

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
