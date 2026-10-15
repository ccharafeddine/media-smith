/** Timeline viewport maths: only what's on screen is rendered, so long
 *  timelines (hours, thousands of captions) stay responsive. */
import type { Item } from "../store/model";

/** Seconds between ruler labels so labels sit at least `minPx` apart. */
export function rulerStep(pxPerSec: number, minPx = 60): number {
  for (const s of [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600]) if (s * pxPerSec >= minPx) return s;
  return 3600;
}

/** Ruler label times (s) within the visible window plus a margin. */
export function rulerTicks(scrollPx: number, widthPx: number, pxPerSec: number, totalSec: number, margin = 200): number[] {
  const step = rulerStep(pxPerSec);
  const from = Math.max(0, Math.floor((scrollPx - margin) / pxPerSec / step) * step);
  const to = Math.min(totalSec, (scrollPx + widthPx + margin) / pxPerSec);
  const out: number[] = [];
  for (let s = from; s <= to; s += step) out.push(s);
  return out;
}

/** Items overlapping the visible window (plus margin); the selected item is
 *  always kept so an in-progress drag never unmounts. */
export function visibleItems<T extends Pick<Item, "id" | "start_ms" | "duration_ms">>(
  items: T[], scrollPx: number, widthPx: number, pxPerSec: number, keep: string | null = null, margin = 400,
): T[] {
  const a = ((scrollPx - margin) / pxPerSec) * 1000;
  const b = ((scrollPx + widthPx + margin) / pxPerSec) * 1000;
  return items.filter((i) => i.id === keep || (i.start_ms + i.duration_ms >= a && i.start_ms <= b));
}
