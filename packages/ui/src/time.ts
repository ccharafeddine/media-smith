/** Format milliseconds as m:ss.t (or h:mm:ss.t past an hour). */
export function formatTime(ms: number): string {
  const safe = Math.max(0, Math.round(ms));
  const tenths = Math.floor((safe % 1000) / 100);
  const total = Math.floor(safe / 1000);
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}.${tenths}` : `${m}:${ss}.${tenths}`;
}

/** Pixel x for a time at a given zoom (px per second). */
export function timeToX(ms: number, pxPerSec: number): number {
  return (ms / 1000) * pxPerSec;
}
