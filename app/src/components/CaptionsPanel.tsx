import { createSignal, For, Show } from "solid-js";
import { formatTime } from "@mediasmith/ui";
import type { Platform } from "@mediasmith/platform";
import type { Editor } from "../store/store";
import { captions } from "../store/model";

const PRESETS = ["Fix spelling and punctuation", "Make it punchier and shorter", "Add a few fitting emoji", "Translate to Spanish"];

/** Captions tool: list, add, and AI rewrite (text only; timing kept). The
 *  rewrite is previewed as a diff and applied in one undoable edit. */
export function CaptionsPanel(props: { editor: Editor; platform: Platform }) {
  const e = props.editor;
  const [instruction, setInstruction] = createSignal(PRESETS[0]);
  const [preview, setPreview] = createSignal<Record<string, string> | null>(null);
  const [busy, setBusy] = createSignal(false);
  const list = () => captions(e.project());

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
      <button class="btn2" onClick={() => e.addCaption("New caption", e.playhead())}>+ Caption at playhead</button>
      <div class="cap-list">
        <For each={list()} fallback={<p class="dim">No captions yet.</p>}>{(c) => (
          <div class={`cap-row ${preview()?.[c.id] && preview()![c.id] !== c.text ? "changed" : ""}`} onClick={() => { e.select(c.id); e.seek(c.start_ms); }}>
            <time>{formatTime(c.start_ms)}</time>
            <span>{preview()?.[c.id] ?? c.text}</span>
          </div>
        )}</For>
      </div>
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
