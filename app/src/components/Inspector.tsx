import { For, Show } from "solid-js";
import type { Editor } from "../store/store";
import {
  ANIMATIONS, DEFAULT_TRANSFORM, PIP_PRESETS, POSITION_PRESETS, STYLE_PRESETS, findItem, isOverlayItem, type Item, type TextStyle, type Transform,
} from "../store/model";

type TextItem = Item & { type: "text" | "caption" };

const secs = (ms: number) => (ms / 1000).toFixed(2);
const toMs = (v: string) => Math.round(Number(v) * 1000);

/** Inline editor for the selected item: text/caption content, timing, style
 *  presets and position; overlay clips get picture-in-picture placement. */
export function Inspector(props: { editor: Editor }) {
  const e = props.editor;
  const item = () => { const id = e.selected(); return id ? findItem(e.project(), id) : undefined; };
  const text = () => { const i = item(); return i && (i.type === "text" || i.type === "caption") ? (i as TextItem) : undefined; };
  const overlay = () => { const i = item(); return i && i.type === "clip" && isOverlayItem(e.project(), i.id) ? i : undefined; };

  const style = (patch: Partial<TextStyle>) => { const t = text(); if (t) e.updateText(t.id, { style: patch }); };
  const tf = (patch: Partial<Transform>) => { const o = overlay(); if (o) e.setTransform(o.id, { ...(o.transform ?? DEFAULT_TRANSFORM), ...patch }); };

  return (
    <>
      <Show when={text()}>{(t) => (
        <div class="inspector" data-testid="text-inspector">
          <div class="sec">{t().type === "caption" ? "Caption" : "Text"}</div>
          <textarea rows="2" value={t().text} onChange={(ev) => e.updateText(t().id, { text: ev.currentTarget.value })} />
          <div class="row">
            <label class="field">Start (s)<input type="number" step="0.05" min="0" value={secs(t().start_ms)}
              onChange={(ev) => e.updateText(t().id, { start_ms: toMs(ev.currentTarget.value) })} /></label>
            <label class="field">Length (s)<input type="number" step="0.05" min="0.05" value={secs(t().duration_ms)}
              onChange={(ev) => e.updateText(t().id, { duration_ms: toMs(ev.currentTarget.value) })} /></label>
          </div>
          <div class="sec">Style</div>
          <div class="chips">
            <For each={STYLE_PRESETS}>{(p) => (
              <button class="chip" style={{ color: p.style.color, "-webkit-text-stroke": p.style.stroke ? `0.5px ${p.style.stroke}` : undefined }}
                onClick={() => style(p.style)}>{p.label}</button>
            )}</For>
          </div>
          <div class="row">
            <label class="field">Size<input type="number" min="8" max="400" value={t().style.size_px} onChange={(ev) => style({ size_px: Number(ev.currentTarget.value) })} /></label>
            <label class="field">Colour<input type="color" value={t().style.color} onInput={(ev) => style({ color: ev.currentTarget.value })} /></label>
            <label class="field">Outline<input type="checkbox" checked={!!t().style.stroke} onChange={(ev) => style({ stroke: ev.currentTarget.checked ? "#000000" : null })} /></label>
          </div>
          <div class="sec">Position</div>
          <div class="chips">
            <For each={POSITION_PRESETS}>{(p) => <button class="chip" onClick={() => style({ x: p.x, y: p.y })}>{p.label}</button>}</For>
          </div>
          <label class="field">X<input type="range" min="0" max="1" step="0.01" value={t().style.x} onChange={(ev) => style({ x: Number(ev.currentTarget.value) })} /></label>
          <label class="field">Y<input type="range" min="0" max="1" step="0.01" value={t().style.y} onChange={(ev) => style({ y: Number(ev.currentTarget.value) })} /></label>
          <div class="sec">Animation</div>
          <div class="chips">
            <For each={ANIMATIONS}>{(a) => (
              <button class={`chip ${(t().style.anim ?? "none") === a.id ? "on" : ""}`} onClick={() => style({ anim: a.id })}>{a.label}</button>
            )}</For>
          </div>
        </div>
      )}</Show>
      <Show when={item()}>{(it) => (
        <div class="inspector" data-testid="fade-inspector">
          <div class="sec">Fade (transition)</div>
          <div class="row">
            <label class="field">In (s)<input type="number" step="0.1" min="0" value={secs(it().fade?.in_ms ?? 0)}
              onChange={(ev) => e.setFade(it().id, { in_ms: toMs(ev.currentTarget.value), out_ms: it().fade?.out_ms ?? 0 })} /></label>
            <label class="field">Out (s)<input type="number" step="0.1" min="0" value={secs(it().fade?.out_ms ?? 0)}
              onChange={(ev) => e.setFade(it().id, { in_ms: it().fade?.in_ms ?? 0, out_ms: toMs(ev.currentTarget.value) })} /></label>
          </div>
          <div class="chips">
            <button class="chip" onClick={() => e.setFade(it().id, { in_ms: 500, out_ms: 500 })}>Fade in + out</button>
            <button class="chip" onClick={() => e.setFade(it().id, null)}>No fade</button>
          </div>
        </div>
      )}</Show>
      <Show when={overlay()}>{(o) => {
        const cur = () => o().transform ?? DEFAULT_TRANSFORM;
        return (
          <div class="inspector" data-testid="overlay-inspector">
            <div class="sec">Overlay · picture-in-picture</div>
            <div class="chips">
              <For each={PIP_PRESETS}>{(p) => (
                <button class="chip" onClick={() => tf(p.id === "full" ? { x: 0.5, y: 0.5, scale: 1 } : { x: p.x, y: p.y })}>{p.label}</button>
              )}</For>
            </div>
            <label class="field">Size {Math.round(cur().scale * 100)}%<input type="range" min="0.05" max="1" step="0.01" value={cur().scale} onChange={(ev) => tf({ scale: Number(ev.currentTarget.value) })} /></label>
            <label class="field">X<input type="range" min="0" max="1" step="0.01" value={cur().x} onChange={(ev) => tf({ x: Number(ev.currentTarget.value) })} /></label>
            <label class="field">Y<input type="range" min="0" max="1" step="0.01" value={cur().y} onChange={(ev) => tf({ y: Number(ev.currentTarget.value) })} /></label>
            <label class="field">Opacity<input type="range" min="0" max="1" step="0.05" value={cur().opacity} onChange={(ev) => tf({ opacity: Number(ev.currentTarget.value) })} /></label>
          </div>
        );
      }}</Show>
    </>
  );
}
