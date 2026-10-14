import { describe, expect, it } from "vitest";
import { appendClip, newProject } from "./model";
import { snap, snapMove, snapTargets, snapThresholdMs } from "./snap";

describe("snapping", () => {
  const p0 = newProject();
  const v = p0.tracks[0].id;
  const r1 = appendClip(p0, v, "a", 2000);
  const r2 = appendClip(r1.project, v, "b", 1000);
  const p = r2.project;

  it("collects item edges and the playhead, excluding the dragged item", () => {
    expect(snapTargets(p, 1234)).toEqual([0, 1234, 2000, 3000]);
    expect(snapTargets(p, 0, r2.id)).toEqual([0, 2000]);
  });
  it("snaps within the threshold only", () => {
    expect(snap(2080, [0, 2000], 100)).toBe(2000);
    expect(snap(2200, [0, 2000], 100)).toBe(2200);
    expect(snap(1990, [1950, 2000], 100)).toBe(2000);
  });
  it("move snaps by the closer edge", () => {
    // 1000ms block starting at 4950 → start snaps to 5000? No target; end 5950 none → unchanged.
    expect(snapMove(4950, 1000, [0, 3000], 100)).toBe(4950);
    // start 3040 → 3000 (start edge)
    expect(snapMove(3040, 1000, [0, 3000], 100)).toBe(3000);
    // end 2960 (start 1960) → end snaps to 3000 → start 2000
    expect(snapMove(1960, 1000, [0, 3000, 9000], 100)).toBe(2000);
  });
  it("threshold scales with zoom", () => {
    expect(snapThresholdMs(80)).toBe(100);
    expect(snapThresholdMs(160)).toBe(50);
  });
});
