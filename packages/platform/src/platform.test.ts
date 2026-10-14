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
    expect(p.tracks.map((t: { kind: string }) => t.kind)).toEqual(["video", "video", "audio", "caption"]);
  });
  it("builds a palette GIF command", () => {
    const a = gifArgs("in.mp4", "out.gif", { startSec: 1, durationSec: 3, width: 480, fps: 15 });
    expect(a.slice(0, 4)).toEqual(["-ss", "1", "-t", "3"]);
    expect(a.join(" ")).toContain("palettegen");
    expect(a.at(-1)).toBe("out.gif");
  });
});
