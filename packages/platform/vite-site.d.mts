import type { Plugin } from "vite";
export const HEADERS: string;
/** SMITH_SITE_URL (normalised to end in "/") if set, else `fallback`. Throws on a non-http(s) URL. */
export function siteUrl(fallback: string, env?: string): string;
export default function smithSite(o: { page?: string; url: string; title: string; description: string; og: string }): Plugin;
