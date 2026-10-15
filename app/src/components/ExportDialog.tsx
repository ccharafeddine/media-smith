import { createSignal, For, Show } from "solid-js";
import { webSize, type ExportPreset, type Platform } from "@mediasmith/platform";
import { formatTime } from "@mediasmith/ui";
import { PRESETS } from "../mock";

/** Export: pick a format, see what will be rendered and where, then watch
 *  progress. The dialog stays open until the file is written or cancelled. */
export function ExportDialog(props: {
  platform: Platform;
  name: string;
  durationMs: number;
  preset: ExportPreset;
  onPreset: (p: ExportPreset) => void;
  hasText: boolean;
  onExport: () => Promise<string | null | void>;
  progress: number | null;
  onClose: () => void;
}) {
  const [done, setDone] = createSignal<string | null>(null);
  const [err, setErr] = createSignal<string | null>(null);
  const p = () => PRESETS.find((x) => x.id === props.preset)!;
  const allowed = (id: ExportPreset) => props.platform.caps.exportPresets.includes(id);
  const isGif = () => props.preset === "gif";
  const ext = () => (isGif() ? "gif" : "mp4");
  const outSize = () => { const [w, h] = props.platform.kind === "web" ? webSize(p().w, p().h) : [p().w, p().h]; return `${w}×${h}`; };
  const go = async () => {
    setErr(null); setDone(null);
    try { const r = await props.onExport(); if (typeof r === "string") setDone(r); } catch (e) { setErr(e instanceof Error ? e.message : String(e)); }
  };
  return (
    <div class="sm-backdrop" onClick={() => props.progress === null && props.onClose()}>
      <div class="sm-dialog export-dlg" role="dialog" aria-label="Export" onClick={(e) => e.stopPropagation()}>
        <div class="row"><h2>Export</h2><span class="grow" /><button class="sm-btn sm-btn--ghost sm-btn--icon" title="Close" disabled={props.progress !== null} onClick={props.onClose}>✕</button></div>
        <div class="sm-field"><span>Format</span>
          <div class="pills">
            <For each={PRESETS}>{(x) => (
              <button class={`pill ${props.preset === x.id ? "on" : ""}`} disabled={!allowed(x.id) || props.progress !== null} onClick={() => props.onPreset(x.id)}>
                <b>{x.label}</b><small>{x.id === "gif" ? "480 px · 15 fps" : `${x.w}×${x.h} · ${x.short}`}</small>
              </button>
            )}</For>
          </div>
        </div>
        <div class="export-sum">
          <span><b class="sm-num">{formatTime(props.durationMs)}</b> · {ext().toUpperCase()}{isGif() ? " (first clip, up to 15 s)" : ` · ${outSize()}`}</span>
          <span>{props.hasText && !isGif() ? "Text and captions burned in ✓" : isGif() ? "Video only" : "No text layers"}</span>
        </div>
        <p class="dim small">{props.platform.caps.exportNote} File: {props.name}-{props.preset}.{ext()}</p>
        <Show when={props.progress !== null}>
          <div class="sm-field"><span>Exporting… {Math.round((props.progress ?? 0) * 100)}%</span><div class="sm-progress"><i style={{ width: `${Math.round((props.progress ?? 0) * 100)}%` }} /></div></div>
        </Show>
        <Show when={done()}>{(d) => <div class="sm-toast sm-toast--ok">✓ Saved {d()}</div>}</Show>
        <Show when={err()}>{(e) => <div class="sm-toast sm-toast--err">{e()}</div>}</Show>
        <div class="row end">
          <Show when={props.progress !== null} fallback={<button class="sm-btn" onClick={props.onClose}>{done() ? "Done" : "Cancel"}</button>}>
            <button class="sm-btn" onClick={() => void props.platform.cancelExport()}>Stop</button>
          </Show>
          <button class="sm-btn sm-btn--primary sm-btn--lg" disabled={props.progress !== null || props.durationMs === 0} onClick={go}>⇪ Export {ext().toUpperCase()}</button>
        </div>
      </div>
    </div>
  );
}
