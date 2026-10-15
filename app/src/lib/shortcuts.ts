/** Editor keyboard shortcuts as data, so the help overlay and the handler
 *  can't drift apart, and the mapping is unit tested. */
export type Action =
  | "play" | "split" | "delete" | "undo" | "redo" | "save" | "open" | "duplicate"
  | "frameBack" | "frameFwd" | "secBack" | "secFwd" | "home" | "end" | "zoomIn" | "zoomOut" | "help" | "export";

export interface Shortcut { action: Action; keys: string; label: string }

export const SHORTCUTS: Shortcut[] = [
  { action: "play", keys: "Space", label: "Play / pause" },
  { action: "split", keys: "S", label: "Split at playhead" },
  { action: "delete", keys: "Delete", label: "Delete selected" },
  { action: "duplicate", keys: "Ctrl+D", label: "Duplicate selected" },
  { action: "undo", keys: "Ctrl+Z", label: "Undo" },
  { action: "redo", keys: "Ctrl+Shift+Z / Ctrl+Y", label: "Redo" },
  { action: "save", keys: "Ctrl+S", label: "Save project" },
  { action: "open", keys: "Ctrl+O", label: "Open project" },
  { action: "export", keys: "Ctrl+E", label: "Export" },
  { action: "frameBack", keys: "←", label: "Back one frame (text selected: nudge)" },
  { action: "frameFwd", keys: "→", label: "Forward one frame" },
  { action: "secBack", keys: "Shift+←", label: "Back 1 second" },
  { action: "secFwd", keys: "Shift+→", label: "Forward 1 second" },
  { action: "home", keys: "Home", label: "Go to start" },
  { action: "end", keys: "End", label: "Go to end" },
  { action: "zoomIn", keys: "+", label: "Zoom timeline in" },
  { action: "zoomOut", keys: "−", label: "Zoom timeline out" },
  { action: "help", keys: "?", label: "Show shortcuts" },
];

export interface KeyLike { key: string; ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean; altKey?: boolean }

/** The action for a key press, or null. Ctrl and ⌘ are treated alike. */
export function actionFor(ev: KeyLike): Action | null {
  const mod = !!(ev.ctrlKey || ev.metaKey);
  const k = ev.key.length === 1 ? ev.key.toLowerCase() : ev.key;
  if (mod) {
    if (k === "z") return ev.shiftKey ? "redo" : "undo";
    if (k === "y") return "redo";
    if (k === "s") return "save";
    if (k === "o") return "open";
    if (k === "d") return "duplicate";
    if (k === "e") return "export";
    return null;
  }
  if (ev.altKey) return null;
  switch (k) {
    case " ": return "play";
    case "s": return "split";
    case "Delete": case "Backspace": return "delete";
    case "ArrowLeft": return ev.shiftKey ? "secBack" : "frameBack";
    case "ArrowRight": return ev.shiftKey ? "secFwd" : "frameFwd";
    case "Home": return "home";
    case "End": return "end";
    case "+": case "=": return "zoomIn";
    case "-": case "_": return "zoomOut";
    case "?": return "help";
  }
  return null;
}

/** Typing in a field must never trigger editor shortcuts. */
export function isTypingTarget(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el || !el.tagName) return false;
  return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable === true;
}
