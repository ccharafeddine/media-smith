import { createSignal, For, Show } from "solid-js";
import { formatTime } from "@mediasmith/ui";
import type { Platform } from "@mediasmith/platform";
import type { Editor } from "../store/store";
import { captions, fromJson, speakerColor, speakers, toJson } from "../store/model";
import { missingDiarization, missingModel, type CaptionModel } from "@mediasmith/platform";
import { parseSubtitles, toSrt, toVtt } from "../lib/subtitles";

const PRESETS = ["Fix spelling and punctuation", "Make it punchier and shorter", "Add a few fitting emoji", "Translate to Spanish"];

/** Captions tool: list, add, and AI rewrite (text only; timing kept). The
 *  rewrite is previewed as a diff and applied in one undoable edit. */
export function CaptionsPanel(props: { editor: Editor; platform: Platform }) {
  const e = props.editor;
  const [instruction, setInstruction] = createSignal(PRESETS[0]);
  const [preview, setPreview] = createSignal<Record<string, string> | null>(null);
  const [busy, setBusy] = createSignal(false);
  const list = () => captions(e.project());
  const [models, setModels] = createSignal<CaptionModel[]>([]);
  const [model, setModel] = createSignal<string>("ggml-base.en.bin");
  const [language, setLanguage] = createSignal("en");
  const [progress, setProgress] = createSignal<{ label: string; ratio: number | null } | null>(null);
  const cap = props.platform.captions;
  const refresh = () => cap.available && void cap.models().then(setModels).catch(() => undefined);
  refresh();
  const chosen = () => models().find((m) => m.file === model());
  const [detectSpeakers, setDetectSpeakers] = createSignal(false);
  const [diar, setDiar] = createSignal<{ models: boolean; engine: boolean }>({ models: false, engine: false });
  const refreshDiar = () => cap.available && void cap.diarizationStatus().then(setDiar).catch(() => undefined);
  refreshDiar();
  const karaokeOn = () => list().some((c) => !!c.style.highlight);
  const hasWords = () => list().some((c) => c.words?.length);
  const spk = () => speakers(e.project());

  async function downloadSpeakers() {
    setProgress({ label: "Downloading speaker models", ratio: 0 });
    try {
      await cap.downloadDiarization((r, label) => setProgress({ label: `Downloading ${label}`, ratio: r }));
      refreshDiar();
    } finally { setProgress(null); }
  }

  async function download(file: string) {
    setProgress({ label: "Downloading model", ratio: 0 });
    try {
      await cap.downloadModel(file, (r) => setProgress({ label: "Downloading model", ratio: r }));
      refresh();
    } finally { setProgress(null); }
  }

  async function autoCaption() {
    if (e.duration() === 0) return e.setMessage("Import a video first");
    if (list().length && !confirm("Replace the existing captions?")) return;
    try {
      if (chosen() && !chosen()!.installed) await download(model());
      if (detectSpeakers() && !diar().models) await downloadSpeakers();
      setProgress({ label: detectSpeakers() ? "Transcribing + detecting speakers" : "Transcribing", ratio: 0 });
      const json = await cap.autoCaption(toJson(e.project()), model(), chosen()?.multilingual ? language() : "en",
        (r) => setProgress({ label: "Transcribing", ratio: r }), detectSpeakers());
      e.replace(fromJson(json));
      const n = speakers(e.project()).length;
      e.setMessage(`Auto-caption: ${captions(e.project()).length} captions${n ? `, ${n} speaker${n > 1 ? "s" : ""}` : ""}`);
    } catch (err) {
      const m = missingModel(err);
      e.setMessage(m ? `Download ${m} first` : missingDiarization(err) ? "Download the speaker models first"
        : `Auto-caption failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally { setProgress(null); }
  }

  /** Offline captions: read an .srt/.vtt the user already has. */
  function importSubs() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".srt,.vtt,text/vtt,application/x-subrip";
    input.onchange = async () => {
      const f = input.files?.[0];
      if (!f) return;
      const cues = parseSubtitles(await f.text());
      if (!cues.length) return e.setMessage(`No captions found in ${f.name}`);
      if (list().length && !confirm("Replace the existing captions?")) return;
      if (e.importCues(cues)) e.setMessage(`Imported ${cues.length} captions from ${f.name}`);
    };
    input.click();
  }
  function exportSubs(kind: "srt" | "vtt") {
    const cues = list().map((c) => ({ start_ms: c.start_ms, end_ms: c.start_ms + c.duration_ms, text: c.text }));
    if (!cues.length) return e.setMessage("No captions to export");
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([kind === "srt" ? toSrt(cues) : toVtt(cues)], { type: "text/plain" }));
    a.download = `${e.project().name || "captions"}.${kind}`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
  }
  async function importModelFile(kind: "whisper" | "speakers") {
    try {
      const r = kind === "whisper" ? await cap.importModel() : await cap.importDiarization();
      if (r) { e.setMessage(`Installed ${r}`); refresh(); refreshDiar(); }
    } catch (err) { e.setMessage(`Import failed: ${err instanceof Error ? err.message : String(err)}`); }
  }

  async function rewrite() {
    const items = list();
    if (!items.length) return e.setMessage("Add captions first");
    setBusy(true);
    try {
      const out = await props.platform.ai.rewriteCaptions(items.map((i) => i.text), instruction());
      setPreview(Object.fromEntries(items.map((i, k) => [i.id, out[k]])));
    } catch (err) {
      e.setMessage(`Rewrite failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally { setBusy(false); }
  }

  return (
    <>
      <div class="sec">Add captions</div>
      <div class="row">
        <button class="btn2" onClick={() => e.addCaption("New caption", e.playhead())}>＋ At playhead</button>
        <button class="btn2" onClick={importSubs}>⇣ Import .srt / .vtt</button>
      </div>
      <div class="sec">Auto-caption · local speech model</div>
      <Show when={cap.available} fallback={<p class="dim">Auto-captions run in the desktop app with a local whisper model. Here, type captions or import a subtitle file.</p>}>
        <select value={model()} onChange={(ev) => setModel(ev.currentTarget.value)}>
          <For each={models()}>{(m) => <option value={m.file}>{m.label} · {m.size_mb} MB{m.installed ? " ✓" : ""}</option>}</For>
        </select>
        <Show when={chosen()?.multilingual}>
          <input placeholder="Language (en, es, auto…)" value={language()} onChange={(ev) => setLanguage(ev.currentTarget.value.trim() || "auto")} />
        </Show>
        <Show when={progress()} fallback={
          <button class="btn2 primary" onClick={autoCaption}>🎙 Auto-caption</button>
        }>{(p) => (
          <div class="progress">
            <span class="dim">{p().label}</span>
            <div class="bar"><i style={{ width: `${Math.round((p().ratio ?? 0) * 100)}%` }} /></div>
            <button class="chip" onClick={() => void (p().label.startsWith("Down") ? cap.cancelDownload() : cap.cancel())}>Cancel</button>
          </div>
        )}</Show>
        <Show when={chosen() && !chosen()!.installed}><p class="dim">Not installed yet. Download it once (SHA-256 checked), or import the file if you already have it.</p></Show>
        <div class="row">
          <button class="btn2" title="Install a ggml-*.bin you already have (works offline)" onClick={() => void importModelFile("whisper")}>Import model file…</button>
          <button class="btn2" title="Install a speaker model file you already have (works offline)" onClick={() => void importModelFile("speakers")}>Import speaker model…</button>
        </div>
        <label class="row" title={diar().engine ? "" : "The speaker-detection engine isn't bundled in this build"}>
          <input type="checkbox" checked={detectSpeakers()} disabled={!diar().engine} onChange={(ev) => setDetectSpeakers(ev.currentTarget.checked)} />
          Detect speakers{diar().engine ? (diar().models ? "" : " (46 MB download)") : " (engine not bundled)"}
        </label>
      </Show>
      <Show when={list().length}>
        <div class="sec">Caption style</div>
        <label class="row" title={hasWords() ? "" : "Needs word timings: run Auto-caption"}>
          <input type="checkbox" checked={karaokeOn()} disabled={!hasWords()}
            onChange={(ev) => e.styleAllCaptions({ highlight: ev.currentTarget.checked ? "#ffd400" : null })} />
          Karaoke word highlight
        </label>
        <Show when={karaokeOn()}>
          <label class="row">Highlight <input type="color" value={list().find((c) => c.style.highlight)?.style.highlight ?? "#ffd400"}
            onChange={(ev) => e.styleAllCaptions({ highlight: ev.currentTarget.value })} /></label>
        </Show>
        <Show when={spk().length}>
          <div class="row">
            <For each={spk()}>{(i) => <span class="chip"><i class="spk" style={{ background: speakerColor(i) }} />Speaker {i + 1}</span>}</For>
          </div>
          <button class="btn2" onClick={() => e.colorBySpeaker()}>Colour captions by speaker</button>
        </Show>
      </Show>
      <div class="row">
        <span class="sec grow">Captions · {list().length}</span>
        <button class="chip" disabled={!list().length} onClick={() => exportSubs("srt")}>.srt</button>
        <button class="chip" disabled={!list().length} onClick={() => exportSubs("vtt")}>.vtt</button>
      </div>
      <div class="cap-list">
        <For each={list()} fallback={<p class="dim">No captions yet.</p>}>{(c) => (
          <div class={`cap-row ${preview()?.[c.id] && preview()![c.id] !== c.text ? "changed" : ""}`} onClick={() => { e.select(c.id); e.seek(c.start_ms); }}>
            <time>{formatTime(c.start_ms)}</time>
            <Show when={c.speaker != null}>
              <select class="spk-pick" value={c.speaker!} onClick={(ev) => ev.stopPropagation()}
                onChange={(ev) => e.setSpeaker(c.id, Number(ev.currentTarget.value))} title="Speaker">
                <For each={[...new Set([...spk(), spk().length])]}>{(i) => <option value={i}>S{i + 1}</option>}</For>
              </select>
            </Show>
            <span>{preview()?.[c.id] ?? c.text}</span>
          </div>
        )}</For>
      </div>
      <div class="sec">AI rewrite · optional</div>
      <select onChange={(ev) => setInstruction(ev.currentTarget.value)}>
        <For each={PRESETS}>{(p) => <option>{p}</option>}</For>
      </select>
      <textarea rows="2" value={instruction()} onInput={(ev) => setInstruction(ev.currentTarget.value)} />
      <Show when={preview()} fallback={
        <button class="btn2" disabled={busy() || !props.platform.ai.available} onClick={rewrite}>{busy() ? "Rewriting…" : "✨ Rewrite captions"}</button>
      }>
        <div class="row">
          <button class="btn2" onClick={() => { e.setTexts(preview()!); setPreview(null); }}>Apply</button>
          <button class="btn2" onClick={() => setPreview(null)}>Discard</button>
        </div>
      </Show>
      <p class="dim small">{props.platform.ai.available ? "Uses the provider in Settings. Pick Ollama for a fully local model." : "Optional, desktop only (local Ollama or your own key). Editing never needs it."}</p>
    </>
  );
}
