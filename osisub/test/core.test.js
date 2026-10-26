import { test } from "node:test";
import assert from "node:assert/strict";
import { parse, serialize, parseAssTime, formatAssTime, load, save, check, fixes, stretch, shift, visibleText, stripTags, parseSrt, serializeSrt } from "../src/core/index.js";

const ASS = `[Script Info]
Title: x
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,25,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:02.50,Default,Aoi,0,0,0,,{\\i1}Hello, world{\\i0}\\Nsecond
Comment: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,note, with comma
`;

test("time round trip", () => {
  assert.equal(parseAssTime("0:01:02.34"), 62340);
  assert.equal(formatAssTime(62340), "0:01:02.34");
  assert.equal(parseAssTime("1:00:00.5"), 3600500);
});

test("ass parse/serialize keeps commas in text and style flags", () => {
  const d = parse(ASS);
  assert.equal(d.events.length, 2);
  assert.equal(d.events[0].actor, "Aoi");
  assert.equal(d.events[1].text, "note, with comma");
  assert.equal(d.events[1].comment, true);
  assert.equal(d.styles[0].bold, true);
  assert.equal(d.styles[0].margin_t, 25);
  const again = parse(serialize(d));
  assert.deepEqual(again.events, d.events);
  assert.deepEqual(again.styles, d.styles);
});

test("tags", () => {
  assert.equal(visibleText("{\\i1}Hi{\\i0}\\Nthere"), "Hithere");
  assert.equal(stripTags("{\\b1}A{\\b0}\\NB"), "A\nB");
  assert.equal(visibleText("{\\p1}m 0 0 l 10 10{\\p0}text"), "text");
});

test("srt round trip via ass", () => {
  const d = parseSrt("1\n00:00:01,000 --> 00:00:02,500\n<i>Hi</i>\nthere\n\n2\n00:00:03,000 --> 00:00:04,000\nBye\n");
  assert.equal(d.events.length, 2);
  assert.equal(d.events[0].text, "{\\i1}Hi{\\i0}\\Nthere");
  assert.match(serializeSrt(d), /00:00:01,000 --> 00:00:02,500\n<i>Hi<\/i>\nthere/);
});

test("format sniffing", () => {
  assert.equal(load(ASS).format, "ass");
  assert.equal(load("WEBVTT\n\n00:01.000 --> 00:02.000\nhi\n").format, "vtt");
});

test("qc finds overlap, cps and missing style", () => {
  const d = parse(ASS);
  d.events.push({ ...d.events[0], start: 2000, end: 2300, text: "x".repeat(60) });
  d.events.push({ ...d.events[0], start: 9000, end: 10000, style: "Nope" });
  const codes = check(d).map((i) => i.code);
  for (const c of ["overlap", "cps", "missing-style", "cpl", "short"]) assert.ok(codes.includes(c), c);
});

test("fixes", () => {
  const d = parse(ASS);
  d.events.push({ ...d.events[0], start: 2000, end: 3000 });
  assert.equal(fixes.overlaps(d), 1);
  assert.equal(d.events[0].end, 2000);
  d.events[0].text = "a   b  \\N  \\Nc";
  fixes.whitespace(d);
  assert.equal(d.events[0].text, "a b\\Nc");
});

test("shift and stretch", () => {
  const d = parse(ASS);
  shift(d, 500);
  assert.equal(d.events[0].start, 1500);
  stretch(d, 0, 10000, 0, 20000);
  assert.equal(d.events[0].start, 3000);
});
