import type { Project } from "./model";
import { endMs } from "./model";

/** Edges a dragged item can snap to: every other item's start/end, the
 *  playhead, and zero. */
export function snapTargets(p: Project, playhead: number, excludeId?: string): number[] {
  const t = new Set<number>([0, Math.round(playhead)]);
  for (const tr of p.tracks) for (const i of tr.items) if (i.id !== excludeId) { t.add(i.start_ms); t.add(endMs(i)); }
  return [...t].sort((a, b) => a - b);
}

/** Snap `ms` to the nearest target within `threshold` ms; otherwise unchanged. */
export function snap(ms: number, targets: number[], threshold: number): number {
  let best = ms;
  let dist = threshold + 1;
  for (const t of targets) {
    const d = Math.abs(t - ms);
    if (d <= threshold && d < dist) { best = t; dist = d; }
  }
  return best;
}

/** Snap a moving block by whichever of its edges lands closer to a target. */
export function snapMove(start: number, duration: number, targets: number[], threshold: number): number {
  const s = snap(start, targets, threshold);
  const e = snap(start + duration, targets, threshold) - duration;
  const ds = Math.abs(s - start);
  const de = Math.abs(e - start);
  if (s !== start && (e === start || ds <= de)) return s;
  if (e !== start) return Math.max(0, e);
  return start;
}

/** Pixel distance that counts as "close" turned into ms at the current zoom. */
export const snapThresholdMs = (pxPerSec: number, px = 8) => (px / pxPerSec) * 1000;
