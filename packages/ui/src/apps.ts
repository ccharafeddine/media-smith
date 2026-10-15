import { MARKS } from "./marks";

export type SmithApp = "mediasmith" | "clipsmith" | "gifsmith" | "captionsmith";

export interface AppInfo {
  id: SmithApp;
  /** The half of the wordmark drawn in the app hue. */
  prefix: string;
  hue: string;
  tagline: string;
}

/** Hues must match the [data-app] blocks in tokens.css (a test checks). */
export const APPS: Record<SmithApp, AppInfo> = {
  mediasmith: { id: "mediasmith", prefix: "Media", hue: "#a974ff", tagline: "Edit, caption and export short video." },
  clipsmith: { id: "clipsmith", prefix: "Clip", hue: "#45f2f2", tagline: "Grab and trim a clip from any link." },
  gifsmith: { id: "gifsmith", prefix: "Gif", hue: "#ffb547", tagline: "Turn any moment into a crisp GIF." },
  captionsmith: { id: "captionsmith", prefix: "Caption", hue: "#ff5fa2", tagline: "Accurate captions, styled your way." },
};

export function markSvg(app: SmithApp): string {
  return MARKS[app];
}

export function markDataUrl(app: SmithApp): string {
  return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(MARKS[app]);
}

export type Theme = "dark" | "light";

/** Apply app hue and theme to the document root. */
export function applyIdentity(app: SmithApp, theme: Theme = "dark", root: { setAttribute(k: string, v: string): void } = document.documentElement): void {
  root.setAttribute("data-app", app);
  root.setAttribute("data-theme", theme);
}
