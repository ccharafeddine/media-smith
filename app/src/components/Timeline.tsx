import { createResource, For, Show } from "solid-js";
import type { Platform } from "@mediasmith/platform";
import { snap, snapMove, snapTargets, snapThresholdMs } from "../store/snap";
import { formatTime, timeToX } from "@mediasmith/ui";
import type { Editor } from "../store/store";
import type { Item } from "../store/model";

const HEAD = 112;

function label(it: Item): string {
  if (it.type === "clip") return it.source.split(/[\\/]/).pop() ?? it.source;
  return it.text;
}

/** Real timeline: click to seek, drag a block to move, drag its edges to
 *  trim. Edits commit on pointer-up through the store (invalid ones bounce). */
const stripCache = new Map<string, Promise<string | null>>();

/** ClipSmith-style filmstrip behind a video clip (desktop only). */
function Filmstrip(props: { platform: Platform; item: Item; pxPerSec: number }) {
  const key = () => {
    const it = props.item;
    if (it.type !== "clip") return null;
    const count = Math.max(1, Math.min(40, Math.round(timeToX(it.duration_ms, props.pxPerSec) / 46)));
    return { source: it.source, in: it.source_in_ms, dur: it.duration_ms, count };
  };
  const [src] = createResource(key, (k) => {
    const id = `${k.source}|${k.in}|${k.dur}|${k.count}`;
    if (!stripCache.has(id)) stripCache.set(id, props.platform.filmstrip(k.source, k.in, k.dur, k.count).catch(() => null));
    return stripCache.get(id)!;
  });
  return <Show when={src()}>{(u) => <img class="strip" src={u()} alt="" draggable={false} />}</Show>;
}

export function Timeline(props: { editor: Editor; pxPerSec: number; platform: Platform }) {
  const e = props.editor;
  const xToMs = (x: number) => Math.max(0, (x / props.pxPerSec) * 1000);
  const seconds = () => Math.max(10, Math.ceil(e.duration() / 1000) + 5);

  function onLanePointer(ev: PointerEvent) {
    const lane = ev.currentTarget as HTMLElement;
    e.seek(xToMs(ev.clientX - lane.getBoundingClientRect().left));
  }

  function drag(ev: PointerEvent, it: Item, mode: "move" | "left" | "right") {
    ev.stopPropagation();
    e.select(it.id);
    const el = (ev.currentTarget as HTMLElement).closest(".item") as HTMLElement;
    const x0 = ev.clientX;
    const targets = snapTargets(e.project(), e.playhead(), it.id);
    const thr = snapThresholdMs(props.pxPerSec);
    const end = it.start_ms + it.duration_ms;
    /** Snapped [start, duration] for a pointer delta. */
    const geometry = (dx: number): [number, number] => {
      const dms = (dx / props.pxPerSec) * 1000;
      if (mode === "move") return [snapMove(Math.max(0, it.start_ms + dms), it.duration_ms, targets, thr), it.duration_ms];
      if (mode === "left") { const s0 = Math.min(end - 50, Math.max(0, snap(it.start_ms + dms, targets, thr))); return [s0, end - s0]; }
      return [it.start_ms, Math.max(50, snap(end + dms, targets, thr) - it.start_ms)];
    };
    const onMove = (m: PointerEvent) => {
      const [s0, d0] = geometry(m.clientX - x0);
      el.style.left = `${timeToX(s0, props.pxPerSec)}px`;
      el.style.width = `${timeToX(d0, props.pxPerSec)}px`;
    };
    const onUp = (u: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      if (Math.abs(u.clientX - x0) >= 2) {
        const [s0, d0] = geometry(u.clientX - x0);
        if (mode === "move") e.move(it.id, s0);
        else e.trim(it.id, s0, d0);
      }
      // Re-render from the store either way (snaps back a rejected edit).
      const now = e.project().tracks.flatMap((t) => t.items).find((x) => x.id === it.id) ?? it;
      el.style.left = `${timeToX(now.start_ms, props.pxPerSec)}px`;
      el.style.width = `${timeToX(now.duration_ms, props.pxPerSec)}px`;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  return (
    <section class="ms-timeline">
      <div class="ruler" onPointerDown={onLanePointer} style={{ width: `${seconds() * props.pxPerSec}px` }}>
        <For each={Array.from({ length: seconds() + 1 }, (_, i) => i)}>{(s) => (
          <span style={{ left: `${timeToX(s * 1000, props.pxPerSec)}px` }}>{formatTime(s * 1000).replace(/\.\d$/, "")}</span>
        )}</For>
      </div>
      <For each={e.project().tracks}>{(t) => (
        <div class={`track ${t.kind}`}>
          <div class="head">{t.name}</div>
          <div class="lane" onPointerDown={onLanePointer} style={{ width: `${seconds() * props.pxPerSec}px` }}>
            <For each={t.items}>{(it) => (
              <div class={`item ${e.selected() === it.id ? "sel" : ""}`} data-id={it.id}
                style={{ left: `${timeToX(it.start_ms, props.pxPerSec)}px`, width: `${timeToX(it.duration_ms, props.pxPerSec)}px` }}
                onPointerDown={(ev) => drag(ev, it, "move")}>
                <Show when={t.kind === "video"}><Filmstrip platform={props.platform} item={it} pxPerSec={props.pxPerSec} /></Show>
                <i class="grip l" onPointerDown={(ev) => drag(ev, it, "left")} />
                <span>{label(it)}</span>
                <i class="grip r" onPointerDown={(ev) => drag(ev, it, "right")} />
              </div>
            )}</For>
          </div>
        </div>
      )}</For>
      <div class="playhead" style={{ left: `${HEAD + timeToX(e.playhead(), props.pxPerSec)}px` }} />
    </section>
  );
}
