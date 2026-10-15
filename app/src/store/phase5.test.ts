import { describe, expect, it } from "vitest";
import {
  addItem, colorBySpeaker, defaultStyle, dragPosition, EditError, fromJson, karaokeWords, newProject, setSpeaker,
  speakerColor, speakers, styleAllCaptions, toJson, type Item,
} from "./model";
import { createEditor } from "./store";

function project() {
  const p = newProject("t");
  const cap = p.tracks.find((t) => t.kind === "caption")!.id;
  const a = addItem(p, cap, 1000, 1000, {
    type: "caption", text: "hey you", style: defaultStyle(),
    words: [{ start_ms: 0, end_ms: 300, text: "hey" }, { start_ms: 400, end_ms: 1000, text: "you" }], speaker: 1,
  });
  const b = addItem(a.project, cap, 3000, 500, { type: "caption", text: "plain", style: defaultStyle() });
  return { p: b.project, a: a.id, b: b.id };
}
const item = (p: ReturnType<typeof project>["p"], id: string) => p.tracks.flatMap((t) => t.items).find((i) => i.id === id)! as Item & { type: "caption" };

describe("canvas drag", () => {
  it("moves by pixel delta, clamps, and snaps to centre lines", () => {
    const m = dragPosition({ x: 0.2, y: 0.2 }, 100, 0, 1000, 1000);
    expect(m.x).toBeCloseTo(0.3);
    expect(m).toMatchObject({ y: 0.2, snapX: false, snapY: false });
    expect(dragPosition({ x: 0.9, y: 0.1 }, 500, -500, 1000, 1000)).toEqual({ x: 1, y: 0, snapX: false, snapY: false });
    expect(dragPosition({ x: 0.3, y: 0.3 }, 196, 204, 1000, 1000)).toEqual({ x: 0.5, y: 0.5, snapX: true, snapY: true });
    const free = dragPosition({ x: 0.3, y: 0.3 }, 196, 0, 1000, 1000, true);
    expect(free.snapX).toBe(false);
    expect(free.x).toBeCloseTo(0.496);
  });

  it("commits a drag as one undoable edit", () => {
    const { p, a } = project();
    const e = createEditor(p);
    expect(e.moveText(a, 0.25, 0.4)).toBe(true);
    expect(item(e.project(), a).style).toMatchObject({ x: 0.25, y: 0.4 });
    e.undo();
    expect(item(e.project(), a).style).toMatchObject({ x: 0.5, y: 0.82 });
  });
});

describe("karaoke", () => {
  it("lights words as the playhead reaches them", () => {
    const { p, a, b } = project();
    expect(karaokeWords(item(p, a), 1000)).toEqual([{ text: "hey", lit: true }, { text: "you", lit: false }]);
    expect(karaokeWords(item(p, a), 1450)!.every((w) => w.lit)).toBe(true);
    expect(karaokeWords(item(p, b), 3100)).toBeNull();
  });

  it("toggles highlight on all captions and round-trips JSON", () => {
    const { p, a, b } = project();
    const q = styleAllCaptions(p, { highlight: "#ffd400" });
    expect(item(q, a).style.highlight).toBe("#ffd400");
    expect(item(q, b).style.highlight).toBe("#ffd400");
    const back = fromJson(toJson(q));
    expect(item(back, a).words).toHaveLength(2);
    expect(() => styleAllCaptions(newProject("e"), { highlight: null })).toThrow(EditError);
  });
});

describe("speakers", () => {
  it("lists, recolours and reassigns speakers", () => {
    const { p, a, b } = project();
    expect(speakers(p)).toEqual([1]);
    const q = colorBySpeaker(p);
    expect(item(q, a).style.color).toBe(speakerColor(1));
    expect(item(q, b).style.color).toBe("#ffffff");
    const r = setSpeaker(q, b, 0);
    expect(speakers(r)).toEqual([0, 1]);
    expect(item(setSpeaker(r, a, null), a).speaker).toBeUndefined();
    expect(speakerColor(9)).toBe(speakerColor(1));
    expect(() => colorBySpeaker(newProject("e"))).toThrow(/No speakers/);
  });
});
