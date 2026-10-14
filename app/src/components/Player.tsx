import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { formatTime } from "@mediasmith/ui";
import type { Platform } from "@mediasmith/platform";
import type { Editor } from "../store/store";
import { clipAt } from "../store/model";

/** Plays the main video track: shows the clip under the playhead at the
 *  right source time, and drives the playhead from a clock while playing. */
export function Player(props: { editor: Editor; platform: Platform; aspect: string }) {
  const e = props.editor;
  const [playing, setPlaying] = createSignal(false);
  let video!: HTMLVideoElement;
  let raf = 0;
  let last = 0;

  const current = () => clipAt(e.project(), e.playhead());

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
      <div class="canvas" style={{ "aspect-ratio": props.aspect }}>
        <video ref={video} class="frame-video" playsinline />
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
