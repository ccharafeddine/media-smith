import { $, baseName, exportJob, run, segmented, shell, stage, withEngine } from "./common";
import { cropFilter } from "./subs";

shell("gifsmith", ["longer GIFs", "frame-accurate trim handles", "gallery of past exports", "link import (optional)"]);

const start = $<HTMLInputElement>("#start"), dur = $<HTMLInputElement>("#dur");
const ratio = segmented("ratio"), loop = segmented("loop");
const s = stage((_f, v) => { start.value = "0"; dur.value = Math.min(3, v.duration).toFixed(1); });
$("#setIn").onclick = () => { start.value = s.video.currentTime.toFixed(1); };

const go = $<HTMLButtonElement>("#go");
go.onclick = () => {
  const file = s.file();
  if (!file) return;
  const a = Number(start.value), d = Math.min(15, Math.max(0.5, Number(dur.value)));
  const w = Number($<HTMLSelectElement>("#width").value), fps = Number($<HTMLSelectElement>("#fps").value);
  const head = [cropFilter(Number(ratio()) || null), `fps=${fps}`, `scale=${w}:-2:flags=lanczos`].filter(Boolean).join(",");
  const boom = loop() === "boomerang";
  const graph = boom
    ? `[0:v]${head},split[f][r];[r]reverse[rv];[f][rv]concat=n=2:v=1,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4`
    : `[0:v]${head},split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4`;
  void exportJob(s, go, (p) => withEngine(p, async (f, enc) => {
    const bytes = await run(f, file, (i) => ["-ss", a.toFixed(3), "-t", d.toFixed(3), "-i", i, "-filter_complex", graph, "-loop", "0", "out.gif"], "out.gif", enc);
    return { bytes, name: `${baseName(file.name)}${boom ? "-boomerang" : ""}.gif`, type: "image/gif" };
  }), (url) => `<img src="${url}" alt="Your GIF" />`);
};
