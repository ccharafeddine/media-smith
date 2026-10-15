/** Crash recovery: the open project is written to local storage a moment
 *  after each edit, and offered back on the next launch if it was never
 *  saved. Storage is injected so it's testable (and works on web + desktop). */
export interface KV { getItem(k: string): string | null; setItem(k: string, v: string): void; removeItem(k: string): void }

export const KEY = "mediasmith.autosave.v1";

export interface Snapshot { at: number; name: string; json: string }

export function writeSnapshot(kv: KV, name: string, json: string, now = Date.now()): boolean {
  try {
    kv.setItem(KEY, JSON.stringify({ at: now, name, json } satisfies Snapshot));
    return true;
  } catch {
    return false; // quota or private mode: recovery is best effort
  }
}

/** The pending snapshot, or null if none / unreadable / older than `maxAgeMs`. */
export function readSnapshot(kv: KV, now = Date.now(), maxAgeMs = 14 * 24 * 3600_000): Snapshot | null {
  try {
    const raw = kv.getItem(KEY);
    if (!raw) return null;
    const s = JSON.parse(raw) as Snapshot;
    if (typeof s?.json !== "string" || typeof s.at !== "number" || now - s.at > maxAgeMs) return null;
    return s;
  } catch {
    return null;
  }
}

export function clearSnapshot(kv: KV) {
  try { kv.removeItem(KEY); } catch { /* ignore */ }
}

/** Debounce `f` by `ms`; `flush()` runs a pending call now. */
export function debounce<A extends unknown[]>(f: (...a: A) => void, ms: number) {
  let t: ReturnType<typeof setTimeout> | undefined;
  let last: A | undefined;
  const run = (...a: A) => { last = a; clearTimeout(t); t = setTimeout(() => { t = undefined; f(...a); }, ms); };
  run.flush = () => { if (t !== undefined && last) { clearTimeout(t); t = undefined; f(...last); } };
  return run;
}
