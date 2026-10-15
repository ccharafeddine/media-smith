import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { APPS, applyIdentity, markDataUrl, type SmithApp } from "./apps";
import { MARKS } from "./marks";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
const comp = readFileSync(new URL("./components.css", import.meta.url), "utf8");

function block(selector: string): string {
  const i = css.indexOf(selector);
  expect(i, selector).toBeGreaterThan(-1);
  return css.slice(i, css.indexOf("}", i));
}
const names = (s: string) => [...s.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]);

describe("design tokens", () => {
  it("every app hue in APPS matches its [data-app] block", () => {
    for (const id of Object.keys(APPS) as SmithApp[]) {
      expect(block(`:root[data-app="${id}"]`)).toContain(`--app-hue: ${APPS[id].hue}`);
    }
  });
  it("the light theme only overrides names the dark base defines", () => {
    const base = new Set(names(css.slice(css.indexOf("Smith design system v2"))));
    for (const n of names(block(':root[data-theme="light"]'))) expect(base.has(n), n).toBe(true);
  });
  it("components only reference defined tokens", () => {
    const defined = new Set(names(css));
    const used = [...comp.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]);
    for (const u of used) expect(defined.has(u), u).toBe(true);
  });
  it("v1 names survive so old screens keep rendering", () => {
    for (const n of ["--bg-base", "--bg-panel", "--fg", "--accent", "--accent-2", "--grad", "--s4", "--r-panel"]) expect(names(css)).toContain(n);
  });
});

describe("brand", () => {
  it("each app has a mark in its own hue plus the shared spark", () => {
    for (const id of Object.keys(APPS) as SmithApp[]) {
      expect(MARKS[id]).toContain(APPS[id].hue);
      expect(MARKS[id]).toContain("#45f2f2");
      expect(markDataUrl(id).startsWith("data:image/svg+xml")).toBe(true);
    }
  });
  it("applyIdentity sets app and theme on the root", () => {
    const attrs: Record<string, string> = {};
    applyIdentity("gifsmith", "light", { setAttribute: (k, v) => (attrs[k] = v) });
    expect(attrs).toEqual({ "data-app": "gifsmith", "data-theme": "light" });
  });
});
