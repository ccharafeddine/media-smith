import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { formatTime } from "@mediasmith/ui";
import type { Platform } from "@mediasmith/platform";
import type { Editor } from "../store/store";
import { clipAt, overlaysAt, textsAt } from "../store/model";

/** Plays the main video track: shows the clip under the playhead at the
 *  right source time, and drives the playhead from a clock while playing. */
export function Player(props: { editor: Editor; platform: Platform; aspect: string }) {
  const e = props.editor;
  const [playing, setPlaying] = createSignal(false);
  let video!: HTMLVideoElement;
  let raf = 0;
  let last = 0;

  const current = () => clipAt(e.project(), e.playhead());
  const overlays = () => overlaysAt(e.project(), e.playhead());
  const texts = () => textsAt(e.project(), e.playhead());
  let canvas!: HTMLDivElement;
  /** Canvas px per project px, so text sizes match the export. */
  const [k, setK] = createSignal(0.3);
  const ro = new ResizeObserver(() => canvas && setK(canvas.clientHeight / Math.max(1, e.project().canvas.height)));
  createEffect(() => canvas && ro.observe(canvas));
  onCleanup(() => ro.disconnect());

  createEffect(() => {
    const c = current();
    if (!video) return;
    if (!c) { video.removeAttribute("src"); return; }
    const url = props.platform.mediaUrl(c.item.source);
    if (video.dataset.src !== url) { video.dataset.src = url; video.src = url; }
    const want = c.sourceMs / 1000;
    if (!playing() || Math.abs(video.currentTime - want) > 0.25) video.currentTime = want;
    video.volume = Math.min(1, c.item.volume);
  });

  function tick(now: number) {
    const dt = now - last;
    last = now;
    const next = e.playhead() + dt;
    if (next >= e.duration()) { e.seek(e.duration()); return stop(); }
    e.seek(next);
    raf = requestAnimationFrame(tick);
  }
  function play() {
    if (e.duration() === 0) return;
    if (e.playhead() >= e.duration()) e.seek(0);
    setPlaying(true);
    void video.play().catch(() => undefined);
    last = performance.now();
    raf = requestAnimationFrame(tick);
  }
  function stop() { setPlaying(false); cancelAnimationFrame(raf); video?.pause(); }
  onCleanup(stop);

  return (
    <main class="ms-stage">
      <div class="canvas" ref={canvas} style={{ "aspect-ratio": props.aspect }}>
        <video ref={video} class="frame-video" playsinline />
        <For each={overlays()}>{(o) => (
          <OverlayVideo url={props.platform.mediaUrl(o.item.source)} sourceMs={o.sourceMs} playing={playing()}
            style={{ left: `${o.transform.x * 100}%`, top: `${o.transform.y * 100}%`, width: `${o.transform.scale * 100}%`, opacity: o.transform.opacity }} />
        )}</For>
        <For each={texts()}>{(t) => (
          <div class={`text-layer ${e.selected() === t.id ? "sel" : ""}`} onClick={() => e.select(t.id)}
            style={{
              left: `${t.style.x * 100}%`, top: `${t.style.y * 100}%`, "font-family": `${t.style.font}, sans-serif`,
              "font-size": `${t.style.size_px * k()}px`, color: t.style.color,
              "-webkit-text-stroke": t.style.stroke ? `${Math.max(1, 3 * k())}px ${t.style.stroke}` : undefined,
            }}>{t.text}</div>
        )}</For>
        <Show when={!current()}><div class="empty">{e.duration() === 0 ? "Import a video to start" : "—"}</div></Show>
      </div>
      <div class="transport">
        <span class="tc">{formatTime(e.playhead())}</span>
        <button class="play" onClick={() => (playing() ? stop() : play())}>{playing() ? "❚❚" : "▶"}</button>
        <span class="tc dim">{formatTime(e.duration())}</span>
      </div>
    </main>
  );
}

/** A muted overlay clip, kept in sync with the playhead. */
function OverlayVideo(props: { url: string; sourceMs: number; playing: boolean; style: Record<string, string | number> }) {
  let v!: HTMLVideoElement;
  createEffect(() => {
    if (!v) return;
    if (v.dataset.src !== props.url) { v.dataset.src = props.url; v.src = props.url; }
    const want = props.sourceMs / 1000;
    if (!props.playing || Math.abs(v.currentTime - want) > 0.25) v.currentTime = want;
    if (props.playing) void v.play().catch(() => undefined); else v.pause();
  });
  return <video ref={v} class="overlay-video" muted playsinline style={props.style} />;
}
