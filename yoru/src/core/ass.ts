import { formatAssTime, parseAssTime } from "./time";

export interface Style {
  name: string; fontname: string; fontsize: number;
  primary: string; secondary: string; outline: string; back: string;
  bold: boolean; italic: boolean; underline: boolean; strikeout: boolean;
  scalex: number; scaley: number; spacing: number; angle: number;
  borderStyle: number; outlineW: number; shadow: number; alignment: number;
  marginL: number; marginR: number; marginV: number; encoding: number;
}

export interface Event {
  comment: boolean;
  layer: number; start: number; end: number;
  style: string; actor: string;
  marginL: number; marginR: number; marginV: number;
  effect: string; text: string;
}

export interface AssFile {
  /** Script Info (sıra korunur). */
  info: [string, string][];
  styles: Style[];
  events: Event[];
  /** Fonts / Graphics / Aegisub Project Garbage / Extradata gibi bilinmeyen bölümler, ham halde. */
  extra: { header: string; lines: string[] }[];
  /** [V4+ Styles] ve [Events] format satırları (okunan sıra korunsun diye) */
  styleFormat?: string[];
  eventFormat?: string[];
}

export const DEFAULT_STYLE: Style = {
  name: "Default", fontname: "Arial", fontsize: 48,
  primary: "&H00FFFFFF", secondary: "&H000000FF", outline: "&H00000000", back: "&H00000000",
  bold: false, italic: false, underline: false, strikeout: false,
  scalex: 100, scaley: 100, spacing: 0, angle: 0, borderStyle: 1, outlineW: 2, shadow: 2,
  alignment: 2, marginL: 10, marginR: 10, marginV: 10, encoding: 1,
};

export function newEvent(p: Partial<Event> = {}): Event {
  return { comment: false, layer: 0, start: 0, end: 0, style: "Default", actor: "", marginL: 0, marginR: 0, marginV: 0, effect: "", text: "", ...p };
}

export function newFile(): AssFile {
  return {
    info: [["ScriptType", "v4.00+"], ["PlayResX", "1920"], ["PlayResY", "1080"], ["WrapStyle", "0"], ["ScaledBorderAndShadow", "yes"], ["YCbCr Matrix", "TV.709"]],
    styles: [{ ...DEFAULT_STYLE }], events: [], extra: [],
  };
}

const STYLE_FIELDS = ["Name","Fontname","Fontsize","PrimaryColour","SecondaryColour","OutlineColour","BackColour","Bold","Italic","Underline","StrikeOut","ScaleX","ScaleY","Spacing","Angle","BorderStyle","Outline","Shadow","Alignment","MarginL","MarginR","MarginV","Encoding"];
const EVENT_FIELDS = ["Layer","Start","End","Style","Name","MarginL","MarginR","MarginV","Effect","Text"];

const b = (v: string) => v.trim() !== "0" && v.trim() !== "";
const n = (v: string, d = 0) => { const x = parseFloat(v); return Number.isFinite(x) ? x : d; };

/** ASS renk normalize: &HAABBGGRR (veya &HBBGGRR) -> &HAABBGGRR */
export function normColor(v: string): string {
  const t = v.trim();
  if (/^-?\d+$/.test(t)) { // eski ondalık biçim
    const x = (Number(t) >>> 0).toString(16).toUpperCase().padStart(8, "0");
    return "&H" + x;
  }
  const m = /^&?H?([0-9a-fA-F]{1,8})&?$/.exec(t);
  if (!m) return "&H00FFFFFF";
  const h = m[1].toUpperCase();
  return "&H" + (h.length <= 6 ? "00" + h.padStart(6, "0") : h.padStart(8, "0"));
}

export function parseAss(src: string): AssFile {
  if (src.charCodeAt(0) === 0xfeff) src = src.slice(1);
  const f: AssFile = { info: [], styles: [], events: [], extra: [] };
  let section = "";
  let curExtra: { header: string; lines: string[] } | null = null;
  let sfmt = STYLE_FIELDS.map((s) => s.toLowerCase());
  let efmt = EVENT_FIELDS.map((s) => s.toLowerCase());
  for (const raw of src.split(/\r\n|\n|\r/)) {
    const line = raw;
    const sec = /^\s*\[(.+?)\]\s*$/.exec(line);
    if (sec) {
      section = sec[1].toLowerCase();
      curExtra = null;
      if (!["script info", "v4+ styles", "v4 styles", "events"].includes(section)) {
        curExtra = { header: sec[1], lines: [] };
        f.extra.push(curExtra);
      }
      continue;
    }
    if (curExtra) { curExtra.lines.push(line); continue; }
    if (!line.trim() || line.startsWith(";")) continue;
    if (section === "script info") {
      const i = line.indexOf(":");
      if (i > 0) f.info.push([line.slice(0, i).trim(), line.slice(i + 1).trim()]);
    } else if (section === "v4+ styles" || section === "v4 styles") {
      const i = line.indexOf(":");
      if (i < 0) continue;
      const key = line.slice(0, i).trim().toLowerCase();
      const val = line.slice(i + 1).trim();
      if (key === "format") { sfmt = val.split(",").map((s) => s.trim().toLowerCase()); f.styleFormat = val.split(",").map((s) => s.trim()); continue; }
      if (key !== "style") continue;
      const parts = val.split(",");
      const g = (name: string) => { const k = sfmt.indexOf(name); return k >= 0 ? (parts[k] ?? "") : ""; };
      const s: Style = {
        name: g("name").trim(), fontname: g("fontname").trim() || "Arial", fontsize: n(g("fontsize"), 48),
        primary: normColor(g("primarycolour")), secondary: normColor(g("secondarycolour")),
        outline: normColor(g("outlinecolour") || g("tertiarycolour")), back: normColor(g("backcolour")),
        bold: b(g("bold")), italic: b(g("italic")), underline: b(g("underline")), strikeout: b(g("strikeout")),
        scalex: n(g("scalex"), 100), scaley: n(g("scaley"), 100), spacing: n(g("spacing")), angle: n(g("angle")),
        borderStyle: n(g("borderstyle"), 1), outlineW: n(g("outline"), 2), shadow: n(g("shadow"), 2),
        alignment: n(g("alignment"), 2), marginL: n(g("marginl")), marginR: n(g("marginr")), marginV: n(g("marginv")),
        encoding: n(g("encoding"), 1),
      };
      f.styles.push(s);
    } else if (section === "events") {
      const i = line.indexOf(":");
      if (i < 0) continue;
      const key = line.slice(0, i).trim().toLowerCase();
      const val = line.slice(i + 1).replace(/^ /, "");
      if (key === "format") { efmt = val.split(",").map((s) => s.trim().toLowerCase()); f.eventFormat = val.split(",").map((s) => s.trim()); continue; }
      if (key !== "dialogue" && key !== "comment") continue;
      // Text son alan olduğundan yalnızca ilk (efmt.length-1) virgülde böl.
      const parts: string[] = [];
      let rest = val;
      for (let k = 0; k < efmt.length - 1; k++) {
        const c = rest.indexOf(",");
        if (c < 0) { parts.push(rest); rest = ""; } else { parts.push(rest.slice(0, c)); rest = rest.slice(c + 1); }
      }
      parts.push(rest);
      const g = (name: string) => { const k = efmt.indexOf(name); return k >= 0 ? (parts[k] ?? "") : ""; };
      const layer = g("layer") || g("marked");
      f.events.push({
        comment: key === "comment",
        layer: n(layer.replace(/marked=/i, "")),
        start: parseAssTime(g("start")), end: parseAssTime(g("end")),
        style: g("style").trim(), actor: (g("name") || g("actor")).trim(),
        marginL: n(g("marginl")), marginR: n(g("marginr")), marginV: n(g("marginv")),
        effect: g("effect").trim(), text: g("text"),
      });
    }
  }
  if (!f.info.length) f.info = newFile().info;
  return f;
}

const fnum = (x: number) => (Number.isInteger(x) ? String(x) : String(+x.toFixed(4)));
const bi = (v: boolean) => (v ? "-1" : "0");

export function serializeStyle(s: Style): string {
  return `Style: ${[s.name, s.fontname, fnum(s.fontsize), s.primary, s.secondary, s.outline, s.back, bi(s.bold), bi(s.italic), bi(s.underline), bi(s.strikeout), fnum(s.scalex), fnum(s.scaley), fnum(s.spacing), fnum(s.angle), s.borderStyle, fnum(s.outlineW), fnum(s.shadow), s.alignment, s.marginL, s.marginR, s.marginV, s.encoding].join(",")}`;
}

export function serializeEvent(e: Event): string {
  const m = (v: number) => String(v).padStart(4, "0");
  return `${e.comment ? "Comment" : "Dialogue"}: ${e.layer},${formatAssTime(e.start)},${formatAssTime(e.end)},${e.style},${e.actor},${m(e.marginL)},${m(e.marginR)},${m(e.marginV)},${e.effect},${e.text}`;
}

export function serializeAss(f: AssFile): string {
  const out: string[] = ["[Script Info]"];
  for (const [k, v] of f.info) out.push(`${k}: ${v}`);
  out.push("", "[V4+ Styles]", "Format: " + STYLE_FIELDS.join(", "));
  for (const s of f.styles) out.push(serializeStyle(s));
  for (const x of f.extra) {
    // Aegisub'da Fonts/Graphics gibi bölümler stil ile olaylar arasındadır; Garbage/Extradata sondadır.
    if (/^(fonts|graphics)$/i.test(x.header)) { out.push("", `[${x.header}]`, ...trimTail(x.lines)); }
  }
  out.push("", "[Events]", "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");
  for (const e of f.events) out.push(serializeEvent(e));
  for (const x of f.extra) {
    if (!/^(fonts|graphics)$/i.test(x.header)) { out.push("", `[${x.header}]`, ...trimTail(x.lines)); }
  }
  return out.join("\r\n") + "\r\n";
}

function trimTail(lines: string[]): string[] {
  const l = lines.slice();
  while (l.length && !l[l.length - 1].trim()) l.pop();
  return l;
}

export function getInfo(f: AssFile, key: string): string | undefined {
  return f.info.find(([k]) => k.toLowerCase() === key.toLowerCase())?.[1];
}
export function setInfo(f: AssFile, key: string, value: string) {
  const e = f.info.find(([k]) => k.toLowerCase() === key.toLowerCase());
  if (e) e[1] = value; else f.info.push([key, value]);
}
