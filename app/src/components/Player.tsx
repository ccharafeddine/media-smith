import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { formatTime } from "@mediasmith/ui";
import type { Platform } from "@mediasmith/platform";
import type { Editor } from "../store/store";
import { clipAt, dragPosition, fadeOpacity, karaokeWords, overlaysAt, textsAt } from "../store/model";
import { isTypingTarget } from "../lib/shortcuts";

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

  /** Live drag of a text layer: previewed locally, committed on pointer-up
   *  as one undoable edit. Alt disables snapping; Esc cancels. */
  const [drag, setDrag] = createSignal<{ id: string; x: number; y: number; snapX: boolean; snapY: boolean } | null>(null);
  function startDrag(ev: PointerEvent, id: string, x0: number, y0: number) {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.stopPropagation();
    e.select(id);
    const sx = ev.clientX, sy = ev.clientY;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    let moved = false;
    const onMove = (m: PointerEvent) => {
      const dx = m.clientX - sx, dy = m.clientY - sy;
      if (!moved && Math.hypot(dx, dy) < 3) return; // a click, not a drag
      moved = true;
      setDrag({ id, ...dragPosition({ x: x0, y: y0 }, dx, dy, w, h, m.altKey) });
    };
    const end = (commit: boolean) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("keydown", onKey);
      const d = drag();
      setDrag(null);
      if (commit && moved && d && (d.x !== x0 || d.y !== y0)) e.moveText(id, d.x, d.y);
    };
    const onUp = () => end(true);
    const onKey = (k: KeyboardEvent) => k.key === "Escape" && end(false);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey);
  }
  const pos = (t: { id: string; style: { x: number; y: number } }) => {
    const d = drag();
    return d && d.id === t.id ? d : t.style;
  };

  /** Nudge the selected text layer with arrow keys (Shift = 10x). */
  function onKeyDown(ev: KeyboardEvent) {
    const t = texts().find((x) => x.id === e.selected());
    if (!t || !ev.key.startsWith("Arrow") || ev.ctrlKey || ev.metaKey) return;
    if (isTypingTarget(ev.target)) return;
    ev.preventDefault();
    const px = ev.shiftKey ? 10 : 1;
    const dx = ev.key === "ArrowLeft" ? -px : ev.key === "ArrowRight" ? px : 0;
    const dy = ev.key === "ArrowUp" ? -px : ev.key === "ArrowDown" ? px : 0;
    const d = dragPosition(t.style, dx, dy, canvas.clientWidth, canvas.clientHeight, true);
    e.moveText(t.id, d.x, d.y);
  }
  window.addEventListener("keydown", onKeyDown);
  onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  // App-level shortcuts (Space) drive playback through this event.
  const toggle = () => (playing() ? stop() : play());
  window.addEventListener("ms:toggle-play", toggle);
  onCleanup(() => window.removeEventListener("ms:toggle-play", toggle));

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
        <video ref={video} class="frame-video" playsinline style={{ opacity: current() ? fadeOpacity(current()!.item, e.playhead()) : 1 }} />
        <For each={overlays()}>{(o) => (
          <OverlayVideo url={props.platform.mediaUrl(o.item.source)} sourceMs={o.sourceMs} playing={playing()}
            style={{ left: `${o.transform.x * 100}%`, top: `${o.transform.y * 100}%`, width: `${o.transform.scale * 100}%`, opacity: o.transform.opacity * fadeOpacity(o.item, e.playhead()) }} />
        )}</For>
        <Show when={drag()?.snapX}><i class="snap-guide v" /></Show>
        <Show when={drag()?.snapY}><i class="snap-guide h" /></Show>
        <For each={texts()}>{(t) => (
          <div class={`text-layer anim-${t.style.anim ?? "none"} ${e.selected() === t.id ? "sel" : ""} ${drag()?.id === t.id ? "dragging" : ""}`}
            onPointerDown={(ev) => startDrag(ev, t.id, t.style.x, t.style.y)}
            title="Drag to move (Alt: no snapping, arrows to nudge)"
            style={{
              left: `${pos(t).x * 100}%`, top: `${pos(t).y * 100}%`, "font-family": `${t.style.font}, sans-serif`,
              "font-size": `${t.style.size_px * k()}px`, color: t.style.color, opacity: fadeOpacity(t, e.playhead()),
              "-webkit-text-stroke": t.style.stroke ? `${Math.max(1, 3 * k())}px ${t.style.stroke}` : undefined,
            }}>
            <Show when={t.style.highlight ? karaokeWords(t, e.playhead()) : null} fallback={t.text}>{(ws) => (
              <For each={ws()}>{(w, i) => <>{i() > 0 ? " " : ""}<span style={{ color: w.lit ? t.style.highlight! : undefined }}>{w.text}</span></>}</For>
            )}</Show>
          </div>
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
