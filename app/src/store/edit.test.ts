import { describe, expect, it } from "vitest";
import {
  addItem, appendClip, clampTransform, DEFAULT_TRANSFORM, EditError, ensureTrack, fromJson, isOverlayItem, newProject,
  overlaysAt, setTransform, STYLE_PRESETS, textsAt, toJson, updateText, defaultStyle,
} from "./model";
import { createEditor } from "./store";

function withCaption() {
  const p = newProject("t");
  const cap = p.tracks.find((t) => t.kind === "caption")!.id;
  const r = addItem(p, cap, 1000, 2000, { type: "caption", text: "hello", style: defaultStyle() });
  const r2 = addItem(r.project, cap, 4000, 1000, { type: "caption", text: "next", style: defaultStyle() });
  return { p: r2.project, id: r.id, next: r2.id };
}

describe("inline text editing", () => {
  it("edits text, style and timing in one step", () => {
    const { p, id } = withCaption();
    const q = updateText(p, id, { text: "Hi!", style: { ...STYLE_PRESETS[1].style, y: 0.12 }, start_ms: 500, duration_ms: 1500 });
    const it = q.tracks.find((t) => t.kind === "caption")!.items[0];
    expect(it).toMatchObject({ text: "Hi!", start_ms: 500, duration_ms: 1500, style: { color: "#ffd400", size_px: 84, y: 0.12, x: 0.5 } });
    expect(p.tracks.find((t) => t.kind === "caption")!.items[0]).toMatchObject({ text: "hello" }); // original untouched
  });

  it("clamps position and size, and rejects bad edits", () => {
    const { p, id } = withCaption();
    const q = updateText(p, id, { style: { x: 3, y: -1, size_px: 9999 } });
    expect(q.tracks.find((t) => t.kind === "caption")!.items[0]).toMatchObject({ style: { x: 1, y: 0, size_px: 400 } });
    expect(() => updateText(p, id, { text: "  " })).toThrow(EditError);
    expect(() => updateText(p, id, { duration_ms: 3500 })).toThrow(/overlaps/); // runs into "next"
    expect(() => updateText(p, id, { duration_ms: 0 })).toThrow(/no length/);
    const v = appendClip(p, p.tracks[0].id, "a.mp4", 1000);
    expect(() => updateText(v.project, v.id, { text: "x" })).toThrow(/Only text/);
  });

  it("finds texts on screen", () => {
    const { p } = withCaption();
    expect(textsAt(p, 1500).map((i) => i.text)).toEqual(["hello"]);
    expect(textsAt(p, 3500)).toEqual([]);
  });

  it("editor adds + Text on a new text track and edits are undoable", () => {
    const e = createEditor();
    const id = e.addText("Title", 0)!;
    expect(e.selected()).toBe(id);
    expect(e.project().tracks.filter((t) => t.kind === "text")).toHaveLength(1);
    e.updateText(id, { text: "Big title" });
    expect(textsAt(e.project(), 10)[0].text).toBe("Big title");
    e.undo();
    expect(textsAt(e.project(), 10)[0].text).toBe("Title");
    e.addText("Again", 5000);
    expect(e.project().tracks.filter((t) => t.kind === "text")).toHaveLength(1);
  });
});

describe("overlay track", () => {
  it("imports picture-in-picture onto a second video track", () => {
    const e = createEditor();
    e.importClip("main.mp4", 10000);
    e.seek(2000);
    const id = e.importOverlay("cam.mp4", 3000)!;
    const p = e.project();
    expect(p.tracks.filter((t) => t.kind === "video")).toHaveLength(2);
    expect(isOverlayItem(p, id)).toBe(true);
    const o = overlaysAt(p, 2500);
    expect(o).toHaveLength(1);
    expect(o[0]).toMatchObject({ sourceMs: 500, transform: DEFAULT_TRANSFORM });
    expect(overlaysAt(p, 6000)).toEqual([]);
    // Round-trips through JSON (the Rust model reads the same `transform` key).
    expect(fromJson(toJson(p))).toEqual(p);
  });

  it("sets, clamps and clears transforms", () => {
    let p = newProject();
    const t = ensureTrack(p, "video", "Overlay", 1);
    const r = addItem(t.project, t.id, 0, 1000, { type: "clip", source: "x.mp4", source_in_ms: 0, volume: 1 });
    p = setTransform(r.project, r.id, { x: 2, y: 0.5, scale: 0, opacity: 0.5 });
    expect(p.tracks[3].items[0].transform).toEqual({ x: 1, y: 0.5, scale: 0.05, opacity: 0.5 });
    p = setTransform(p, r.id, null);
    expect("transform" in p.tracks[3].items[0]).toBe(false);
    expect(clampTransform({ x: NaN, y: 0, scale: 2, opacity: -1 })).toEqual({ x: 0, y: 0, scale: 1, opacity: 0 });
    expect(ensureTrack(p, "video", "Overlay", 1).id).toBe(t.id);
  });
});

describe("auto-caption result", () => {
  it("editor.replace swaps the project in one undoable step", () => {
    const e = createEditor();
    const before = e.project();
    const next = structuredClone(before);
    next.tracks.find((t) => t.kind === "caption")!.items.push({ id: "i99", start_ms: 0, duration_ms: 500, type: "caption", text: "auto", style: defaultStyle() });
    expect(e.replace(next)).toBe(true);
    expect(textsAt(e.project(), 100)[0].text).toBe("auto");
    e.undo();
    expect(e.project()).toEqual(before);
  });
});
