import { describe, expect, it } from "vitest";
import { assColor, cropFilter, parseSubs, toAss, toSrt, toVtt, DEFAULT_STYLE } from "./subs";

const SRT = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello <i>there</i>\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nTwo\r\nlines\r\n";

describe("subs", () => {
  it("parses SRT and VTT", () => {
    const c = parseSubs(SRT);
    expect(c).toEqual([{ start: 1, end: 2.5, text: "Hello there" }, { start: 3, end: 4, text: "Two\nlines" }]);
    expect(parseSubs("WEBVTT\n\n00:01.5 --> 00:02.000 align:center\nHi\n")).toEqual([{ start: 1.5, end: 2, text: "Hi" }]);
  });
  it("round-trips through SRT and VTT", () => {
    const c = parseSubs(SRT);
    expect(parseSubs(toSrt(c))).toEqual(c);
    expect(parseSubs(toVtt(c))).toEqual(c);
    expect(toSrt(c)).toContain("00:00:01,000 --> 00:00:02,500");
    expect(toVtt(c).startsWith("WEBVTT")).toBe(true);
  });
  it("skips malformed blocks", () => {
    expect(parseSubs("garbage\n\n1\n00:00:05,000 --> 00:00:04,000\nbackwards")).toEqual([]);
  });
  it("builds an ASS script at the video resolution", () => {
    const a = toAss([{ start: 0, end: 1.25, text: "a{b}\nc" }], DEFAULT_STYLE, 720, 1280);
    expect(a).toContain("PlayResY: 1280");
    expect(a).toContain("Dialogue: 0,0:00:00.00,0:00:01.25,Cap,,0,0,0,,a\\{b\\}\\Nc");
    expect(assColor("#ff8800")).toBe("&H000088FF");
  });
  it("crops to even dimensions only when a ratio is set", () => {
    expect(cropFilter(null)).toBe("");
    expect(cropFilter(9 / 16)).toMatch(/^crop='trunc\(/);
  });
});
