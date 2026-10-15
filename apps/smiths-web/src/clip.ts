import { $, baseName, exportJob, fmt, run, segmented, shell, stage, withEngine } from "./common";
import { cropFilter } from "./subs";

shell("clipsmith", ["any length and resolution", "native-speed encoding", "link import (optional)"]);

const start = $<HTMLInputElement>("#start"), end = $<HTMLInputElement>("#end");
const ratio = segmented("ratio"), fmtSel = segmented("fmt", (v) => ($("#go").textContent = v === "mp3" ? "Export audio" : "Export clip"));
const s = stage((_f, v) => { start.value = "0"; end.value = v.duration.toFixed(1); update(); });
const update = () => { const d = Number(end.value) - Number(start.value); $("#len").textContent = d > 0 ? `Clip length ${fmt(d)}` : "End must be after start."; };
start.oninput = end.oninput = update;
$("#setIn").onclick = () => { start.value = s.video.currentTime.toFixed(1); update(); };
$("#setOut").onclick = () => { end.value = s.video.currentTime.toFixed(1); update(); };

const go = $<HTMLButtonElement>("#go");
go.onclick = () => {
  const file = s.file();
  const a = Number(start.value), d = Number(end.value) - a;
  if (!file || !(d > 0)) return;
  const kind = fmtSel(), r = Number(ratio()) || null;
  const crop = cropFilter(r);
  const vf = (more: string) => [crop, more].filter(Boolean).join(",");
  const name = `${baseName(file.name)}-clip.${kind}`;
  void exportJob(s, go, (p) => withEngine(p, async (f, enc) => {
    const seek = ["-ss", a.toFixed(3), "-t", d.toFixed(3)];
    const args = (i: string) =>
      kind === "mp3" ? [...seek, "-i", i, "-vn", "-c:a", "libmp3lame", "-q:a", "2", "out.mp3"]
      : kind === "gif" ? [...seek, "-i", i, "-vf", vf("fps=15,scale='min(480,iw)':-2:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4"), "-loop", "0", "out.gif"]
      : [...seek, "-i", i, "-vf", vf("scale=-2:'min(720,trunc(ih/2)*2)'"), "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", "out.mp4"];
    const bytes = await run(f, file, args, `out.${kind}`, enc);
    return { bytes, name, type: kind === "mp3" ? "audio/mpeg" : kind === "gif" ? "image/gif" : "video/mp4" };
  }), (url, type) => type.startsWith("audio") ? `<audio controls src="${url}"></audio>` : type === "image/gif" ? `<img src="${url}" alt="Exported GIF" />` : `<video controls src="${url}"></video>`);
};
