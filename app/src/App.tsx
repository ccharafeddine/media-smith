import { createEffect, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { getPlatform, type ExportPreset } from "@mediasmith/platform";
import { EmptyState, Kbd, Segmented, Wordmark } from "@mediasmith/ui";
import sampleUrl from "@mediasmith/ui/sample.mp4?url";
import { PRESETS, TOOL_ICONS, TOOLS, type Tool } from "./mock";
import { createEditor } from "./store/store";
import { findItem, fromJson, STYLE_PRESETS, TEMPLATES, textsAt, toJson } from "./store/model";
import { actionFor, isTypingTarget, SHORTCUTS } from "./lib/shortcuts";
import { clearSnapshot, debounce, readSnapshot, writeSnapshot, type Snapshot } from "./lib/autosave";
import { sampleProject } from "./lib/sample";
import { Timeline } from "./components/Timeline";
import { Player } from "./components/Player";
import { Settings } from "./components/Settings";
import { CaptionsPanel } from "./components/CaptionsPanel";
import { Inspector } from "./components/Inspector";
import { Home } from "./components/Home";
import { ExportDialog } from "./components/ExportDialog";

export default function App() {
  const platform = getPlatform();
  const editor = createEditor();
  const [recover, setRecover] = createSignal<Snapshot | null>(readSnapshot(localStorage));
  const [tool, setTool] = createSignal<Tool>("Home");
  const [preset, setPreset] = createSignal<ExportPreset>("vertical");
  const [busy, setBusy] = createSignal<string | null>(null);
  const [zoom, setZoom] = createSignal(60);
  const [progress, setProgress] = createSignal<number | null>(null);
  const [exporting, setExporting] = createSignal(false);
  const [help, setHelp] = createSignal(false);
  const home = () => tool() === "Home";
  const aspect = () => `${editor.project().canvas.width} / ${editor.project().canvas.height}`;
  const selectedItem = () => { const id = editor.selected(); return id ? findItem(editor.project(), id) : undefined; };
  const hasText = () => editor.project().tracks.some((t) => (t.kind === "text" || t.kind === "caption") && t.items.length > 0);
  const clips = () => editor.project().tracks.filter((t) => t.kind === "video").flatMap((t) => t.items).filter((i) => i.type === "clip");

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

  /** Canvas follows the chosen aspect preset (GIF keeps the current canvas). */
  function pickPreset(id: ExportPreset) {
    setPreset(id);
    const p = PRESETS.find((x) => x.id === id)!;
    if (id !== "gif") {
      const pr = editor.project();
      if (pr.canvas.width !== p.w || pr.canvas.height !== p.h) editor.replace({ ...pr, canvas: { ...pr.canvas, width: p.w, height: p.h } });
    }
  }
  createEffect(() => {
    const c = editor.project().canvas;
    const m = PRESETS.find((x) => x.w === c.width && x.h === c.height && x.id !== "gif");
    if (m && preset() !== "gif" && preset() !== m.id) setPreset(m.id);
  });

  const importMedia = () => run("Import", async () => {
    const refs = await platform.importMedia();
    for (const r of refs) editor.importClip(r.source, Math.round(r.meta.duration_secs * 1000));
    if (refs.length && home()) setTool("Media");
    return refs.length ? `${refs.length} file(s) added` : null;
  });
  const importFiles = (files: File[]) => run("Import", async () => {
    for (const f of files) { const r = await platform.importFile(f); editor.importClip(r.source, Math.round(r.meta.duration_secs * 1000)); }
    setTool("Media");
    return `${files.length} file(s) added`;
  });
  const importOverlay = () => run("Overlay", async () => {
    const [r] = await platform.importMedia();
    if (r) editor.importOverlay(r.source, Math.round(r.meta.duration_secs * 1000));
    return r ? `${r.name} added as picture-in-picture` : null;
  });
  const loadSample = () => run("Sample", async () => {
    const blob = await (await fetch(sampleUrl)).blob();
    const ref = await platform.importFile(new File([blob], "sunset-sample.mp4", { type: "video/mp4" }));
    editor.load(sampleProject(ref.source, ref.meta.duration_secs * 1000));
    setPreset("vertical");
    setTool("Text");
    editor.seek(2600);
    return "loaded the demo edit";
  });
  const save = () => run("Saved", async () => {
    const where = await platform.saveProject(toJson(editor.project()), editor.project().name);
    if (where) { editor.markSaved(); clearSnapshot(localStorage); }
    return where;
  });
  const open = () => run("Opened", async () => {
    const json = await platform.openProject();
    if (!json) return null;
    const p = fromJson(json);
    editor.load(p);
    setTool("Media");
    return p.name;
  });
  const exportNow = async () => {
    setProgress(0);
    try {
      return await platform.exportProject(toJson(editor.project()), preset(), (r) => setProgress(r));
    } finally {
      setProgress(null);
    }
  };

  // Autosave / crash recovery.
  const snap = debounce((json: string, name: string) => writeSnapshot(localStorage, name, json), 1500);
  createEffect(() => {
    const p = editor.project();
    if (editor.dirty() && !recover()) snap(toJson(p), p.name);
  });
  const flush = () => snap.flush();
  window.addEventListener("beforeunload", flush);
  onCleanup(() => window.removeEventListener("beforeunload", flush));
  const doRecover = () => {
    const r = recover();
    if (!r) return;
    try { editor.load(fromJson(r.json)); editor.setMessage(`Recovered “${r.name}”`); setTool("Media"); }
    catch { editor.setMessage("That autosave couldn't be read"); }
    setRecover(null);
  };

  const frameMs = () => 1000 / Math.max(1, editor.project().canvas.fps);
  function onKey(ev: KeyboardEvent) {
    if (ev.defaultPrevented || isTypingTarget(ev.target) || home()) return;
    const a = actionFor(ev);
    if (!a) return;
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
      case "export": if (editor.duration() > 0) setExporting(true); break;
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
  const fit = () => {
    const w = (document.querySelector(".ms-timeline") as HTMLElement | null)?.clientWidth ?? 1200;
    setZoom(Math.max(5, Math.min(400, Math.floor(((w - 140) / Math.max(1, editor.duration() / 1000))))));
  };

  const applyTemplate = (id: string) => {
    const t = TEMPLATES.find((x) => x.id === id);
    if (editor.applyTemplate(id) && t) editor.setMessage(`Template: ${t.label}`);
  };

  return (
    <div class={`ms sm-app ${home() ? "is-home" : ""}`}>
      <header class="sm-topbar ms-top">
        <button class="brand-btn" title="Home" onClick={() => setTool("Home")}><Wordmark app="mediasmith" /></button>
        <span class="vsep" />
        <Show when={!home()} fallback={<span class="dim">Video editor · runs on this device</span>}>
          <div class="proj">
            <input class="name" aria-label="Project name" value={editor.project().name} onChange={(ev) => editor.rename(ev.currentTarget.value)} />
            <span class="sm-chip"><i class={`dot ${editor.dirty() ? "warn" : "ok"}`} />{editor.dirty() ? "Unsaved" : "Saved"}</span>
          </div>
        </Show>
        <span class="grow" />
        <Show when={!home()}>
          <button class="sm-btn sm-btn--ghost sm-btn--icon" title="Undo (Ctrl+Z)" onClick={() => editor.undo()}>↶</button>
          <button class="sm-btn sm-btn--ghost sm-btn--icon" title="Redo (Ctrl+Shift+Z)" onClick={() => editor.redo()}>↷</button>
          <Segmented label="Canvas" value={preset() === "gif" ? "vertical" : preset()} onChange={pickPreset}
            options={PRESETS.filter((p) => p.id !== "gif").map((p) => ({ value: p.id, label: p.short }))} />
          <button class="sm-btn sm-btn--ghost" onClick={open}>Open</button>
          <button class="sm-btn sm-btn--ghost" onClick={save}>Save</button>
        </Show>
        <button class="sm-btn sm-btn--ghost sm-btn--icon" title="Keyboard shortcuts (?)" onClick={() => setHelp(true)}>?</button>
        <Show when={!home()} fallback={<button class="sm-btn sm-btn--primary" onClick={importMedia}>＋ New video</button>}>
          <button class="sm-btn sm-btn--primary" disabled={editor.duration() === 0} onClick={() => setExporting(true)}>⇪ Export</button>
        </Show>
      </header>

      <nav class="ms-rail" aria-label="Tools">
        <For each={TOOLS}>{(t) => (
          <button class={`rail-btn ${tool() === t ? "on" : ""}`} aria-pressed={tool() === t} onClick={() => setTool(t)}>
            <i aria-hidden="true">{TOOL_ICONS[t]}</i><span>{t}</span>
          </button>
        )}</For>
      </nav>

      <Show when={!home()} fallback={
        <Home recover={recover()} onRecover={doRecover} busy={busy()} linkImport={platform.caps.urlImport}
          onNew={importMedia} onSample={loadSample} onOpen={open} onDropFiles={importFiles}
          onCaption={() => setTool("Captions")} onGif={() => { setPreset("gif"); setTool("Media"); }}
          onTemplate={(id) => { applyTemplate(id); setTool("Design"); }} />
      }>
        <aside class="ms-side">
          <div class="sm-panel__head">{tool()}</div>
          <div class="side-body">
            <Show when={editor.message()}><p class="note">{editor.message()}</p></Show>
            <Switch>
              <Match when={tool() === "Media"}>
                <button class="sm-btn sm-btn--lg wide" onClick={importMedia}>⇣ Import video</button>
                <button class="sm-btn wide" onClick={importOverlay}>＋ Overlay (picture-in-picture)</button>
                <div class="sm-drop drop-mini" onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => { e.preventDefault(); const f = [...(e.dataTransfer?.files ?? [])].filter((x) => x.type.startsWith("video/")); if (f.length) void importFiles(f); }}>
                  Drop videos here
                </div>
                <div class="lbl">In this project</div>
                <For each={clips()} fallback={<p class="dim small">No clips yet. Import a file, or try the sample from Home.</p>}>{(c) => (
                  <button class={`clip-row ${editor.selected() === c.id ? "on" : ""}`} onClick={() => { editor.select(c.id); editor.seek(c.start_ms); }}>
                    <span class="clip-ic">▶</span><span class="grow ellip">{c.type === "clip" ? platform.sourceName(c.source) : ""}</span><span class="sm-num dim">{(c.duration_ms / 1000).toFixed(1)}s</span>
                  </button>
                )}</For>
                <Show when={!platform.caps.urlImport}><p class="dim small">Link import is optional and lives in the desktop app. Local files always work here.</p></Show>
              </Match>
              <Match when={tool() === "Text"}>
                <button class="sm-btn sm-btn--lg wide left" onClick={() => editor.addText("YOUR TITLE", editor.playhead())}>＋ Add a heading</button>
                <button class="sm-btn wide left" onClick={() => { const id = editor.addText("Body text", editor.playhead()); if (id) editor.updateText(id, { style: { size_px: 56, y: 0.5 } }); }}>＋ Add body text</button>
                <div class="lbl">Styles · click to apply to the selected text</div>
                <div class="style-grid">
                  <For each={STYLE_PRESETS}>{(p, i) => (
                    <button class={`tpl g${(i() % 6) + 1}`} onClick={() => { const s = selectedItem(); if (s && (s.type === "text" || s.type === "caption")) editor.updateText(s.id, { style: p.style }); else editor.setMessage("Select a text layer first"); }}>
                      <b style={{ color: p.style.color, "-webkit-text-stroke": p.style.stroke ? `1px ${p.style.stroke}` : undefined }}>{p.label}</b>
                    </button>
                  )}</For>
                </div>
              </Match>
              <Match when={tool() === "Captions"}><CaptionsPanel editor={editor} platform={platform} /></Match>
              <Match when={tool() === "Design"}>
                <div class="lbl">Templates · canvas, caption style and title in one step</div>
                <div class="style-grid">
                  <For each={TEMPLATES}>{(t, i) => (
                    <button class={`tpl g${(i() % 6) + 1}`} onClick={() => applyTemplate(t.id)}><b>{t.label}</b><small>{t.w}×{t.h}</small></button>
                  )}</For>
                </div>
                <p class="dim small">Undo with Ctrl+Z.</p>
              </Match>
              <Match when={tool() === "Settings"}><Settings platform={platform} /></Match>
            </Switch>
          </div>
        </aside>

        <Player editor={editor} platform={platform} aspect={aspect()} />

        <aside class="ms-inspector" aria-label="Inspector">
          <Show when={selectedItem()} fallback={
            <EmptyState glyph="◫" title="Nothing selected" body={editor.duration() === 0 ? "Import a clip or open the sample to start." : "Click a clip, a text layer or a caption to edit it here."} />
          }>{(it) => (
            <>
              <div class="sm-panel__head">{it().type === "clip" ? "Clip" : it().type === "text" ? "Text" : "Caption"}<span class="sm-chip sm-chip--hue">Selected</span></div>
              <div class="insp-body">
                <Inspector editor={editor} />
                <div class="kvs">
                  <div class="kv">Start<b class="sm-num">{(it().start_ms / 1000).toFixed(2)} s</b></div>
                  <div class="kv">Length<b class="sm-num">{(it().duration_ms / 1000).toFixed(2)} s</b></div>
                </div>
              </div>
            </>
          )}</Show>
        </aside>

        <section class="ms-tl">
          <div class="tlbar">
            <button class="sm-btn sm-btn--sm" title="Split at playhead (S)" onClick={() => editor.splitAtPlayhead()}>✂ Split</button>
            <button class="sm-btn sm-btn--sm sm-btn--ghost" title="Duplicate (Ctrl+D)" onClick={() => editor.duplicateSelected()}>⧉ Duplicate</button>
            <button class="sm-btn sm-btn--sm sm-btn--ghost" title="Delete" onClick={() => editor.deleteSelected()}>Delete</button>
            <span class="grow" />
            <span class="lbl">Zoom</span>
            <input class="sm-range zoom" type="range" min="5" max="400" value={zoom()} aria-label="Timeline zoom" onInput={(ev) => setZoom(Number(ev.currentTarget.value))} />
            <button class="sm-btn sm-btn--sm sm-btn--ghost" onClick={fit}>Fit</button>
          </div>
          <Timeline editor={editor} pxPerSec={zoom()} platform={platform} />
        </section>
      </Show>

      <Show when={exporting()}>
        <ExportDialog platform={platform} name={editor.project().name} durationMs={editor.duration()} preset={preset()} onPreset={setPreset}
          hasText={hasText()} progress={progress()} onExport={exportNow} onClose={() => setExporting(false)} />
      </Show>
      <Show when={help()}>
        <div class="sm-backdrop" onClick={() => setHelp(false)}>
          <div class="sm-dialog help-dlg" onClick={(ev) => ev.stopPropagation()}>
            <h2>Keyboard shortcuts</h2>
            <table><tbody>
              <For each={SHORTCUTS}>{(k) => <tr><td><Kbd>{k.keys}</Kbd></td><td>{k.label}</td></tr>}</For>
            </tbody></table>
          </div>
        </div>
      </Show>
    </div>
  );
}
