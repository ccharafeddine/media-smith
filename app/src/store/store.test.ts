import { createRoot } from "solid-js";
import { describe, expect, it } from "vitest";
import { createEditor } from "./store";

const withEditor = (f: (e: ReturnType<typeof createEditor>) => void) => createRoot((dispose) => { f(createEditor()); dispose(); });

describe("editor store", () => {
  it("imports, splits at the playhead, deletes and undoes", () => withEditor((e) => {
    const a = e.importClip("a.mp4", 4000)!;
    e.importClip("b.mp4", 2000);
    expect(e.duration()).toBe(6000);
    e.select(a);
    e.seek(1500);
    expect(e.splitAtPlayhead()).toBe(true);
    expect(e.project().tracks[0].items).toHaveLength(3);
    expect(e.deleteSelected()).toBe(true);
    expect(e.project().tracks[0].items).toHaveLength(2);
    expect(e.undo()).toBe(true);
    expect(e.project().tracks[0].items).toHaveLength(3);
    expect(e.dirty()).toBe(true);
  }));

  it("reports rejected edits without changing the project", () => withEditor((e) => {
    const a = e.importClip("a.mp4", 4000)!;
    const b = e.importClip("b.mp4", 2000)!;
    const before = e.project();
    expect(e.move(b, 1000)).toBe(false);
    expect(e.message()).toMatch(/overlaps/);
    expect(e.project()).toBe(before);
    expect(e.trim(a, 0, 3000)).toBe(true);
    expect(e.message()).toBeNull();
  }));

  it("clamps seeking and needs a selection to split", () => withEditor((e) => {
    e.importClip("a.mp4", 1000);
    e.select(null);
    e.seek(99_999);
    expect(e.playhead()).toBe(1000);
    expect(e.splitAtPlayhead()).toBe(false);
    expect(e.addCaption("hi", 0)).toBe(true);
  }));
});
