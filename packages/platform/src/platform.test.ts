import { describe, expect, it } from "vitest";
import { getPlatform, gifArgs, isTauri } from "./index";

describe("platform adapter", () => {
  it("detects Tauri vs browser", () => {
    expect(isTauri({ __TAURI_INTERNALS__: {} })).toBe(true);
    expect(isTauri({})).toBe(false);
    expect(getPlatform({}).kind).toBe("web");
    expect(getPlatform({ __TAURI_INTERNALS__: {} }).kind).toBe("desktop");
  });
  it("web has no URL import", () => {
    expect(getPlatform({}).caps.urlImport).toBe(false);
  });
  it("web projects match the Rust schema", async () => {
    const p = JSON.parse(await getPlatform({}).newProject("x", 1080, 1920, 30));
    expect(p.version).toBe(1);
    expect(p.tracks.map((t: { kind: string }) => t.kind)).toEqual(["video", "audio", "caption"]);
  });
  it("builds a palette GIF command", () => {
    const a = gifArgs("in.mp4", "out.gif", { startSec: 1, durationSec: 3, width: 480, fps: 15 });
    expect(a.slice(0, 4)).toEqual(["-ss", "1", "-t", "3"]);
    expect(a.join(" ")).toContain("palettegen");
    expect(a.at(-1)).toBe("out.gif");
  });
});

import { PRESET_SIZES, safeName } from "./types";

describe("export presets and names", () => {
  it("matches the Rust preset sizes", () => {
    expect(PRESET_SIZES).toEqual({ vertical: [1080, 1920], landscape: [1920, 1080], square: [1080, 1080], gif: [480, 480] });
  });
  it("desktop exports all presets, web only GIF", () => {
    expect(getPlatform({ __TAURI_INTERNALS__: {} }).caps.exportPresets).toHaveLength(4);
    expect(getPlatform({}).caps.exportPresets).toEqual(["gif"]);
  });
  it("sanitises file names", () => {
    expect(safeName('my: "clip"/v2', "mp4")).toBe("my_ _clip_v2.mp4");
    expect(safeName("  ", "msproj")).toBe("Untitled.msproj");
  });
  it("web MP4 export explains it needs desktop", async () => {
    await expect(getPlatform({}).exportProject("{}", "vertical")).rejects.toThrow(/desktop/);
  });
});
