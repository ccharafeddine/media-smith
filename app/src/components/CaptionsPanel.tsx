import { createSignal, For, Show } from "solid-js";
import { formatTime } from "@mediasmith/ui";
import type { Platform } from "@mediasmith/platform";
import type { Editor } from "../store/store";
import { captions, fromJson, toJson } from "../store/model";
import { missingModel, type CaptionModel } from "@mediasmith/platform";
import { Inspector } from "./Inspector";

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
      setProgress({ label: "Transcribing", ratio: 0 });
      const json = await cap.autoCaption(toJson(e.project()), model(), chosen()?.multilingual ? language() : "en",
        (r) => setProgress({ label: "Transcribing", ratio: r }));
      e.replace(fromJson(json));
      e.setMessage(`Auto-caption: ${captions(e.project()).length} captions`);
    } catch (err) {
      const m = missingModel(err);
      e.setMessage(m ? `Download ${m} first` : `Auto-caption failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally { setProgress(null); }
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
      <div class="sec">Auto-caption</div>
      <Show when={cap.available} fallback={<p class="dim">Auto-captions need the desktop app.</p>}>
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
        <Show when={chosen() && !chosen()!.installed}><p class="dim">The model downloads once and is checked against its SHA-256.</p></Show>
      </Show>
      <button class="btn2" onClick={() => e.addCaption("New caption", e.playhead())}>+ Caption at playhead</button>
      <div class="cap-list">
        <For each={list()} fallback={<p class="dim">No captions yet.</p>}>{(c) => (
          <div class={`cap-row ${preview()?.[c.id] && preview()![c.id] !== c.text ? "changed" : ""}`} onClick={() => { e.select(c.id); e.seek(c.start_ms); }}>
            <time>{formatTime(c.start_ms)}</time>
            <span>{preview()?.[c.id] ?? c.text}</span>
          </div>
        )}</For>
      </div>
      <Inspector editor={e} />
      <div class="sec">AI rewrite</div>
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
      <Show when={!props.platform.ai.available}><p class="dim">AI rewrite needs the desktop app.</p></Show>
    </>
  );
}
