import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { getPlatform, type ExportPreset } from "@mediasmith/platform";
import { PRESETS, TOOLS, type Tool } from "./mock";
import { createEditor } from "./store/store";
import { findItem, fromJson, TEMPLATES, textsAt, toJson } from "./store/model";
import { actionFor, isTypingTarget, SHORTCUTS } from "./lib/shortcuts";
import { clearSnapshot, debounce, readSnapshot, writeSnapshot, type Snapshot } from "./lib/autosave";
import { Timeline } from "./components/Timeline";
import { Player } from "./components/Player";
import { Settings } from "./components/Settings";
import { CaptionsPanel } from "./components/CaptionsPanel";
import { Inspector } from "./components/Inspector";

const ICONS: Record<Tool, string> = { Edit: "✂", Clip: "▭", GIF: "◎", Captions: "CC", Design: "◆", Import: "⇣", Settings: "⚙" };

export default function App() {
  const platform = getPlatform();
  const editor = createEditor();
  const [tool, setTool] = createSignal<Tool>("Edit");
  const [preset, setPreset] = createSignal<ExportPreset>(platform.caps.exportPresets[0]);
  const [busy, setBusy] = createSignal<string | null>(null);
  const [zoom, setZoom] = createSignal(60);
  const [progress, setProgress] = createSignal<number | null>(null);
  const aspect = () => { const p = PRESETS.find((x) => x.id === preset())!; return `${p.w} / ${p.h}`; };
  const selectedItem = () => { const id = editor.selected(); return id ? findItem(editor.project(), id) : undefined; };

  async function run(label: string, f: () => Promise<string | null | void>) {
    setBusy(label);
    try {
      const r = await f();
      if (typeof r === "string") editor.setMessage(`${label}: ${r}`);
    } catch (err) {
      editor.setMessage(`${label} failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(null);
    }
  }

  const importMedia = () => run("Import", async () => {
    const refs = await platform.importMedia();
    for (const r of refs) editor.importClip(r.source, Math.round(r.meta.duration_secs * 1000));
    return refs.length ? `${refs.length} file(s) added` : null;
  });
  const importOverlay = () => run("Overlay", async () => {
    const [r] = await platform.importMedia();
    if (r) editor.importOverlay(r.source, Math.round(r.meta.duration_secs * 1000));
    return r ? `${r.name} added as picture-in-picture` : null;
  });
  const save = () => run("Saved", async () => {
    const where = await platform.saveProject(toJson(editor.project()), editor.project().name);
    if (where) { editor.markSaved(); clearSnapshot(localStorage); }
    return where;
  });

  // Autosave / crash recovery: snapshot 1.5 s after the last edit while
  // unsaved; offer a pending snapshot back on launch.
  const [recover, setRecover] = createSignal<Snapshot | null>(readSnapshot(localStorage));
  const snap = debounce((json: string, name: string) => writeSnapshot(localStorage, name, json), 1500);
  createEffect(() => {
    const p = editor.project();
    if (editor.dirty() && !recover()) snap(toJson(p), p.name);
  });
  const flush = () => snap.flush();
  window.addEventListener("beforeunload", flush);
  onCleanup(() => window.removeEventListener("beforeunload", flush));
  const [help, setHelp] = createSignal(false);
  const open = () => run("Opened", async () => {
    const json = await platform.openProject();
    if (!json) return null;
    const p = fromJson(json);
    editor.load(p);
    return p.name;
  });
  const exportNow = () => run("Exported", async () => {
    setProgress(0);
    try {
      return await platform.exportProject(toJson(editor.project()), preset(), (r) => setProgress(r));
    } finally {
      setProgress(null);
    }
  });

  const frameMs = () => 1000 / Math.max(1, editor.project().canvas.fps);
  function onKey(ev: KeyboardEvent) {
    if (ev.defaultPrevented || isTypingTarget(ev.target)) return;
    const a = actionFor(ev);
    if (!a) return;
    // Arrow keys nudge a selected on-screen text layer (handled in Player).
    const textSel = textsAt(editor.project(), editor.playhead()).some((t) => t.id === editor.selected());
    if (textSel && (a === "frameBack" || a === "frameFwd" || a === "secBack" || a === "secFwd")) return;
    ev.preventDefault();
    const ph = editor.playhead();
    switch (a) {
      case "play": window.dispatchEvent(new Event("ms:toggle-play")); break;
      case "split": editor.splitAtPlayhead(); break;
      case "delete": editor.deleteSelected(); break;
      case "duplicate": editor.duplicateSelected(); break;
      case "undo": editor.undo(); break;
      case "redo": editor.redo(); break;
      case "save": void save(); break;
      case "open": void open(); break;
      case "export": if (editor.duration() > 0 && !busy()) void exportNow(); break;
      case "frameBack": editor.seek(ph - frameMs()); break;
      case "frameFwd": editor.seek(ph + frameMs()); break;
      case "secBack": editor.seek(ph - 1000); break;
      case "secFwd": editor.seek(ph + 1000); break;
      case "home": editor.seek(0); break;
      case "end": editor.seek(editor.duration()); break;
      case "zoomIn": setZoom((z) => Math.min(400, Math.round(z * 1.25))); break;
      case "zoomOut": setZoom((z) => Math.max(5, Math.round(z / 1.25))); break;
      case "help": setHelp((h) => !h); break;
    }
  }
  onMount(() => window.addEventListener("keydown", onKey));
  onCleanup(() => window.removeEventListener("keydown", onKey));

  return (
    <div class="ms">
      <header class="ms-top">
        <div class="brand"><span class="mark">◆</span><b>Media</b><span class="smith">Smith</span></div>
        <div class="proj">
          <input class="name" value={editor.project().name} onChange={(ev) => editor.rename(ev.currentTarget.value)} />
          <span class="dim">{editor.dirty() ? "· unsaved" : "· saved"}</span>
          <button class="chip" onClick={open}>Open</button>
          <button class="chip" onClick={save}>Save</button>
        </div>
        <div class="presets">
          <For each={PRESETS}>{(p) => (
            <button class={`chip ${preset() === p.id ? "on" : ""}`} disabled={!platform.caps.exportPresets.includes(p.id)}
              title={p.label} onClick={() => setPreset(p.id)}>{p.label.split(" · ")[0]}</button>
          )}</For>
        </div>
        <Show when={recover()}>{(r) => (
          <div class="recover">
            <span>Unsaved work from {new Date(r().at).toLocaleString()}</span>
            <button class="chip" onClick={() => { try { editor.load(fromJson(r().json)); editor.setMessage(`Recovered “${r().name}”`); } catch { editor.setMessage("That autosave couldn't be read"); } setRecover(null); }}>Restore</button>
            <button class="chip" onClick={() => { clearSnapshot(localStorage); setRecover(null); }}>Discard</button>
          </div>
        )}</Show>
        <button class="chip" title="Keyboard shortcuts (?)" onClick={() => setHelp(true)}>⌨</button>
        <span class={`badge ${platform.kind}`}>{platform.kind}</span>
        <Show when={progress() !== null} fallback={
          <button class="export" disabled={!!busy() || editor.duration() === 0} onClick={exportNow}>Export</button>
        }>
          <div class="progress">
            <div class="bar"><i style={{ width: `${Math.round((progress() ?? 0) * 100)}%` }} /></div>
            <span class="tc">{Math.round((progress() ?? 0) * 100)}%</span>
            <button class="chip" onClick={() => void platform.cancelExport()}>Cancel</button>
          </div>
        </Show>
      </header>

      <nav class="ms-tools">
        <For each={TOOLS}>{(t) => (
          <button class={`tool ${tool() === t ? "on" : ""}`} onClick={() => (t === "Import" ? void importMedia() : setTool(t))}>
            <span class="ico">{ICONS[t]}</span><span>{t}</span>
          </button>
        )}</For>
      </nav>

      <Player editor={editor} platform={platform} aspect={aspect()} />

      <aside class="ms-panel">
        <h3>{tool()}</h3>
        <Show when={editor.message()}><p class="note">{editor.message()}</p></Show>
        <Show when={tool() === "Captions"}><CaptionsPanel editor={editor} platform={platform} /></Show>
        <Show when={tool() !== "Captions" && tool() === "Settings"}><Settings platform={platform} /></Show>
        <Show when={tool() === "Design"}>
          <div class="sec">Templates</div>
          <div class="templates">
            <For each={TEMPLATES}>{(t) => (
              <button class="btn2" onClick={() => {
                if (editor.applyTemplate(t.id)) {
                  const p = PRESETS.find((x) => x.w === t.w && x.h === t.h);
                  if (p && platform.caps.exportPresets.includes(p.id)) setPreset(p.id);
                  editor.setMessage(`Template: ${t.label}`);
                }
              }}>{t.label} <span class="dim">{t.w}×{t.h}</span></button>
            )}</For>
          </div>
          <p class="dim">Restyles captions, sets the canvas, and adds the title at the playhead. Undo with Ctrl+Z.</p>
        </Show>
        <Show when={tool() !== "Captions" && tool() !== "Settings" && tool() !== "Design"}>
            <button class="btn2" onClick={importMedia}>⇣ Import video</button>
            <div class="row">
              <button class="btn2" onClick={() => editor.addText("Your title", editor.playhead())}>+ Text</button>
              <button class="btn2" onClick={importOverlay}>+ Overlay (PiP)</button>
            </div>
            <Inspector editor={editor} />
            <Show when={selectedItem()} fallback={<p class="dim">Select a clip on the timeline.</p>}>
              {(it) => (
                <div class="kvs">
                  <div class="kv">Start<b>{(it().start_ms / 1000).toFixed(2)} s</b></div>
                  <div class="kv">Length<b>{(it().duration_ms / 1000).toFixed(2)} s</b></div>
                  <Show when={it().type === "clip"}>
                    <div class="kv">Source in<b>{((it() as { source_in_ms: number }).source_in_ms / 1000).toFixed(2)} s</b></div>
                  </Show>
                </div>
              )}
            </Show>
            <div class="row">
              <button class="btn2" onClick={() => editor.splitAtPlayhead()} title="S">Split</button>
              <button class="btn2" onClick={() => editor.deleteSelected()} title="Delete">Delete</button>
              <button class="btn2" onClick={() => editor.undo()} title="Ctrl+Z">Undo</button>
              <button class="btn2" onClick={() => editor.redo()} title="Ctrl+Shift+Z">Redo</button>
              <button class="btn2" onClick={() => editor.duplicateSelected()} title="Ctrl+D">Duplicate</button>
            </div>
            <label>Zoom<input type="range" min="5" max="400" value={zoom()} onInput={(ev) => setZoom(Number(ev.currentTarget.value))} /></label>
            <Show when={!platform.caps.urlImport}><p class="dim">Link import and MP4 export need the desktop app.</p></Show>
        </Show>
      </aside>

      <Timeline editor={editor} pxPerSec={zoom()} platform={platform} />
      <Show when={help()}>
        <div class="help-overlay" onClick={() => setHelp(false)}>
          <div class="card" onClick={(ev) => ev.stopPropagation()}>
            <h3>Keyboard shortcuts</h3>
            <table><tbody>
              <For each={SHORTCUTS}>{(k) => <tr><td><kbd>{k.keys}</kbd></td><td>{k.label}</td></tr>}</For>
            </tbody></table>
          </div>
        </div>
      </Show>
    </div>
  );
}
