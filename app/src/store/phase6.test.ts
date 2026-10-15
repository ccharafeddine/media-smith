import { describe, expect, it } from "vitest";
import {
  addItem, applyTemplate, appendClip, defaultStyle, duplicateItem, EditError, fadeOpacity, fromJson, newProject, setFade,
  splitItem, TEMPLATES, toJson, type Item,
} from "./model";
import { createEditor } from "./store";
import { actionFor, isTypingTarget, SHORTCUTS } from "../lib/shortcuts";
import { clearSnapshot, debounce, KEY, readSnapshot, writeSnapshot, type KV } from "../lib/autosave";
import { rulerStep, rulerTicks, visibleItems } from "../lib/view";

const items = (p: ReturnType<typeof newProject>) => p.tracks.flatMap((t) => t.items);

function memKV(): KV & { m: Map<string, string> } {
  const m = new Map<string, string>();
  return { m, getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v), removeItem: (k) => void m.delete(k) };
}

describe("fades", () => {
  it("clamps, clears and drives preview opacity", () => {
    const p0 = newProject("t");
    const v = p0.tracks.find((t) => t.kind === "video")!.id;
    const r = appendClip(p0, v, "a.mp4", 2000);
    const p = setFade(r.project, r.id, { in_ms: 1500, out_ms: 1500 });
    const it = items(p).find((i) => i.id === r.id)!;
    expect(it.fade).toEqual({ in_ms: 1500, out_ms: 500 });
    expect(fadeOpacity(it, 0)).toBe(0);
    expect(fadeOpacity(it, 750)).toBeCloseTo(0.5);
    expect(fadeOpacity(it, 1600)).toBeCloseTo(0.8);
    expect(items(setFade(p, r.id, null)).find((i) => i.id === r.id)!.fade).toBeUndefined();
    expect(fromJson(toJson(p))).toEqual(p);
  });

  it("split keeps fade-in left, fade-out right, and divides karaoke words", () => {
    const p0 = newProject("t");
    const cap = p0.tracks.find((t) => t.kind === "caption")!.id;
    const a = addItem(p0, cap, 1000, 2000, {
      type: "caption", text: "a b c", style: defaultStyle(),
      words: [{ start_ms: 0, end_ms: 400, text: "a" }, { start_ms: 800, end_ms: 1200, text: "b" }, { start_ms: 1500, end_ms: 1900, text: "c" }],
    });
    const f = setFade(a.project, a.id, { in_ms: 300, out_ms: 500 });
    const r = splitItem(f, a.id, 2000);
    const [l, rt] = items(r.project) as (Item & { type: "caption" })[];
    expect(l.fade).toEqual({ in_ms: 300, out_ms: 0 });
    expect(rt.fade).toEqual({ in_ms: 0, out_ms: 500 });
    expect(l.words!.map((w) => [w.text, w.start_ms, w.end_ms])).toEqual([["a", 0, 400], ["b", 800, 1000]]);
    expect(rt.words!.map((w) => [w.text, w.start_ms, w.end_ms])).toEqual([["b", 0, 200], ["c", 500, 900]]);
  });
});

describe("templates and duplicate", () => {
  it("applies a template in one step and restyles captions", () => {
    const p0 = newProject("t");
    const cap = p0.tracks.find((t) => t.kind === "caption")!.id;
    const a = addItem(p0, cap, 0, 1000, { type: "caption", text: "hi", style: defaultStyle() });
    const q = applyTemplate(a.project, "shorts-hook", 500);
    expect(q.canvas).toMatchObject({ width: 1080, height: 1920 });
    const cs = items(q).find((i) => i.id === a.id) as Item & { type: "caption" };
    expect(cs.style.size_px).toBe(72);
    const title = items(q).find((i) => i.type === "text") as Item & { type: "text" };
    expect(title).toMatchObject({ start_ms: 500, text: "WAIT FOR IT…" });
    expect(title.style.anim).toBe("slide-up");
    expect(() => applyTemplate(q, "nope")).toThrow(EditError);
    for (const t of TEMPLATES) expect(() => applyTemplate(newProject("x"), t.id)).not.toThrow();
  });

  it("duplicates into the next free gap", () => {
    const p0 = newProject("t");
    const v = p0.tracks.find((t) => t.kind === "video")!.id;
    const a = appendClip(p0, v, "a.mp4", 1000);
    const b = appendClip(a.project, v, "b.mp4", 500);
    const d = duplicateItem(b.project, a.id);
    const dup = items(d.project).find((i) => i.id === d.id)!;
    expect(dup.start_ms).toBe(1500);
    expect(dup.duration_ms).toBe(1000);
  });

  it("editor redo replays undone edits and a new edit clears redo", () => {
    const e = createEditor();
    e.addText("one", 0);
    e.addText("two", 5000);
    expect(e.undo()).toBe(true);
    expect(items(e.project()).length).toBe(1);
    expect(e.redo()).toBe(true);
    expect(items(e.project()).length).toBe(2);
    e.undo();
    e.addText("three", 9000);
    expect(e.redo()).toBe(false);
  });
});

describe("shortcuts", () => {
  it("maps keys to actions", () => {
    expect(actionFor({ key: " " })).toBe("play");
    expect(actionFor({ key: "z", ctrlKey: true })).toBe("undo");
    expect(actionFor({ key: "Z", ctrlKey: true, shiftKey: true })).toBe("redo");
    expect(actionFor({ key: "y", metaKey: true })).toBe("redo");
    expect(actionFor({ key: "d", ctrlKey: true })).toBe("duplicate");
    expect(actionFor({ key: "ArrowLeft", shiftKey: true })).toBe("secBack");
    expect(actionFor({ key: "S" })).toBe("split");
    expect(actionFor({ key: "s", altKey: true })).toBeNull();
    expect(actionFor({ key: "q" })).toBeNull();
    expect(new Set(SHORTCUTS.map((s) => s.action)).size).toBe(SHORTCUTS.length);
    expect(isTypingTarget({ tagName: "TEXTAREA" } as unknown as EventTarget)).toBe(true);
    expect(isTypingTarget({ tagName: "DIV" } as unknown as EventTarget)).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe("autosave", () => {
  it("writes, reads, expires and clears snapshots", () => {
    const kv = memKV();
    expect(readSnapshot(kv)).toBeNull();
    expect(writeSnapshot(kv, "p", "{}", 1000)).toBe(true);
    expect(readSnapshot(kv, 2000)).toEqual({ at: 1000, name: "p", json: "{}" });
    expect(readSnapshot(kv, 1000 + 15 * 24 * 3600_000)).toBeNull();
    kv.m.set(KEY, "{broken");
    expect(readSnapshot(kv)).toBeNull();
    clearSnapshot(kv);
    expect(kv.m.size).toBe(0);
    const full: KV = { getItem: () => null, setItem: () => { throw new Error("quota"); }, removeItem: () => undefined };
    expect(writeSnapshot(full, "p", "{}")).toBe(false);
  });

  it("debounce coalesces calls and flush runs the last one", async () => {
    const seen: number[] = [];
    const d = debounce((n: number) => seen.push(n), 20);
    d(1); d(2); d(3);
    d.flush();
    expect(seen).toEqual([3]);
    d(4);
    await new Promise((r) => setTimeout(r, 40));
    expect(seen).toEqual([3, 4]);
  });
});

describe("timeline viewport", () => {
  it("spaces ruler labels and culls off-screen items", () => {
    expect(rulerStep(60)).toBe(1);
    expect(rulerStep(5)).toBe(15);
    expect(rulerTicks(0, 600, 60, 3600).at(-1)).toBeLessThanOrEqual(14);
    const many = Array.from({ length: 10_000 }, (_, i) => ({ id: `i${i}`, start_ms: i * 1000, duration_ms: 900 }));
    const vis = visibleItems(many, 6000, 600, 60, "i9999");
    expect(vis.length).toBeLessThan(40);
    expect(vis.some((i) => i.id === "i9999")).toBe(true);
    expect(vis.some((i) => i.id === "i100")).toBe(true);
  });
});
