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

  function apply(f: (p: M.Project) => M.Project): boolean {
    try {
      const next = f(project());
      undo.push(project());
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
      if (prev) setProject(prev);
      return !!prev;
    },
  };
}

export type Editor = ReturnType<typeof createEditor>;
