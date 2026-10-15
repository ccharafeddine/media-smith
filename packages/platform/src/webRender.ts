import type { FFmpeg } from "@ffmpeg/ffmpeg";
import { buildAss, hasOverlays } from "./ass";

/** Browser MP4 export: the main video track's clips are trimmed, scaled into
 *  the canvas, faded and concatenated, then text + captions are burned in
 *  with libass. Same layout rules as the desktop renderer, but encoded at a
 *  lighter size because wasm runs on one CPU thread. */
export interface RenderClip { source: string; source_in_ms: number; duration_ms: number; volume?: number; fade?: { in_ms: number; out_ms: number } | null }
export interface RenderPlan { w: number; h: number; clips: RenderClip[]; ass: string | null; totalMs: number }

/** Web output height cap: 960p keeps a 15 s 9:16 export near a minute. */
export const WEB_MAX_H = 960;

export function webSize(w: number, h: number, maxH = WEB_MAX_H): [number, number] {
  const k = Math.min(1, maxH / h);
  const even = (n: number) => Math.max(2, Math.round((n * k) / 2) * 2);
  return [even(w), even(h)];
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function planRender(p: any, w: number, h: number): RenderPlan {
  const video = p.tracks.find((t: { kind: string }) => t.kind === "video");
  const clips: RenderClip[] = (video?.items ?? [])
    .filter((i: { type: string }) => i.type === "clip")
    .sort((a: { start_ms: number }, b: { start_ms: number }) => a.start_ms - b.start_ms);
  if (!clips.length) throw new Error("Add a clip to the video track first.");
  return { w, h, clips, ass: hasOverlays(p) ? buildAss(p, w, h) : null, totalMs: clips.reduce((n, c) => n + c.duration_ms, 0) };
}

export function clipArgs(c: RenderClip, input: string, out: string, w: number, h: number): string[] {
  const d = c.duration_ms / 1000;
  const fin = (c.fade?.in_ms ?? 0) / 1000, fout = (c.fade?.out_ms ?? 0) / 1000;
  let vf = `scale=${w}:${h}:force_original_aspect_ratio=decrease,pad=${w}:${h}:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps=30,format=yuv420p`;
  let af = `aresample=44100,volume=${c.volume ?? 1}`;
  if (fin > 0) { vf += `,fade=t=in:st=0:d=${fin}`; af += `,afade=t=in:st=0:d=${fin}`; }
  if (fout > 0) { vf += `,fade=t=out:st=${Math.max(0, d - fout)}:d=${fout}`; af += `,afade=t=out:st=${Math.max(0, d - fout)}:d=${fout}`; }
  return ["-ss", String(c.source_in_ms / 1000), "-t", String(d), "-i", input,
    "-f", "lavfi", "-t", String(d), "-i", "anullsrc=r=44100:cl=stereo",
    "-filter_complex", `[0:v]${vf}[v];[0:a]${af}[a0];[1:a][a0]amix=inputs=2:duration=first:normalize=0[a]`,
    "-map", "[v]", "-map", "[a]", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "24", "-c:a", "aac", "-b:a", "128k", "-ac", "2", out];
}

/** Same as clipArgs but for a source without audio (amix needs [0:a]). */
export function silentClipArgs(c: RenderClip, input: string, out: string, w: number, h: number): string[] {
  const a = clipArgs(c, input, out, w, h);
  const i = a.indexOf("-filter_complex");
  a[i + 1] = a[i + 1].replace(/;\[0:a\][^;]*;\[1:a\]\[a0\]amix=[^[]*\[a\]/, ";[1:a]anull[a]");
  return a;
}

export async function renderMp4(
  f: FFmpeg,
  plan: RenderPlan,
  fileFor: (source: string) => File | undefined,
  onProgress?: (ratio: number) => void,
): Promise<Uint8Array> {
  const { fetchFile } = await import("@ffmpeg/util");
  const written = new Map<string, string>();
  let done = 0;
  const steps = plan.totalMs * (plan.ass ? 2 : 1);
  let base = 0, span = 0;
  const handler = ({ progress }: { progress: number }) => onProgress?.(Math.min(0.99, (base + span * Math.min(1, Math.max(0, progress))) / steps));
  f.on("progress", handler);
  const segs: string[] = [];
  try {
    for (const [i, c] of plan.clips.entries()) {
      let input = written.get(c.source);
      if (!input) {
        const file = fileFor(c.source);
        if (!file) throw new Error("A clip's file isn't loaded in this tab. Import it again.");
        input = `src${written.size}.${file.name.split(".").pop() || "mp4"}`;
        await f.writeFile(input, await fetchFile(file));
        written.set(c.source, input);
      }
      const out = `seg${i}.mp4`;
      base = done; span = c.duration_ms;
      let code = await f.exec(clipArgs(c, input, out, plan.w, plan.h));
      if (code !== 0) code = await f.exec(silentClipArgs(c, input, out, plan.w, plan.h));
      if (code !== 0) throw new Error(`Couldn't encode clip ${i + 1} (ffmpeg exit ${code}).`);
      segs.push(out);
      done += c.duration_ms;
    }
    await f.writeFile("list.txt", segs.map((s) => `file '${s}'`).join("\n"));
    const joined = plan.ass ? "joined.mp4" : "out.mp4";
    if ((await f.exec(["-f", "concat", "-safe", "0", "-i", "list.txt", "-c", "copy", joined])) !== 0) throw new Error("Couldn't join the clips.");
    if (plan.ass) {
      await f.writeFile("overlay.ass", plan.ass);
      base = done; span = plan.totalMs;
      const code = await f.exec(["-i", joined, "-vf", "ass=overlay.ass:fontsdir=/fonts", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "23", "-c:a", "copy", "out.mp4"]);
      if (code !== 0) throw new Error("Couldn't burn in the text and captions.");
    }
    const data = (await f.readFile("out.mp4")) as Uint8Array;
    onProgress?.(1);
    return data;
  } finally {
    f.off("progress", handler);
    for (const n of [...written.values(), ...segs, "list.txt", "joined.mp4", "overlay.ass", "out.mp4"]) await f.deleteFile(n).catch(() => undefined);
  }
}
