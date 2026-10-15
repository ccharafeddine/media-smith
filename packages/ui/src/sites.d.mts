export interface Site { app: "mediasmith" | "clipsmith" | "gifsmith" | "captionsmith"; url: string; title: string; description: string }
export const SITES: Record<"media" | "clip" | "gif" | "caption", Site>;
export type SiteId = keyof typeof SITES;
