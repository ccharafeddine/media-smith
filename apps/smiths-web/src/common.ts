import "@mediasmith/ui/fonts.css";
import "@mediasmith/ui/tokens.css";
import "@mediasmith/ui/components.css";
import "./style.css";
import { APPS, applyIdentity, type SmithApp } from "@mediasmith/ui/apps";
import { configureFfmpeg, loadFfmpeg } from "@mediasmith/platform";
import type { FFmpeg } from "@ffmpeg/ffmpeg";
import sampleUrl from "@mediasmith/ui/sample.mp4";

const REPOS: Partial<Record<SmithApp, string>> = { clipsmith: "ClipSmith", gifsmith: "GifSmith", captionsmith: "CaptionSmith" };

export const MAX_INPUT = 500 * 1024 * 1024;
export const $ = <T extends HTMLElement = HTMLElement>(sel: string, root: ParentNode = document) => root.querySelector(sel) as T;

/** Topbar + identity shared by every demo page. */
export function shell(app: SmithApp, desktopOnly: string[]): void {
  applyIdentity(app);
  configureFfmpeg(import.meta.env.BASE_URL);
  const info = APPS[app];
  const bar = document.createElement("header");
  bar.className = "sm-topbar";
  bar.innerHTML = `
    <a class="sm-wordmark" href="${import.meta.env.BASE_URL}${import.meta.env.BASE_URL === "/" ? "" : "index.html"}" aria-label="${info.prefix}Smith">
      <img src="${import.meta.env.BASE_URL}${app}.svg" alt="" width="26" height="26" /><span><span class="wm-app">${info.prefix}</span><span class="wm-smith">Smith</span></span>
    </a>
    <span class="vsep"></span><span class="tag">Web demo</span>
    <span class="grow"></span>
    ${REPOS[app] ? `<a class="sm-btn sm-btn--sm sm-btn--ghost" href="https://github.com/ccharafeddine/${REPOS[app]}/releases/latest" target="_blank" rel="noopener">Get the desktop app</a>` : ""}
    <span class="sm-chip" title="Files are processed in this tab with ffmpeg.wasm. Nothing is uploaded."><i class="dot"></i> Runs in your browser</span>`;
  document.body.prepend(bar);
  const foot = document.createElement("footer");
  foot.className = "foot";
  foot.innerHTML = `Everything happens on this device: no uploads, no server. ${desktopOnly.length ? `Desktop app adds: ${desktopOnly.join(" · ")}.` : ""}`;
  document.body.append(foot);
}

/** Wire a file input + drop target. Rejects files over the in-browser cap. */
export function picker(drop: HTMLElement, input: HTMLInputElement, onFile: (f: File) => void, onError: (msg: string) => void): void {
  const take = (f?: File) => {
    if (!f) return;
    if (f.size > MAX_INPUT) return onError("That file is over 500 MB, which is too big for the browser. Trim it first, or use the desktop app.");
    onFile(f);
  };
  input.addEventListener("change", () => take(input.files?.[0]));
  drop.addEventListener("click", (e) => { if (!(e.target as HTMLElement).closest("input,button,a")) input.click(); });
  for (const t of [drop, document.body]) {
    t.addEventListener("dragover", (e) => { e.preventDefault(); drop.dataset.over = "true"; });
    t.addEventListener("dragleave", () => { drop.dataset.over = "false"; });
    t.addEventListener("drop", (e) => { e.preventDefault(); drop.dataset.over = "false"; take(e.dataTransfer?.files[0]); });
  }
}

/** Load the engine (one-time ~31 MB, then cached) with a progress callback
 *  that covers both the download (first 25%) and the encode (rest). */
export async function engine(onProgress: (ratio: number, label: string) => void): Promise<FFmpeg> {
  onProgress(0, "Loading the video engine (one time, ~31 MB)…");
  const f = await loadFfmpeg((r) => onProgress(r * 0.25, "Loading the video engine (one time, ~31 MB)…"));
  return f;
}

/** Run one ffmpeg job on `file`, return the output bytes. */
export async function run(f: FFmpeg, file: File, args: (input: string) => string[], out: string, onProgress: (ratio: number) => void, extra: Record<string, string | Uint8Array> = {}): Promise<Uint8Array> {
  const ext = (file.name.split(".").pop() || "mp4").toLowerCase().replace(/[^a-z0-9]/g, "");
  const input = `in.${ext}`;
  await f.writeFile(input, new Uint8Array(await file.arrayBuffer()));
  for (const [k, v] of Object.entries(extra)) await f.writeFile(k, v);
  const onP = ({ progress }: { progress: number }) => onProgress(Math.max(0, Math.min(1, progress)));
  f.on("progress", onP);
  try {
    const code = await f.exec(args(input));
    if (code !== 0) throw new Error("ffmpeg couldn't process this file (exit " + code + ").");
    const data = await f.readFile(out);
    return data as Uint8Array;
  } finally {
    f.off("progress", onP);
    for (const n of [input, out, ...Object.keys(extra)]) await f.deleteFile(n).catch(() => undefined);
  }
}

export function download(bytes: Uint8Array | string, name: string, type: string): { url: string; size: number } {
  const blob = new Blob([bytes as BlobPart], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = name; a.click();
  return { url, size: blob.size };
}

export const mb = (n: number) => (n / 1e6).toFixed(n < 1e7 ? 2 : 1) + " MB";
export const fmt = (s: number) => { const m = Math.floor(s / 60); const r = s - m * 60; return `${m}:${r.toFixed(1).padStart(4, "0")}`; };
export const baseName = (n: string) => n.replace(/\.[^.]+$/, "") || "video";

/** Status line + progress bar helper. */
export function status(el: HTMLElement) {
  const text = $(".st-text", el), bar = $<HTMLElement>(".sm-progress > i", el);
  return {
    set(msg: string, ratio?: number, kind: "" | "ok" | "err" = "") {
      el.hidden = false; el.dataset.kind = kind; text.textContent = msg;
      $<HTMLElement>(".sm-progress", el).hidden = ratio === undefined;
      if (ratio !== undefined) bar.style.width = `${Math.round(ratio * 100)}%`;
    },
    hide() { el.hidden = true; },
  };
}


export interface Stage { file: () => File | null; video: HTMLVideoElement; st: ReturnType<typeof status>; result: HTMLElement }

/** Empty state → player + panel. Calls `onLoad` once metadata is known. */
export function stage(onLoad: (file: File, video: HTMLVideoElement) => void): Stage {
  let current: File | null = null;
  const video = $<HTMLVideoElement>("#video");
  const st = status($("#status"));
  const show = (f: File) => {
    current = f;
    $("#drop").hidden = true;
    $("#player").hidden = false;
    $("#panel").hidden = false;
    $("#result").hidden = true;
    st.hide();
    video.src = URL.createObjectURL(f);
    video.onloadedmetadata = () => {
      const meta = $("#meta");
      meta.hidden = false;
      meta.textContent = `${f.name} · ${video.videoWidth}×${video.videoHeight} · ${fmt(video.duration)} · ${mb(f.size)}`;
      onLoad(f, video);
    };
  };
  picker($("#drop"), $<HTMLInputElement>("#file"), show, (m) => { $("#panel").hidden = false; st.set(m, undefined, "err"); });
  $("#sample").addEventListener("click", async (e) => {
    e.preventDefault(); e.stopPropagation();
    const blob = await (await fetch(sampleUrl)).blob();
    show(new File([blob], "sunset-sample.mp4", { type: "video/mp4" }));
  });
  return { file: () => current, video, st, result: $("#result") };
}

/** Segmented control: returns a getter for the pressed value. */
export function segmented(id: string, onChange?: (v: string) => void): () => string {
  const el = $(`#${id}`);
  el.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement).closest("button");
    if (!b) return;
    el.querySelectorAll("button").forEach((x) => x.setAttribute("aria-pressed", String(x === b)));
    onChange?.(b.dataset.v!);
  });
  return () => $<HTMLButtonElement>('button[aria-pressed="true"]', el).dataset.v!;
}

/** Run an export with the standard status/result UI. */
export async function exportJob(s: Stage, go: HTMLButtonElement, job: (progress: (r: number, label?: string) => void) => Promise<{ bytes: Uint8Array; name: string; type: string }>, preview: (url: string, type: string) => string): Promise<void> {
  go.disabled = true;
  s.result.hidden = true;
  const t0 = performance.now();
  try {
    const out = await job((r, label) => s.st.set(label ?? `Encoding… ${Math.round(r * 100)}%`, r));
    const { url, size } = download(out.bytes, out.name, out.type);
    s.st.set(`Done in ${((performance.now() - t0) / 1000).toFixed(1)} s · ${mb(size)}`, undefined, "ok");
    s.result.hidden = false;
    s.result.innerHTML = `${preview(url, out.type)}<a class="sm-btn sm-btn--sm" href="${url}" download="${out.name}">Download ${out.name}</a>`;
  } catch (e) {
    s.st.set(`Export failed: ${e instanceof Error ? e.message : String(e)}`, undefined, "err");
  } finally {
    go.disabled = false;
  }
}

/** Engine + encode progress mapped onto one bar. */
export async function withEngine<T>(progress: (r: number, label?: string) => void, body: (f: FFmpeg, enc: (r: number) => void) => Promise<T>): Promise<T> {
  const f = await engine((r, l) => progress(r, l));
  return body(f, (r) => progress(0.25 + r * 0.75));
}
