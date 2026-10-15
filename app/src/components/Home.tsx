import { For, Show } from "solid-js";
import { TEMPLATES } from "../store/model";
import type { Snapshot } from "../lib/autosave";

const GRADS = ["g1", "g2", "g3", "g4", "g5", "g6"];

/** First screen: what to make, templates, the bundled sample and recovery. */
export function Home(props: {
  onNew: () => void;
  onCaption: () => void;
  onGif: () => void;
  onSample: () => void;
  onTemplate: (id: string) => void;
  onOpen: () => void;
  onDropFiles: (files: File[]) => void;
  recover: Snapshot | null;
  onRecover: () => void;
  linkImport: boolean;
  busy: string | null;
}) {
  const quick = [
    { ic: "＋", t: "New video", s: "Import clips from this computer", hue: "brand", go: () => props.onNew() },
    { ic: "CC", t: "Captions", s: props.linkImport ? "Auto-caption with a local model" : "Type or import .srt / .vtt", hue: "pink", go: () => props.onCaption() },
    { ic: "◎", t: "Make a GIF", s: "Trim any clip into a GIF", hue: "amber", go: () => props.onGif() },
    { ic: "▶", t: "Try the sample", s: "A ready-made 8 s edit", hue: "cyan", go: () => props.onSample() },
  ];
  let over = 0;
  return (
    <div class="home" data-testid="home"
      onDragOver={(e) => { e.preventDefault(); }}
      onDragEnter={(e) => { e.preventDefault(); over++; e.currentTarget.dataset.over = "true"; }}
      onDragLeave={(e) => { if (--over <= 0) { over = 0; e.currentTarget.dataset.over = "false"; } }}
      onDrop={(e) => { e.preventDefault(); over = 0; e.currentTarget.dataset.over = "false"; const f = [...(e.dataTransfer?.files ?? [])].filter((x) => x.type.startsWith("video/")); if (f.length) props.onDropFiles(f); }}>
      <section class="home-hero">
        <h1>What are you making <em>today?</em></h1>
        <p>Start from a template, open the sample, or drop a video anywhere on this page. Everything runs on this device.</p>
      </section>
      <section class="home-quick">
        <For each={quick}>{(q) => (
          <button class={`sm-card qa hue-${q.hue}`} disabled={!!props.busy} onClick={q.go}>
            <span class="qa-ic" aria-hidden="true">{q.ic}</span>
            <span class="qa-t"><b>{q.t}</b><small>{q.s}</small></span>
          </button>
        )}</For>
      </section>
      <Show when={props.recover}>{(r) => (
        <section class="home-recover sm-card">
          <span class="dot" />
          <span class="grow">Unsaved work: <b>{r().name}</b> · {new Date(r().at).toLocaleString()}</span>
          <button class="sm-btn sm-btn--primary sm-btn--sm" onClick={props.onRecover}>Continue editing</button>
        </section>
      )}</Show>
      <div class="home-head"><h2>Templates</h2><button class="sm-btn sm-btn--ghost sm-btn--sm" onClick={props.onOpen}>Open a project…</button></div>
      <section class="home-tpls">
        <For each={TEMPLATES}>{(t, i) => (
          <button class={`tpl ${GRADS[i() % GRADS.length]}`} onClick={() => props.onTemplate(t.id)} title={`${t.label} · ${t.w}×${t.h}`}>
            <i class="tpl-frame" style={{ "aspect-ratio": `${t.w} / ${t.h}` }} aria-hidden="true" />
            <b>{t.label}</b><small>{t.w}×{t.h}</small>
          </button>
        )}</For>
      </section>
      <Show when={props.busy}><p class="home-busy">{props.busy}…</p></Show>
    </div>
  );
}
