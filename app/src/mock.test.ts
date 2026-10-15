import { describe, expect, it } from "vitest";
import { PRESET_SIZES } from "@mediasmith/platform";
import { AI_PROVIDERS, PRESETS, TOOLS } from "./mock";

describe("catalogues", () => {
  it("has the tools in sidebar order", () => {
    expect(TOOLS).toEqual(["Home", "Media", "Text", "Captions", "Design", "Settings"]);
  });
  it("preset sizes agree with the platform package", () => {
    for (const p of PRESETS) expect([p.w, p.h]).toEqual(PRESET_SIZES[p.id]);
  });
  it("lists the four BYOK provider families", () => {
    expect(AI_PROVIDERS.map((p) => p.id)).toEqual(["openai", "gemini", "anthropic", "compatible"]);
  });
});
