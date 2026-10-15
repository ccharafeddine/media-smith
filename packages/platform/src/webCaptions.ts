/** Browser auto-captions: Whisper base in a Web Worker, placed on the timeline
 *  with the same rules as the desktop (ms_captions::place::place_segments and
 *  Project::replace_caption_cues). Speaker detection stays desktop-only. */

export interface WWord { start_ms: number; end_ms: number; text: string }
export interface WSegment { start_ms: number; end_ms: number; text: string; words: WWord[] | null }
export interface Cue { start_ms: number; duration_ms: number; text: string; words: WWord[] }

/** Group whisper word chunks into segments at sentence ends, pauses and ~5 s. */
export function segmentsFromWords(words: WWord[]): WSegment[] {
  const segs: WSegment[] = [];
  let cur: WWord[] = [];
  const flush = () => {
    if (cur.length) segs.push({ start_ms: cur[0].start_ms, end_ms: cur[cur.length - 1].end_ms, text: cur.map((w) => w.text).join(" "), words: cur });
    cur = [];
  };
  for (const w of words) {
    const prev = cur[cur.length - 1];
    if (prev && (w.start_ms - prev.end_ms > 1000 || w.end_ms - cur[0].start_ms > 5000)) flush();
    cur.push(w);
    if (/[.!?…]["')\]]?$/.test(w.text) || (/,$/.test(w.text) && w.end_ms - cur[0].start_ms > 2500)) flush();
  }
  flush();
  return segs;
}

function split(seg: WSegment, max: number): [number, number, string, WWord[]][] {
  if (seg.words && max > 0 && seg.words.length > max) {
    const out: [number, number, string, WWord[]][] = [];
    for (let i = 0; i < seg.words.length; i += max) {
      const c = seg.words.slice(i, i + max);
      out.push([c[0].start_ms, c[c.length - 1].end_ms, c.map((w) => w.text).join(" "), c]);
    }
    return out;
  }
  return [[seg.start_ms, seg.end_ms, seg.text, seg.words ?? []]];
}

/** Port of place_segments: clip-relative segments -> timeline cues. */
export function placeSegments(clipStart: number, clipDur: number, segs: WSegment[], maxWords: number): Cue[] {
  const MIN = 200;
  const out: Cue[] = [];
  let cursor = 0;
  for (const seg of segs) {
    for (const [s0, e0, text, words] of split(seg, maxWords)) {
      const s = Math.min(Math.max(s0, cursor), clipDur);
      let e = Math.max(Math.min(e0, clipDur), s);
      if (!text.trim() || e - s < MIN / 2) continue;
      e = Math.min(Math.max(s + MIN, e), clipDur);
      out.push({
        start_ms: clipStart + s, duration_ms: e - s, text: text.trim(),
        words: words.filter((w) => w.end_ms > s && w.start_ms < e).map((w) => ({ start_ms: clipStart + Math.max(w.start_ms, s), end_ms: clipStart + Math.min(w.end_ms, e), text: w.text })),
      });
      cursor = e;
    }
  }
  return out;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Json = any;
const DEFAULT_STYLE = { font: "Syne", size_px: 64, color: "#ffffff", stroke: "#000000", x: 0.5, y: 0.82 };

/** Port of Project::replace_caption_cues (track created if missing). */
export function replaceCaptionCues(p: Json, cues: Cue[]): Json {
  let t = p.tracks.find((x: Json) => x.kind === "caption");
  if (!t) {
    t = { id: `t${++p.next_id}`, kind: "caption", name: "Captions", muted: false, items: [] };
    p.tracks.push(t);
  }
  const placed: Json[] = [];
  let cursor = 0;
  for (const c of cues) {
    const text = c.text.trim();
    if (c.duration_ms === 0 || !text || c.start_ms < cursor) continue;
    const end = c.start_ms + c.duration_ms;
    const words = c.words
      .filter((w) => w.text.trim() && w.end_ms > c.start_ms && w.start_ms < end)
      .map((w) => ({ start_ms: Math.max(w.start_ms, c.start_ms) - c.start_ms, end_ms: Math.min(w.end_ms, end) - c.start_ms, text: w.text.trim() }));
    placed.push({ id: `i${++p.next_id}`, start_ms: c.start_ms, duration_ms: c.duration_ms, type: "caption", text, style: { ...DEFAULT_STYLE }, words, speaker: null });
    cursor = end;
  }
  t.items = placed;
  return p;
}

const audioCache = new Map<File, Promise<AudioBuffer>>();
/** 16 kHz mono samples of [inMs, inMs+durMs) of a file. */
export async function audioRange(file: File, inMs: number, durMs: number): Promise<Float32Array | null> {
  if (!audioCache.has(file)) {
    const ctx = new OfflineAudioContext(1, 16000, 16000);
    audioCache.set(file, file.arrayBuffer().then((b) => ctx.decodeAudioData(b)));
  }
  let buf: AudioBuffer;
  try { buf = await audioCache.get(file)!; } catch { audioCache.delete(file); return null; }
  const a = Math.floor((inMs / 1000) * buf.sampleRate), z = Math.min(buf.length, Math.floor(((inMs + durMs) / 1000) * buf.sampleRate));
  if (z <= a) return null;
  const out = new Float32Array(z - a);
  for (let c = 0; c < buf.numberOfChannels; c++) { const d = buf.getChannelData(c); for (let i = a; i < z; i++) out[i - a] += d[i] / buf.numberOfChannels; }
  return out;
}

let worker: Worker | null = null;
let abort: ((e: unknown) => void) | null = null;

/** Run Whisper on 16 kHz audio; returns clip-relative word timings. */
export function transcribeWords(audio: Float32Array, modelRoot: string, language: string | null, onProgress: (r: number) => void): Promise<WWord[]> {
  worker ??= new Worker(new URL("./whisper.worker.ts", import.meta.url), { type: "module" });
  const w = worker;
  return new Promise<WWord[]>((resolve, reject) => {
    abort = reject;
    w.onmessage = (ev) => {
      const m = ev.data;
      if (m.type === "progress") onProgress(m.percent / 100);
      else if (m.type === "error") reject(new Error(m.message));
      else if (m.type === "done") {
        resolve((m.chunks as { text: string; timestamp: [number, number | null] }[])
          .map((c) => ({ start_ms: Math.round(c.timestamp[0] * 1000), end_ms: Math.round((c.timestamp[1] ?? m.secs) * 1000), text: c.text.trim() }))
          .filter((x) => x.text));
      }
    };
    w.onerror = (e) => reject(new Error(e.message || "Transcription failed in the browser."));
    w.postMessage({ modelRoot, audio, language: language && language !== "auto" ? language : null, translate: false, words: true }, [audio.buffer]);
  }).finally(() => { abort = null; });
}

export function cancelTranscribe(): void {
  if (!abort) return;
  worker?.terminate();
  worker = null;
  abort(new Error("cancelled"));
}
