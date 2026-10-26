// ASS override-tag helpers.

/** Split text into [{type:'tags'|'text', value}] where tags are {...} blocks. */
export function splitBlocks(text) {
  const out = [];
  const re = /\{[^}]*\}/g;
  let last = 0, m;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push({ type: "text", value: text.slice(last, m.index) });
    out.push({ type: "tags", value: m[0] });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ type: "text", value: text.slice(last) });
  return out;
}

/** Remove override blocks; \N \n \h become newline/space. */
export function stripTags(text, { keepBreaks = false } = {}) {
  let t = text.replace(/\{[^}]*\}/g, "");
  if (keepBreaks) return t;
  return t.replace(/\\N/g, "\n").replace(/\\n/g, " ").replace(/\\h/g, " ");
}

/** Visible characters used for CPS/CPL: no tags, no breaks, no drawing mode text. */
export function visibleText(text) {
  let drawing = false;
  let out = "";
  for (const b of splitBlocks(text)) {
    if (b.type === "tags") {
      const p = /\\p(\d+)/g; let m;
      while ((m = p.exec(b.value))) drawing = Number(m[1]) > 0;
    } else if (!drawing) out += b.value;
  }
  return out.replace(/\\N|\\n|\\h/g, (s) => (s === "\\h" ? " " : "")).replace(/\s+/g, " ").trim();
}

export function isDrawing(text) {
  return /\{[^}]*\\p[1-9][^}]*\}/.test(text);
}

/** Lines of the event, split by \N. */
export const splitLines = (text) => text.split(/\\N/);

/** html-like (srt) tags -> ASS and back */
export function srtToAssText(t) {
  return t
    .replace(/\r?\n/g, "\\N")
    .replace(/<\s*b\s*>/gi, "{\\b1}").replace(/<\s*\/\s*b\s*>/gi, "{\\b0}")
    .replace(/<\s*i\s*>/gi, "{\\i1}").replace(/<\s*\/\s*i\s*>/gi, "{\\i0}")
    .replace(/<\s*u\s*>/gi, "{\\u1}").replace(/<\s*\/\s*u\s*>/gi, "{\\u0}")
    .replace(/<\s*s\s*>/gi, "{\\s1}").replace(/<\s*\/\s*s\s*>/gi, "{\\s0}")
    .replace(/<\s*font[^>]*color\s*=\s*["']?#?([0-9a-f]{6})["']?[^>]*>/gi, (_, c) => `{\\c&H${c.slice(4, 6)}${c.slice(2, 4)}${c.slice(0, 2)}&}`.toUpperCase().replace("\\C", "\\c"))
    .replace(/<\/\s*font\s*>/gi, "{\\c}")
    .replace(/<[^>]+>/g, "")
    .replace(/\{\\an(\d)\}/g, "{\\an$1}");
}

export function assToSrtText(t) {
  let out = t
    .replace(/\{[^}]*\}/g, (blk) => {
      let r = "";
      if (/\\i1/.test(blk)) r += "<i>";
      if (/\\i0/.test(blk)) r += "</i>";
      if (/\\b1/.test(blk)) r += "<b>";
      if (/\\b0/.test(blk)) r += "</b>";
      if (/\\u1/.test(blk)) r += "<u>";
      if (/\\u0/.test(blk)) r += "</u>";
      return r;
    })
    .replace(/\\N/g, "\n").replace(/\\n/g, " ").replace(/\\h/g, " ");
  return out;
}
