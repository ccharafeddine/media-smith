import "@mediasmith/ui/fonts.css";
import "@mediasmith/ui/tokens.css";
import "./style.css";
import coreURL from "@ffmpeg/core?url";
import wasmURL from "@ffmpeg/core/wasm?url";
import { loadFfmpeg, webPlatform } from "@mediasmith/platform";
import { formatTime } from "@mediasmith/ui";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const fileInput = $<HTMLInputElement>("file");
const go = $<HTMLButtonElement>("go");
const prog = $<HTMLProgressElement>("prog");
const status = $<HTMLParagraphElement>("status");
const preview = $<HTMLVideoElement>("preview");
let file: File | null = null;

function pick(f: File | undefined) {
  if (!f) return;
  if (f.size > webPlatform.caps.maxInputBytes) { status.textContent = "That file is over 500 MB; trim it first."; return; }
  file = f;
  $("dropText").textContent = `${f.name} · ${(f.size / 1e6).toFixed(1)} MB`;
  preview.src = URL.createObjectURL(f);
  preview.hidden = false;
  go.disabled = false;
}

fileInput.addEventListener("change", () => pick(fileInput.files?.[0]));
const drop = $("drop");
drop.addEventListener("dragover", (e) => { e.preventDefault(); drop.classList.add("over"); });
drop.addEventListener("dragleave", () => drop.classList.remove("over"));
drop.addEventListener("drop", (e) => { e.preventDefault(); drop.classList.remove("over"); pick(e.dataTransfer?.files[0]); });
preview.addEventListener("timeupdate", () => { status.textContent = `Preview at ${formatTime(preview.currentTime * 1000)}`; });

go.addEventListener("click", async () => {
  if (!file) return;
  go.disabled = true;
  prog.hidden = false;
  prog.value = 0;
  const t0 = performance.now();
  try {
    status.textContent = "Loading ffmpeg.wasm (~30 MB, cached after first run)…";
    await loadFfmpeg(coreURL, wasmURL);
    status.textContent = "Encoding…";
    const blob = (await webPlatform.toGif(file, {
      startSec: Number($<HTMLInputElement>("start").value),
      durationSec: Math.min(15, Number($<HTMLInputElement>("dur").value)),
      width: Number($<HTMLSelectElement>("width").value),
      fps: Number($<HTMLSelectElement>("fps").value),
    }, (r) => { prog.value = r; })) as Blob;
    const url = URL.createObjectURL(blob);
    $<HTMLImageElement>("gif").src = url;
    $<HTMLAnchorElement>("dl").href = url;
    $("size").textContent = `${(blob.size / 1e6).toFixed(2)} MB`;
    $("out").hidden = false;
    status.textContent = `Done in ${((performance.now() - t0) / 1000).toFixed(1)} s.`;
  } catch (e) {
    status.textContent = `Failed: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    go.disabled = false;
    prog.hidden = true;
  }
});
