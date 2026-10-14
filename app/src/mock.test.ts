import { describe, expect, it } from "vitest";
import { DURATION, PRESETS, TOOLS, TRACKS } from "./mock";

describe("mock timeline", () => {
  it("has the six tools in sidebar order", () => {
    expect(TOOLS).toEqual(["Edit", "Clip", "GIF", "Captions", "Design", "Import"]);
  });
  it("keeps every item inside the project and non-overlapping per track", () => {
    for (const t of TRACKS) {
      const items = [...t.items].sort((a, b) => a.start - b.start);
      items.forEach((it, i) => {
        expect(it.start + it.dur).toBeLessThanOrEqual(DURATION);
        if (i > 0) expect(it.start).toBeGreaterThanOrEqual(items[i - 1].start + items[i - 1].dur);
      });
    }
  });
  it("has vertical presets for the social targets", () => {
    expect(PRESETS.filter((p) => p.h > p.w).map((p) => p.id)).toEqual(["tiktok", "reels", "shorts"]);
  });
});
