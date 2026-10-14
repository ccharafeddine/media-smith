import { For } from "solid-js";
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
export function Timeline(props: { editor: Editor; pxPerSec: number }) {
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
    const startPx = timeToX(it.start_ms, props.pxPerSec);
    const widthPx = timeToX(it.duration_ms, props.pxPerSec);
    const onMove = (m: PointerEvent) => {
      const dx = m.clientX - x0;
      if (mode === "move") el.style.left = `${Math.max(0, startPx + dx)}px`;
      if (mode === "left") { el.style.left = `${startPx + dx}px`; el.style.width = `${Math.max(4, widthPx - dx)}px`; }
      if (mode === "right") el.style.width = `${Math.max(4, widthPx + dx)}px`;
    };
    const onUp = (u: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      const dms = ((u.clientX - x0) / props.pxPerSec) * 1000;
      if (Math.abs(u.clientX - x0) < 2) return;
      if (mode === "move") e.move(it.id, it.start_ms + dms);
      if (mode === "left") e.trim(it.id, it.start_ms + dms, it.duration_ms - dms);
      if (mode === "right") e.trim(it.id, it.start_ms, it.duration_ms + dms);
      // Re-render from the store either way (snaps back a rejected edit).
      el.style.left = `${timeToX(e.project().tracks.flatMap((t) => t.items).find((x) => x.id === it.id)?.start_ms ?? it.start_ms, props.pxPerSec)}px`;
      el.style.width = "";
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
