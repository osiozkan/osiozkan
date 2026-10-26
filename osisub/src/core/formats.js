// SRT / WebVTT / plain-text import & export; format sniffing.
import { newDocument, defaultEvent, parse as parseAss, serialize as serializeAss } from "./ass.js";
import { parseSrtTime, formatSrtTime } from "./time.js";
import { srtToAssText, assToSrtText } from "./tags.js";

export function sniff(text, name = "") {
  const t = text.replace(/^﻿/, "").trimStart();
  if (/^\[Script Info\]/i.test(t)) return "ass";
  if (/^WEBVTT/.test(t)) return "vtt";
  if (/\d{1,2}:\d{2}:\d{2}[.,]\d{1,3}\s*-->\s*\d/.test(t.slice(0, 2000))) return /\.vtt$/i.test(name) ? "vtt" : "srt";
  if (/\.ass$|\.ssa$/i.test(name)) return "ass";
  return "txt";
}

export function parseSrt(text, { vtt = false } = {}) {
  const doc = newDocument();
  const blocks = text.replace(/^﻿/, "").replace(/\r\n?/g, "\n").split(/\n{2,}/);
  for (const b of blocks) {
    const lines = b.split("\n").filter((l, i, a) => !(i === a.length - 1 && l === ""));
    const ti = lines.findIndex((l) => l.includes("-->"));
    if (ti < 0) continue;
    const m = /^\s*(\S+)\s*-->\s*(\S+)(.*)$/.exec(lines[ti]);
    if (!m) continue;
    const body = lines.slice(ti + 1).join("\n");
    const ev = defaultEvent({ start: parseSrtTime(m[1]), end: parseSrtTime(m[2]), text: srtToAssText(vtt ? body.replace(/<v\s+[^>]*>/g, "").replace(/<\/v>/g, "") : body) });
    const pos = /line:(\d+)%/.exec(m[3]);
    if (pos && Number(pos[1]) < 40) ev.text = "{\\an8}" + ev.text;
    doc.events.push(ev);
  }
  return doc;
}

export function serializeSrt(doc) {
  const evs = doc.events.filter((e) => !e.comment).sort((a, b) => a.start - b.start);
  return evs.map((e, i) => `${i + 1}\n${formatSrtTime(e.start)} --> ${formatSrtTime(e.end)}\n${assToSrtText(e.text)}\n`).join("\n");
}

export function serializeVtt(doc) {
  const evs = doc.events.filter((e) => !e.comment).sort((a, b) => a.start - b.start);
  return "WEBVTT\n\n" + evs.map((e) => `${formatSrtTime(e.start, ".")} --> ${formatSrtTime(e.end, ".")}\n${assToSrtText(e.text)}\n`).join("\n");
}

export function serializeTxt(doc) {
  return doc.events.filter((e) => !e.comment).map((e) => assToSrtText(e.text).replace(/<[^>]+>/g, "")).join("\n") + "\n";
}

export function load(text, name = "") {
  const f = sniff(text, name);
  if (f === "ass") return { format: f, doc: parseAss(text) };
  if (f === "srt") return { format: f, doc: parseSrt(text) };
  if (f === "vtt") return { format: f, doc: parseSrt(text, { vtt: true }) };
  const doc = newDocument();
  let t = 0;
  for (const l of text.split(/\r?\n/)) if (l.trim()) { doc.events.push(defaultEvent({ start: t, end: t + 2000, text: l.trim() })); t += 2000; }
  return { format: "txt", doc };
}

export function save(doc, format) {
  switch (format) {
    case "ass": return serializeAss(doc);
    case "srt": return serializeSrt(doc);
    case "vtt": return serializeVtt(doc);
    case "txt": return serializeTxt(doc);
    default: throw new Error("unknown format " + format);
  }
}
