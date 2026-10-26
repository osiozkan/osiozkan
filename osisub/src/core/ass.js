// ASS / SSA document model: lossless-ish parse and serialize.
import { parseAssTime, formatAssTime } from "./time.js";

export const STYLE_FIELDS = [
  "name", "fontname", "fontsize", "color1", "color2", "color3", "color4",
  "bold", "italic", "underline", "strikeout", "scale_x", "scale_y", "spacing",
  "angle", "borderstyle", "outline", "shadow", "align",
  "margin_l", "margin_r", "margin_t", "encoding",
];
const STYLE_FORMAT = [
  "Name", "Fontname", "Fontsize", "PrimaryColour", "SecondaryColour", "OutlineColour", "BackColour",
  "Bold", "Italic", "Underline", "StrikeOut", "ScaleX", "ScaleY", "Spacing", "Angle",
  "BorderStyle", "Outline", "Shadow", "Alignment", "MarginL", "MarginR", "MarginV", "Encoding",
];
const EVENT_FORMAT = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"];

export function defaultStyle(over = {}) {
  return {
    name: "Default", fontname: "Arial", fontsize: 48,
    color1: "&H00FFFFFF", color2: "&H000000FF", color3: "&H00000000", color4: "&H00000000",
    bold: false, italic: false, underline: false, strikeout: false,
    scale_x: 100, scale_y: 100, spacing: 0, angle: 0, borderstyle: 1,
    outline: 2, shadow: 2, align: 2, margin_l: 10, margin_r: 10, margin_t: 10, margin_b: 10, encoding: 1,
    ...over,
  };
}

export function defaultEvent(over = {}) {
  return {
    comment: false, layer: 0, start: 0, end: 0, style: "Default", actor: "",
    margin_l: 0, margin_r: 0, margin_t: 0, effect: "", text: "", ...over,
  };
}

export function newDocument() {
  return {
    info: [
      { key: "Title", value: "Untitled" },
      { key: "ScriptType", value: "v4.00+" },
      { key: "WrapStyle", value: "0" },
      { key: "ScaledBorderAndShadow", value: "yes" },
      { key: "YCbCr Matrix", value: "None" },
      { key: "PlayResX", value: "1920" },
      { key: "PlayResY", value: "1080" },
    ],
    styles: [defaultStyle()],
    events: [],
    // sections we do not interpret but must round-trip verbatim
    aegisub: [], // [Aegisub Project Garbage] lines
    extradata: [], // [Aegisub Extradata]
    fonts: [], graphics: [], // raw uuencoded lines
    otherSections: [], // [{name, lines}]
  };
}

export function getInfo(doc, key, def) {
  const e = doc.info.find((i) => i.key.toLowerCase() === key.toLowerCase());
  return e ? e.value : def;
}
export function setInfo(doc, key, value) {
  const e = doc.info.find((i) => i.key.toLowerCase() === key.toLowerCase());
  if (e) e.value = String(value);
  else doc.info.push({ key, value: String(value) });
}

const bool = (v) => v === "-1" || v === "1" || v.toLowerCase() === "yes" || v.toLowerCase() === "true";

/** SSA "&Hbbggrr&" / ASS "&HAABBGGRR" / decimal -> ASS &HAABBGGRR */
export function normalizeColor(v) {
  v = String(v).trim();
  if (/^-?\d+$/.test(v)) {
    const n = Number(v) >>> 0;
    return "&H" + n.toString(16).toUpperCase().padStart(8, "0");
  }
  const m = /^&?H?([0-9a-f]{1,8})&?$/i.exec(v);
  if (!m) return "&H00FFFFFF";
  return "&H" + m[1].toUpperCase().padStart(8, "0");
}

export function parse(text) {
  if (text.charCodeAt(0) === 0xfeff) text = text.slice(1);
  const doc = newDocument();
  doc.info = []; doc.styles = []; doc.events = [];
  const lines = text.split(/\r\n|\r|\n/);
  let section = null, other = null;
  let styleFmt = null, eventFmt = null, ssa = false;
  for (const raw of lines) {
    const line = raw;
    const sm = /^\s*\[(.+?)\]\s*$/.exec(line);
    if (sm) {
      section = sm[1].toLowerCase();
      other = null;
      if (!["script info", "v4+ styles", "v4 styles", "events", "aegisub project garbage", "aegisub extradata", "fonts", "graphics"].includes(section)) {
        other = { name: sm[1], lines: [] };
        doc.otherSections.push(other);
      }
      if (section === "v4 styles") ssa = true;
      continue;
    }
    if (other) { other.lines.push(line); continue; }
    if (line.trim() === "" ) continue;
    switch (section) {
      case "script info": {
        if (line.startsWith(";")) { doc.info.push({ comment: line }); break; }
        const i = line.indexOf(":");
        if (i < 0) break;
        const key = line.slice(0, i).trim(), value = line.slice(i + 1).trim();
        if (key.toLowerCase() === "scripttype" && value.toLowerCase() === "v4.00") ssa = true;
        doc.info.push({ key, value });
        break;
      }
      case "v4+ styles": case "v4 styles": {
        const i = line.indexOf(":");
        if (i < 0) break;
        const k = line.slice(0, i).trim().toLowerCase(), rest = line.slice(i + 1).trim();
        if (k === "format") { styleFmt = rest.split(",").map((s) => s.trim().toLowerCase()); break; }
        if (k !== "style") break;
        doc.styles.push(parseStyle(rest, styleFmt, ssa));
        break;
      }
      case "events": {
        const i = line.indexOf(":");
        if (i < 0) break;
        const k = line.slice(0, i).trim().toLowerCase(), rest = line.slice(i + 1).replace(/^ /, "");
        if (k === "format") { eventFmt = rest.split(",").map((s) => s.trim().toLowerCase()); break; }
        if (k === "dialogue" || k === "comment") doc.events.push(parseEvent(rest, eventFmt || EVENT_FORMAT.map((s) => s.toLowerCase()), k === "comment", ssa));
        break;
      }
      case "aegisub project garbage": doc.aegisub.push(line); break;
      case "aegisub extradata": doc.extradata.push(line); break;
      case "fonts": doc.fonts.push(line); break;
      case "graphics": doc.graphics.push(line); break;
    }
  }
  if (ssa) {
    const t = doc.info.find((i) => i.key && i.key.toLowerCase() === "scripttype");
    if (t) t.value = "v4.00+";
  }
  return doc;
}

function parseStyle(rest, fmt, ssa) {
  const f = fmt || (ssa
    ? ["name", "fontname", "fontsize", "primarycolour", "secondarycolour", "tertiarycolour", "backcolour", "bold", "italic", "borderstyle", "outline", "shadow", "alignment", "marginl", "marginr", "marginv", "alphalevel", "encoding"]
    : STYLE_FORMAT.map((s) => s.toLowerCase()));
  const parts = splitN(rest, f.length);
  const r = {};
  f.forEach((k, i) => (r[k] = (parts[i] ?? "").trim()));
  const s = defaultStyle();
  s.name = r.name ?? "Default";
  s.fontname = r.fontname ?? s.fontname;
  s.fontsize = num(r.fontsize, s.fontsize);
  s.color1 = normalizeColor(r.primarycolour ?? "&H00FFFFFF");
  s.color2 = normalizeColor(r.secondarycolour ?? "&H000000FF");
  s.color3 = normalizeColor(r.outlinecolour ?? r.tertiarycolour ?? "&H00000000");
  s.color4 = normalizeColor(r.backcolour ?? "&H00000000");
  s.bold = bool(r.bold ?? "0"); s.italic = bool(r.italic ?? "0");
  s.underline = bool(r.underline ?? "0"); s.strikeout = bool(r.strikeout ?? "0");
  s.scale_x = num(r.scalex, 100); s.scale_y = num(r.scaley, 100);
  s.spacing = num(r.spacing, 0); s.angle = num(r.angle, 0);
  s.borderstyle = num(r.borderstyle, 1); s.outline = num(r.outline, 2); s.shadow = num(r.shadow, 2);
  let al = num(r.alignment, 2);
  if (ssa) al = ssaAlign(al);
  s.align = al;
  s.margin_l = num(r.marginl, 10); s.margin_r = num(r.marginr, 10);
  s.margin_t = s.margin_b = num(r.marginv, 10);
  s.encoding = num(r.encoding, 1);
  return s;
}

function ssaAlign(a) { // SSA legacy alignment -> numpad
  const m = { 1: 1, 2: 2, 3: 3, 5: 7, 6: 8, 7: 9, 9: 4, 10: 5, 11: 6 };
  return m[a] ?? 2;
}

function parseEvent(rest, fmt, comment, ssa) {
  const parts = splitN(rest, fmt.length);
  const r = {};
  fmt.forEach((k, i) => (r[k] = parts[i] ?? ""));
  const ev = defaultEvent({ comment });
  ev.layer = num(r.layer ?? r.marked?.replace(/\D/g, ""), 0);
  ev.start = parseAssTime(r.start ?? "0:00:00.00");
  ev.end = parseAssTime(r.end ?? "0:00:00.00");
  ev.style = (r.style ?? "Default").trim();
  ev.actor = (r.name ?? r.actor ?? "").trim();
  ev.margin_l = num(r.marginl, 0); ev.margin_r = num(r.marginr, 0); ev.margin_t = num(r.marginv, 0);
  ev.effect = (r.effect ?? "").trim();
  ev.text = r.text ?? "";
  return ev;
}

/** split on commas into at most n parts; the last part keeps remaining commas */
function splitN(s, n) {
  const out = [];
  let rest = s;
  while (out.length < n - 1) {
    const i = rest.indexOf(",");
    if (i < 0) break;
    out.push(rest.slice(0, i));
    rest = rest.slice(i + 1);
  }
  out.push(rest);
  return out;
}
const num = (v, d) => {
  if (v === undefined || v === "") return d;
  const n = Number(v);
  return Number.isFinite(n) ? n : d;
};

const b2 = (b) => (b ? "-1" : "0");
const n2 = (n) => String(Math.round(n * 1e5) / 1e5);

export function serializeStyle(s) {
  return "Style: " + [
    s.name, s.fontname, n2(s.fontsize), s.color1, s.color2, s.color3, s.color4,
    b2(s.bold), b2(s.italic), b2(s.underline), b2(s.strikeout), n2(s.scale_x), n2(s.scale_y),
    n2(s.spacing), n2(s.angle), s.borderstyle, n2(s.outline), n2(s.shadow), s.align,
    s.margin_l, s.margin_r, s.margin_t, s.encoding,
  ].join(",");
}

export function serializeEvent(e) {
  return `${e.comment ? "Comment" : "Dialogue"}: ${e.layer},${formatAssTime(e.start)},${formatAssTime(e.end)},${e.style},${e.actor},${e.margin_l},${e.margin_r},${e.margin_t},${e.effect},${e.text}`;
}

export function serialize(doc, { eol = "\n", bom = false } = {}) {
  const out = ["[Script Info]"];
  for (const i of doc.info) out.push(i.comment !== undefined ? i.comment : `${i.key}: ${i.value}`);
  if (doc.aegisub.length) out.push("", "[Aegisub Project Garbage]", ...doc.aegisub);
  out.push("", "[V4+ Styles]", "Format: " + STYLE_FORMAT.join(", "));
  for (const s of doc.styles) out.push(serializeStyle(s));
  out.push("", "[Events]", "Format: " + EVENT_FORMAT.join(", "));
  for (const e of doc.events) out.push(serializeEvent(e));
  if (doc.fonts.length) out.push("", "[Fonts]", ...doc.fonts);
  if (doc.graphics.length) out.push("", "[Graphics]", ...doc.graphics);
  if (doc.extradata.length) out.push("", "[Aegisub Extradata]", ...doc.extradata);
  for (const o of doc.otherSections) out.push("", `[${o.name}]`, ...o.lines.filter((l) => l !== ""));
  return (bom ? "﻿" : "") + out.join(eol) + eol;
}

export function cloneDoc(doc) {
  return structuredClone(doc);
}
