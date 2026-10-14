import { describe, expect, it } from "vitest";
import { formatTime, timeToX } from "./time";

describe("formatTime", () => {
  it("formats sub-hour times", () => {
    expect(formatTime(0)).toBe("0:00.0");
    expect(formatTime(65_400)).toBe("1:05.4");
  });
  it("formats hours and clamps negatives", () => {
    expect(formatTime(3_723_000)).toBe("1:02:03.0");
    expect(formatTime(-5)).toBe("0:00.0");
  });
});

describe("timeToX", () => {
  it("scales by zoom", () => expect(timeToX(2500, 40)).toBe(100));
});
