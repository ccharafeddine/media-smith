import { $, baseName, download, exportJob, fmt, run, segmented, shell, stage, withEngine } from "./common";
import { parseSubs, toAss, toSrt, toVtt, type CapStyle, type Cue } from "./subs";

shell("captionsmith", ["automatic transcription with a local Whisper model", "speaker labels", "batch captioning", "link import (optional)"]);

// libass Fontsize is the full line height; CSS font-size is the em box. This
// factor makes the live preview match the burned-in result.
const ASS_EM = 0.72;

let cues: Cue[] = [];
const SAMPLE: Cue[] = [
  { start: 0.3, end: 2.6, text: "Golden hour, made in the browser" },
  { start: 2.8, end: 5.2, text: "Nothing was uploaded" },
  { start: 5.4, end: 7.8, text: "Style it, then burn it in" },
];
const pos = segmented("pos", () => paint());
const style = (): CapStyle => ({
  font: $<HTMLSelectElement>("#font").value as CapStyle["font"],
  size: Number($<HTMLSelectElement>("#size").value),
  color: $<HTMLInputElement>("#color").value,
  box: $<HTMLSelectElement>("#box").value === "1",
  position: pos() as CapStyle["position"],
});
const s = stage((f) => { if (f.name === "sunset-sample.mp4" && !cues.length) { cues = SAMPLE.map((c) => ({ ...c })); renderList(); } paint(); });
const overlay = $("#overlay");

/** Live preview: the same style the burn-in uses, scaled to the player. */
function paint() {
  const st = style(), v = s.video, t = v.currentTime;
  const cue = cues.find((c) => t >= c.start && t < c.end);
  // Fit the overlay to the letterboxed picture, not the whole player.
  const box = v.getBoundingClientRect(), ar = (v.videoWidth || 16) / (v.videoHeight || 9);
  const pw = Math.min(box.width, box.height * ar), h = pw / ar || 1;
  Object.assign(overlay.style, { left: `${(box.width - pw) / 2}px`, right: `${(box.width - pw) / 2}px`, top: `${(box.height - h) / 2}px`, bottom: `${(box.height - h) / 2}px` });
  overlay.dataset.pos = st.position;
  overlay.innerHTML = cue ? `<span>${cue.text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!).replace(/\n/g, "<br>")}</span>` : "";
  const span = overlay.firstElementChild as HTMLElement | null;
  if (span) {
    Object.assign(span.style, { fontFamily: st.font === "Syne" ? "Syne" : "Inter", fontSize: `${(h * st.size * ASS_EM) / 100}px`, color: st.color });
    Object.assign(span.style, st.box
      ? { background: "rgb(0 0 0 / 0.62)", padding: "0.12em 0.4em", borderRadius: "6px", textShadow: "none" }
      : { background: "none", padding: "0", textShadow: "0 0 3px #000, 0 0 2px #000" });
  }
}
s.video.addEventListener("timeupdate", paint);
s.video.addEventListener("seeked", paint);
window.addEventListener("resize", paint);
for (const id of ["#font", "#size", "#color", "#box"]) $(id).addEventListener("input", paint);

function renderList() {
  $("#count").textContent = cues.length ? String(cues.length) : "";
  const ol = $("#cues");
  ol.innerHTML = cues.length ? "" : `<li class="empty">No captions yet.</li>`;
  cues.forEach((c, i) => {
    const li = document.createElement("li");
    li.innerHTML = `<button type="button" class="t sm-num" title="Jump here">${fmt(c.start)}</button><textarea class="sm-input" rows="1" aria-label="Caption ${i + 1}"></textarea><button type="button" class="sm-btn sm-btn--ghost sm-btn--sm x" aria-label="Delete caption">✕</button>`;
    const ta = li.querySelector("textarea")!;
    ta.value = c.text;
    ta.oninput = () => { c.text = ta.value; paint(); };
    (li.querySelector(".t") as HTMLElement).onclick = () => { s.video.currentTime = c.start + 0.01; };
    (li.querySelector(".x") as HTMLElement).onclick = () => { cues.splice(i, 1); renderList(); paint(); };
    ol.append(li);
  });
}
renderList();

const subsFile = $<HTMLInputElement>("#subsFile");
$("#importSubs").onclick = () => subsFile.click();
subsFile.onchange = async () => {
  const f = subsFile.files?.[0];
  if (!f) return;
  const parsed = parseSubs(await f.text());
  if (!parsed.length) return s.st.set(`No captions found in ${f.name}.`, undefined, "err");
  cues = parsed; renderList(); paint();
  s.st.set(`Imported ${parsed.length} captions from ${f.name}.`, undefined, "ok");
  subsFile.value = "";
};
$("#addCue").onclick = () => {
  const t = s.video.currentTime;
  cues.push({ start: t, end: Math.min(t + 2, s.video.duration || t + 2), text: "New caption" });
  cues.sort((a, b) => a.start - b.start);
  renderList(); paint();
};

const name = () => baseName(s.file()?.name ?? "captions");
$("#srt").onclick = () => cues.length && download(toSrt(cues), `${name()}.srt`, "application/x-subrip");
$("#vtt").onclick = () => cues.length && download(toVtt(cues), `${name()}.vtt`, "text/vtt");

const go = $<HTMLButtonElement>("#go");
go.onclick = () => {
  const file = s.file();
  if (!file) return;
  if (!cues.length) return s.st.set("Add or import some captions first.", undefined, "err");
  // Burn at the output size (≤720p) so caption size matches the preview.
  const vw = s.video.videoWidth, vh = s.video.videoHeight;
  const oh = Math.min(720, vh - (vh % 2)), ow = Math.round((vw * oh) / vh / 2) * 2;
  const ass = new TextEncoder().encode(toAss(cues, style(), ow, oh));
  void exportJob(s, go, (p) => withEngine(p, async (f, enc) => {
    const bytes = await run(f, file, (i) => ["-i", i, "-vf", `scale=${ow}:${oh},ass=subs.ass:fontsdir=/fonts`, "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", "out.mp4"], "out.mp4", enc, { "subs.ass": ass });
    return { bytes, name: `${name()}-captioned.mp4`, type: "video/mp4" };
  }), (url) => `<video controls src="${url}"></video>`);
};
