import { AssFile, Event, newEvent, newFile, setInfo } from "./ass";
import { formatSrtTime, parseSrtTime, parseVttTime } from "./time";
import { stripTags } from "./tags";

/** ASS satır metni -> SRT/VTT biçimi (etiketler: b/i/u/s ve renk korunur). */
export function assTextToHtmlish(text: string): string {
  let out = "";
  let i = 0;
  const open: Record<string, boolean> = {};
  const tagMap: Record<string, string> = { b: "b", i: "i", u: "u", s: "s" };
  while (i < text.length) {
    if (text[i] === "{") {
      const j = text.indexOf("}", i);
      const blk = text.slice(i + 1, j < 0 ? text.length : j);
      for (const m of blk.matchAll(/\\([bius])(\d*)/g)) {
        const t = tagMap[m[1]];
        const on = m[2] === "1" || (m[1] !== "b" && m[2] === "1") || (m[1] === "b" && m[2] !== "0" && m[2] !== "");
        if (on && !open[t]) { out += `<${t}>`; open[t] = true; }
        else if (!on && open[t]) { out += `</${t}>`; open[t] = false; }
      }
      i = j < 0 ? text.length : j + 1;
    } else if (text.startsWith("\\N", i) || text.startsWith("\\n", i)) { out += "\n"; i += 2; }
    else if (text.startsWith("\\h", i)) { out += " "; i += 2; }
    else out += text[i++];
  }
  for (const t of ["s", "u", "i", "b"]) if (open[t]) out += `</${t}>`;
  return out;
}

export function htmlishToAssText(s: string): string {
  return s
    .replace(/\r?\n/g, "\\N")
    .replace(/<\s*(b|i|u|s)\s*>/gi, (_, t) => `{\\${t.toLowerCase()}1}`)
    .replace(/<\s*\/\s*(b|i|u|s)\s*>/gi, (_, t) => `{\\${t.toLowerCase()}0}`)
    .replace(/<font\s+color="?#?([0-9a-fA-F]{6})"?\s*>/gi, (_, c) => `{\\c&H${c.slice(4, 6)}${c.slice(2, 4)}${c.slice(0, 2)}&}`)
    .replace(/<\/font>/gi, "")
    .replace(/<[^>]+>/g, "")
    .replace(/\{\\an(\d)\}/g, "{\\an$1}");
}

export function parseSrt(src: string): AssFile {
  if (src.charCodeAt(0) === 0xfeff) src = src.slice(1);
  const f = newFile();
  const blocks = src.replace(/\r\n?/g, "\n").split(/\n{2,}/);
  for (const blk of blocks) {
    const lines = blk.split("\n").filter((l, i, a) => !(i === 0 && l.trim() === "" && a.length > 1));
    let k = lines.findIndex((l) => l.includes("-->"));
    if (k < 0) continue;
    const m = /^\s*([\d:.,]+)\s*-->\s*([\d:.,]+)(.*)$/.exec(lines[k]);
    if (!m) continue;
    const s = parseSrtTime(m[1]), e = parseSrtTime(m[2]);
    if (Number.isNaN(s) || Number.isNaN(e)) continue;
    let text = htmlishToAssText(lines.slice(k + 1).join("\n"));
    const pos = /\{\\an(\d)\}/.exec(text);
    f.events.push(newEvent({ start: s, end: e, text }));
    void pos;
  }
  return f;
}

export function toSrt(f: AssFile): string {
  const evs = f.events.filter((e) => !e.comment).slice().sort((a, b) => a.start - b.start || a.end - b.end);
  return evs.map((e, i) => `${i + 1}\r\n${formatSrtTime(e.start)} --> ${formatSrtTime(e.end)}\r\n${assTextToHtmlish(e.text)}\r\n`).join("\r\n");
}

export function parseVtt(src: string): AssFile {
  const f = newFile();
  const blocks = src.replace(/^﻿/, "").replace(/\r\n?/g, "\n").split(/\n{2,}/);
  for (const blk of blocks) {
    if (/^(WEBVTT|NOTE|STYLE|REGION)/.test(blk)) continue;
    const lines = blk.split("\n");
    const k = lines.findIndex((l) => l.includes("-->"));
    if (k < 0) continue;
    const m = /^\s*([\d:.]+)\s*-->\s*([\d:.]+)/.exec(lines[k]);
    if (!m) continue;
    const s = parseVttTime(m[1]), e = parseVttTime(m[2]);
    if (Number.isNaN(s) || Number.isNaN(e)) continue;
    f.events.push(newEvent({ start: s, end: e, text: htmlishToAssText(lines.slice(k + 1).join("\n")) }));
  }
  return f;
}

export function toVtt(f: AssFile): string {
  const evs = f.events.filter((e) => !e.comment).slice().sort((a, b) => a.start - b.start);
  return "WEBVTT\n\n" + evs.map((e) => `${formatSrtTime(e.start, ".")} --> ${formatSrtTime(e.end, ".")}\n${assTextToHtmlish(e.text)}\n`).join("\n");
}

export function toPlainText(f: AssFile): string {
  return f.events.filter((e) => !e.comment).map((e) => stripTags(e.text)).join("\n");
}

/** Biçimi uzantı/içerikten tahmin edip ASS modeline çevirir. */
export function detectAndParse(name: string, src: string): AssFile {
  const ext = name.toLowerCase().split(".").pop();
  const head = src.slice(0, 400);
  if (ext === "ass" || ext === "ssa" || /\[Script Info\]/i.test(head)) return parseAssLazy(src);
  if (ext === "vtt" || /^﻿?WEBVTT/.test(head)) return parseVtt(src);
  return parseSrt(src);
}

import { parseAss } from "./ass";
const parseAssLazy = parseAss;

export function withTitle(f: AssFile, title: string): AssFile { setInfo(f, "Title", title); return f; }
export type { Event };
