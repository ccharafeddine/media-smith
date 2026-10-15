import { describe, expect, it } from "vitest";
import { placeSegments, replaceCaptionCues, segmentsFromWords } from "./webCaptions";

const w = (s: number, e: number, text: string) => ({ start_ms: s, end_ms: e, text });

describe("browser auto-captions", () => {
  it("groups words at sentence ends and long pauses", () => {
    const segs = segmentsFromWords([w(0, 300, "Hello"), w(300, 600, "there."), w(700, 900, "Next"), w(2500, 2800, "after")]);
    expect(segs.map((s) => s.text)).toEqual(["Hello there.", "Next", "after"]);
  });
  it("places like ms_captions::place (split at max words, clipped, offset)", () => {
    const seg = { start_ms: 0, end_ms: 1400, text: "a b c d", words: [w(0, 300, "a"), w(300, 600, "b"), w(600, 900, "c"), w(900, 1400, "d")] };
    const cues = placeSegments(5000, 1200, [seg], 2);
    expect(cues.map((c) => [c.start_ms, c.duration_ms, c.text])).toEqual([[5000, 600, "a b"], [5600, 600, "c d"]]);
    expect(cues[1].words.at(-1)).toEqual(w(5900, 6200, "d"));
  });
  it("replaces the caption track like Project::replace_caption_cues", () => {
    const p = { next_id: 2, tracks: [{ id: "t1", kind: "video", items: [] }] };
    const out = replaceCaptionCues(p, [{ start_ms: 1000, duration_ms: 500, text: " hi ", words: [w(1100, 1300, "hi")] }]);
    const cap = out.tracks.find((t: { kind: string }) => t.kind === "caption");
    expect(cap.items).toHaveLength(1);
    expect(cap.items[0]).toMatchObject({ start_ms: 1000, duration_ms: 500, type: "caption", text: "hi", words: [w(100, 300, "hi")] });
  });
});
