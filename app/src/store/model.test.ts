import { describe, expect, it } from "vitest";
import {
  addItem, appendClip, clipAt, deleteItem, duration, EditError, fromJson, moveItem, newProject, splitItem, toJson, trimItem,
} from "./model";

function sample() {
  let p = newProject("t");
  const v = p.tracks[0].id;
  let r = appendClip(p, v, "a.mp4", 5000);
  p = r.project;
  const a = r.id;
  r = appendClip(p, v, "b.mp4", 3000);
  return { p: r.project, v, a, b: r.id };
}

describe("project model", () => {
  it("appends clips end to end", () => {
    const { p } = sample();
    expect(p.tracks[0].items.map((i) => [i.start_ms, i.duration_ms])).toEqual([[0, 5000], [5000, 3000]]);
    expect(duration(p)).toBe(8000);
  });

  it("moves, rejecting overlaps without mutating", () => {
    const { p, a, b } = sample();
    const moved = moveItem(p, b, 9000);
    expect(moved.tracks[0].items[1].start_ms).toBe(9000);
    expect(() => moveItem(p, b, 4000)).toThrow(EditError);
    expect(p.tracks[0].items[1].start_ms).toBe(5000);
    expect(() => moveItem(p, a, 0, p.tracks[2].id)).toThrow(/different kind/);
  });

  it("left trim shifts the source offset", () => {
    const { p, a } = sample();
    const t = trimItem(p, a, 1000, 3000);
    const it = t.tracks[0].items[0];
    expect([it.start_ms, it.duration_ms]).toEqual([1000, 3000]);
    expect(it.type === "clip" && it.source_in_ms).toBe(1000);
    expect(() => trimItem(p, a, 0, 0)).toThrow(EditError);
    expect(() => trimItem(t, a, 500, 4000)).not.toThrow();
  });

  it("split keeps source continuity and fresh ids", () => {
    const { p, a } = sample();
    const { project, id } = splitItem(p, a, 2000);
    const [l, r] = project.tracks[0].items;
    expect([l.duration_ms, r.start_ms, r.duration_ms]).toEqual([2000, 2000, 3000]);
    expect(r.id).toBe(id);
    expect(r.type === "clip" && r.source_in_ms).toBe(2000);
    expect(() => splitItem(p, a, 0)).toThrow();
  });

  it("deletes and finds the clip under the playhead", () => {
    const { p, a, b } = sample();
    expect(clipAt(p, 5500)?.item.id).toBe(b);
    expect(clipAt(p, 5500)?.sourceMs).toBe(500);
    const d = deleteItem(p, a);
    expect(d.tracks[0].items.map((i) => i.id)).toEqual([b]);
    expect(clipAt(d, 100)).toBeNull();
  });

  it("round-trips JSON and rejects bad files", () => {
    const { p } = sample();
    expect(fromJson(toJson(p))).toEqual(p);
    expect(() => fromJson("{")).toThrow(EditError);
    const bad = structuredClone(p);
    bad.tracks[0].items[1].start_ms = 100;
    expect(() => fromJson(JSON.stringify(bad))).toThrow(/overlaps/);
    expect(() => fromJson(JSON.stringify({ ...p, version: 99 }))).toThrow(/version/);
  });

  it("reads the Rust ms-project JSON shape", () => {
    const rust = `{"version":1,"name":"demo","canvas":{"width":1080,"height":1920,"fps":30},"tracks":[{"id":"t1","kind":"video","name":"V1","muted":false,"items":[{"id":"i3","start_ms":0,"duration_ms":5000,"type":"clip","source":"a.mp4","source_in_ms":0,"volume":1.0}]}],"next_id":3}`;
    const p = fromJson(rust);
    const { id } = addItem(p, "t1", 5000, 1000, { type: "clip", source: "b.mp4", source_in_ms: 0, volume: 1 });
    expect(id).toBe("i4");
  });
});
