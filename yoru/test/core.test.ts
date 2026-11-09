import { describe, expect, it } from "vitest";
import { parseAss, serializeAss } from "../src/core/ass";
import { parseSrt, toSrt, parseVtt, toVtt } from "../src/core/formats";
import { formatAssTime, parseAssTime } from "../src/core/time";
import { parseKaraoke, fitKaraoke, stripTags, parseTags } from "../src/core/tags";
import { runQc } from "../src/core/qc";
import { mergeEvents, splitEvent, shiftTimes, replaceAll } from "../src/core/ops";

const SAMPLE = `[Script Info]
Title: t
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Merhaba, {\\i1}dünya{\\i0}\\Nikinci satır
Comment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,not
`;

describe("time", () => {
  it("round trip", () => { expect(formatAssTime(parseAssTime("1:02:03.45"))).toBe("1:02:03.45"); });
});
describe("ass", () => {
  it("parses commas in text", () => {
    const f = parseAss(SAMPLE);
    expect(f.events.length).toBe(2);
    expect(f.events[0].text).toBe("Merhaba, {\\i1}dünya{\\i0}\\Nikinci satır");
    expect(f.events[1].comment).toBe(true);
    expect(f.styles[0].bold).toBe(true);
  });
  it("round trips", () => {
    const f = parseAss(SAMPLE);
    const g = parseAss(serializeAss(f));
    expect(g.events).toEqual(f.events); expect(g.styles).toEqual(f.styles);
  });
});
describe("srt/vtt", () => {
  it("srt round trip", () => {
    const f = parseSrt("1\n00:00:01,000 --> 00:00:02,500\n<i>Selam</i>\nDünya\n\n2\n00:00:03,000 --> 00:00:04,000\nİki\n");
    expect(f.events[0].text).toBe("{\\i1}Selam{\\i0}\\NDünya");
    expect(toSrt(f)).toContain("<i>Selam</i>\nDünya");
    expect(parseSrt(toSrt(f)).events.length).toBe(2);
  });
  it("vtt", () => {
    const f = parseVtt("WEBVTT\n\n00:01.000 --> 00:02.000\nhi\n");
    expect(f.events[0].start).toBe(1000); expect(toVtt(f)).toContain("00:00:01.000 --> 00:00:02.000");
  });
});
describe("tags", () => {
  it("strip", () => expect(stripTags("{\\an8}a\\Nb")).toBe("a b"));
  it("parse", () => expect(parseTags("{\\an8\\pos(1,2)\\t(0,100,\\fs20)}").map((t) => t.name)).toEqual(["an", "pos", "t"]));
  it("karaoke", () => {
    const k = parseKaraoke("{\\k20}ka{\\kf30}ra");
    expect(k.map((s) => s.duration)).toEqual([200, 300]);
    expect(fitKaraoke("{\\k20}ka{\\k30}ra", 1000)).toBe("{\\k40}ka{\\k60}ra");
  });
});
describe("ops/qc", () => {
  it("split/merge/shift/replace", () => {
    const f = parseAss(SAMPLE);
    splitEvent(f, 0, 2000); expect(f.events.length).toBe(3);
    mergeEvents(f, [0, 1]); expect(f.events.length).toBe(2);
    shiftTimes(f, { ms: 500 }); expect(f.events[0].start).toBe(1500);
    expect(replaceAll(f, { find: "dünya", replace: "evren", skipTags: true })).toBeGreaterThan(0);
  });
  it("qc", () => {
    const f = parseAss(SAMPLE); f.events[0].end = f.events[0].start + 100;
    expect(runQc(f).some((i) => i.code === "cps")).toBe(true);
  });
});
