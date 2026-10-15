import { describe, expect, it } from "vitest";
import { assColor, assTime, buildAss, karaokeText } from "./ass";
import { clipArgs, planRender, silentClipArgs, webSize } from "./webRender";
import { partUrls } from "./ffmpegLoader";

const style = { font: "Syne", size_px: 84, color: "#ffd400", stroke: "#000000", x: 0.5, y: 0.75 };
const project = {
  name: "demo",
  canvas: { width: 1080, height: 1920 },
  tracks: [
    { kind: "video", items: [
      { type: "clip", source: "web:2", start_ms: 4000, duration_ms: 2000, source_in_ms: 500 },
      { type: "clip", source: "web:1", start_ms: 0, duration_ms: 4000, source_in_ms: 0, fade: { in_ms: 300, out_ms: 0 } },
    ] },
    { kind: "caption", items: [
      { type: "caption", start_ms: 1000, duration_ms: 1500, text: "hi there", style: { ...style, highlight: "#a974ff" },
        words: [{ text: "hi", start_ms: 0, end_ms: 500 }, { text: "there", start_ms: 600, end_ms: 1400 }] },
    ] },
  ],
};

describe("ASS port (mirrors ms-media ass.rs)", () => {
  it("converts colours and times like the Rust side", () => {
    expect(assColor("#ffd400")).toBe("&H0000D4FF");
    expect(assColor("nope")).toBe("&H00FFFFFF");
    expect(assTime(3_723_450)).toBe("1:02:03.45");
  });
  it("writes karaoke timing in centiseconds", () => {
    expect(karaokeText([{ text: "a", start_ms: 0, end_ms: 400 }, { text: "b", start_ms: 500, end_ms: 900 }])).toBe("{\\k50}a {\\k40}b");
  });
  it("scales positions and sizes to the output", () => {
    const ass = buildAss(project, 540, 960);
    expect(ass).toContain("PlayResX: 540");
    expect(ass).toContain("\\pos(270,720)");
    expect(ass).toContain("\\fs42");
    expect(ass).toContain("Dialogue: 1,0:00:01.00,0:00:02.50");
  });
});

describe("web render plan", () => {
  it("caps the web size at 960p and keeps it even", () => {
    expect(webSize(1080, 1920)).toEqual([540, 960]);
    expect(webSize(1920, 1080)).toEqual([1706, 960]);
    expect(webSize(480, 480)).toEqual([480, 480]);
  });
  it("orders clips by start and only burns in when there is text", () => {
    const plan = planRender(project, 540, 960);
    expect(plan.clips.map((c) => c.source)).toEqual(["web:1", "web:2"]);
    expect(plan.totalMs).toBe(6000);
    expect(plan.ass).toContain("Dialogue");
    expect(planRender({ ...project, tracks: [project.tracks[0]] }, 540, 960).ass).toBeNull();
    expect(() => planRender({ ...project, tracks: [] }, 540, 960)).toThrow(/clip/);
  });
  it("builds trim, scale, pad and fade arguments", () => {
    const a = clipArgs(project.tracks[0].items[1] as never, "in.mp4", "seg0.mp4", 540, 960).join(" ");
    expect(a).toContain("-ss 0 -t 4 -i in.mp4");
    expect(a).toContain("pad=540:960");
    expect(a).toContain("fade=t=in:st=0:d=0.3");
    expect(a).toContain("amix=inputs=2");
    const s = silentClipArgs(project.tracks[0].items[1] as never, "in.mp4", "seg0.mp4", 540, 960).join(" ");
    expect(s).not.toContain("[0:a]");
    expect(s).toContain("[1:a]anull[a]");
  });
  it("resolves chunked core URLs under the site root", () => {
    expect(partUrls({ core: "c.js", parts: ["w.part0", "w.part1"], bytes: 2 }, "https://x.dev/app/")).toEqual({
      core: "https://x.dev/app/ffmpeg/c.js", parts: ["https://x.dev/app/ffmpeg/w.part0", "https://x.dev/app/ffmpeg/w.part1"],
    });
  });
});
