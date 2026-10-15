import { createSignal } from "solid-js";
import * as M from "./model";

/** Reactive editor store over the project model. Every edit goes through the
 *  pure model functions, so a rejected edit leaves the project untouched and
 *  surfaces as a message instead. Keeps a bounded undo stack. */
export function createEditor(initial: M.Project = M.newProject()) {
  const [project, setProject] = createSignal<M.Project>(initial);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [playhead, setPlayhead] = createSignal(0);
  const [message, setMessage] = createSignal<string | null>(null);
  const [dirty, setDirty] = createSignal(false);
  const undo: M.Project[] = [];
  const redo: M.Project[] = [];

  function apply(f: (p: M.Project) => M.Project): boolean {
    try {
      const next = f(project());
      undo.push(project());
      redo.length = 0;
      if (undo.length > 100) undo.shift();
      setProject(next);
      setDirty(true);
      setMessage(null);
      return true;
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      return false;
    }
  }

  const videoTrack = () => project().tracks.find((t) => t.kind === "video")!;

  return {
    project, selected, playhead, message, dirty,
    select: setSelected,
    seek: (ms: number) => setPlayhead(Math.max(0, Math.min(ms, Math.max(M.duration(project()), 0)))),
    setMessage,
    markSaved: () => setDirty(false),
    duration: () => M.duration(project()),
    load(p: M.Project) {
      undo.length = 0;
      redo.length = 0;
      setProject(p);
      setSelected(null);
      setPlayhead(0);
      setDirty(false);
    },
    rename: (name: string) => apply((p) => ({ ...p, name })),
    importClip(source: string, duration_ms: number) {
      let id = "";
      const ok = apply((p) => {
        const r = M.appendClip(p, videoTrack().id, source, duration_ms);
        id = r.id;
        return r.project;
      });
      if (ok) setSelected(id);
      return ok ? id : null;
    },
    addCaption(text: string, start_ms: number, duration_ms = 2000) {
      const cap = project().tracks.find((t) => t.kind === "caption");
      if (!cap) return false;
      return apply((p) => M.addItem(p, cap.id, start_ms, duration_ms, { type: "caption", text, style: M.defaultStyle() }).project);
    },
    updateText: (id: string, patch: M.TextPatch) => apply((p) => M.updateText(p, id, patch)),
    setTransform: (id: string, t: M.Transform | null) => apply((p) => M.setTransform(p, id, t)),
    /** Commit a canvas drag of a text/caption layer (one undo step). */
    moveText: (id: string, x: number, y: number) => apply((p) => M.updateText(p, id, { style: { x, y } })),
    styleAllCaptions: (patch: Partial<M.TextStyle>) => apply((p) => M.styleAllCaptions(p, patch)),
    colorBySpeaker: () => apply((p) => M.colorBySpeaker(p)),
    setSpeaker: (id: string, s: number | null) => apply((p) => M.setSpeaker(p, id, s)),
    /** Replace the whole project in one undoable step (e.g. after Auto-caption). */
    replace: (next: M.Project) => apply(() => (M.validate(next), next)),
    /** "+ Text" at the playhead on the first text track (created if missing). */
    addText(text: string, start_ms: number, duration_ms = 3000) {
      let id = "";
      const ok = apply((p) => {
        const t = M.ensureTrack(p, "text", "Text");
        const r = M.addItem(t.project, t.id, start_ms, duration_ms, { type: "text", text, style: { ...M.defaultStyle(), y: 0.12, size_px: 96 } });
        id = r.id;
        return r.project;
      });
      if (ok) setSelected(id);
      return ok ? id : null;
    },
    /** Import onto the overlay (second) video track at the playhead, as picture-in-picture. */
    importOverlay(source: string, duration_ms: number) {
      let id = "";
      const ok = apply((p) => {
        const t = M.ensureTrack(p, "video", "Overlay", 1);
        const r = M.addItem(t.project, t.id, playhead(), duration_ms, { type: "clip", source, source_in_ms: 0, volume: 1 });
        id = r.id;
        return M.setTransform(r.project, r.id, M.DEFAULT_TRANSFORM);
      });
      if (ok) setSelected(id);
      return ok ? id : null;
    },
    setTexts: (texts: Record<string, string>) => apply((p) => M.setTexts(p, texts)),
    move: (id: string, start: number, track?: string) => apply((p) => M.moveItem(p, id, start, track)),
    trim: (id: string, start: number, dur: number) => apply((p) => M.trimItem(p, id, start, dur)),
    splitAtPlayhead() {
      const id = selected();
      if (!id) return setMessage("Select a clip to split"), false;
      let right = "";
      const ok = apply((p) => {
        const r = M.splitItem(p, id, playhead());
        right = r.id;
        return r.project;
      });
      if (ok) setSelected(right);
      return ok;
    },
    deleteSelected() {
      const id = selected();
      if (!id) return false;
      const ok = apply((p) => M.deleteItem(p, id));
      if (ok) setSelected(null);
      return ok;
    },
    undo() {
      const prev = undo.pop();
      if (prev) { redo.push(project()); setProject(prev); setDirty(true); }
      return !!prev;
    },
    redo() {
      const next = redo.pop();
      if (next) { undo.push(project()); setProject(next); setDirty(true); }
      return !!next;
    },
    canUndo: () => undo.length > 0,
    setFade: (id: string, f: M.Fade | null) => apply((p) => M.setFade(p, id, f)),
    applyTemplate: (id: string) => apply((p) => M.applyTemplate(p, id, playhead())),
    duplicateSelected() {
      const id = selected();
      if (!id) return setMessage("Select something to duplicate"), false;
      let nid = "";
      const ok = apply((p) => { const r = M.duplicateItem(p, id); nid = r.id; return r.project; });
      if (ok) setSelected(nid);
      return ok;
    },
  };
}

export type Editor = ReturnType<typeof createEditor>;
