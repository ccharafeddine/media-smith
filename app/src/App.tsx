import { createSignal, For, Show } from "solid-js";
import { formatTime, timeToX } from "@mediasmith/ui";
import { getPlatform } from "@mediasmith/platform";
import { DURATION, PRESETS, TOOLS, TRACKS, type Tool } from "./mock";

const ICONS: Record<Tool, string> = { Edit: "✂", Clip: "▭", GIF: "◎", Captions: "CC", Design: "◆", Import: "⇣" };
const PX_PER_SEC = 90;

export default function App() {
  const platform = getPlatform();
  const [tool, setTool] = createSignal<Tool>("Edit");
  const [preset, setPreset] = createSignal(PRESETS[0].id);
  const [playhead] = createSignal(3400);
  const [selected, setSelected] = createSignal("v1");
  const aspect = () => { const p = PRESETS.find((x) => x.id === preset())!; return `${p.w} / ${p.h}`; };

  return (
    <div class="ms">
      <header class="ms-top">
        <div class="brand"><span class="mark">◆</span><b>Media</b><span class="smith">Smith</span></div>
        <div class="proj">Untitled project <span class="dim">· autosaved</span></div>
        <div class="presets">
          <For each={PRESETS}>{(p) => (
            <button class={`chip ${preset() === p.id ? "on" : ""}`} onClick={() => setPreset(p.id)}>{p.label}</button>
          )}</For>
        </div>
        <span class={`badge ${platform.kind}`}>{platform.kind}</span>
        <button class="export">Export</button>
      </header>

      <nav class="ms-tools">
        <For each={TOOLS}>{(t) => (
          <button class={`tool ${tool() === t ? "on" : ""}`} onClick={() => setTool(t)}>
            <span class="ico">{ICONS[t]}</span><span>{t}</span>
          </button>
        )}</For>
      </nav>

      <main class="ms-stage">
        <div class="canvas" style={{ "aspect-ratio": aspect() }}>
          <div class="frame" />
          <Show when={tool() === "Captions" || tool() === "Edit"}><div class="cap">this is the part</div></Show>
        </div>
        <div class="transport">
          <span class="tc">{formatTime(playhead())}</span>
          <button class="play">▶</button>
          <span class="tc dim">{formatTime(DURATION)}</span>
        </div>
      </main>

      <aside class="ms-panel">
        <h3>{tool()}</h3>
        <Show when={tool() === "Import" && !platform.caps.urlImport}>
          <p class="note">Link import needs the desktop app. In the browser, drop a local file.</p>
        </Show>
        <label>Selected<input value={selected()} readOnly /></label>
        <label>Speed<input type="range" min="25" max="400" value="100" /></label>
        <label>Volume<input type="range" min="0" max="200" value="100" /></label>
        <label>Opacity<input type="range" min="0" max="100" value="100" /></label>
      </aside>

      <section class="ms-timeline">
        <div class="ruler">
          <For each={Array.from({ length: DURATION / 1000 + 1 }, (_, i) => i)}>{(s) => (
            <span style={{ left: `${timeToX(s * 1000, PX_PER_SEC)}px` }}>{formatTime(s * 1000).replace(/\.\d$/, "")}</span>
          )}</For>
        </div>
        <For each={TRACKS}>{(t) => (
          <div class={`track ${t.kind}`}>
            <div class="head">{t.name}</div>
            <div class="lane">
              <For each={t.items}>{(it) => (
                <div class={`item ${selected() === it.id ? "sel" : ""}`} onClick={() => setSelected(it.id)}
                  style={{ left: `${timeToX(it.start, PX_PER_SEC)}px`, width: `${timeToX(it.dur, PX_PER_SEC)}px` }}>{it.label}</div>
              )}</For>
            </div>
          </div>
        )}</For>
        <div class="playhead" style={{ left: `calc(112px + ${timeToX(playhead(), PX_PER_SEC)}px)` }} />
      </section>
    </div>
  );
}
