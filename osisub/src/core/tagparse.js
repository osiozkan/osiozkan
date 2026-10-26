// Override tag tokenizer / writer / cleaner.
const NAMES = [
  "fscx", "fscy", "fsp", "fnr", "xbord", "ybord", "xshad", "yshad", "bord", "shad", "blur", "alpha", "clip", "iclip",
  "move", "pos", "org", "fade", "fad", "frx", "fry", "frz", "fax", "fay", "fe", "fn", "fs", "fr", "an", "pbo",
  "kf", "ko", "kt", "be", "1c", "2c", "3c", "4c", "1a", "2a", "3a", "4a", "K", "k", "q", "r", "t", "a", "b", "i", "u", "s", "c", "p",
].sort((x, y) => y.length - x.length);

/** @returns {{name:string, value:string}[]} tags in a {...} block (inner text without braces). Unknown junk is kept as {name:"",value}. */
export function parseBlock(inner) {
  const out = [];
  let i = 0;
  while (i < inner.length) {
    if (inner[i] !== "\\") {
      // comment text between tags
      let j = inner.indexOf("\\", i); if (j < 0) j = inner.length;
      out.push({ name: "", value: inner.slice(i, j) }); i = j; continue;
    }
    const name = NAMES.find((n) => inner.startsWith(n, i + 1));
    if (!name) { let j = inner.indexOf("\\", i + 1); if (j < 0) j = inner.length; out.push({ name: "", value: inner.slice(i, j) }); i = j; continue; }
    let j = i + 1 + name.length, depth = 0;
    for (; j < inner.length; j++) {
      const ch = inner[j];
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
      else if (ch === "\\" && depth <= 0) break;
    }
    out.push({ name, value: inner.slice(i + 1 + name.length, j) });
    i = j;
  }
  return out;
}
export const stringifyTags = (tags) => tags.map((t) => (t.name ? "\\" + t.name + t.value : t.value)).join("");

/** Tags that may not simply be overridden by a later repeat in the same block. */
const STACKING = new Set(["t", "k", "kf", "ko", "K", "kt", "r", "clip", "iclip", "move", "fad", "fade"]);

/** Merge adjacent {..}{..}, drop empty blocks, and remove earlier duplicates inside one block (last wins). */
export function cleanTags(text) {
  const merged = text.replace(/\}\{/g, "");
  return merged.replace(/\{([^}]*)\}/g, (_, inner) => {
    const tags = parseBlock(inner);
    const last = new Map();
    tags.forEach((t, i) => { if (t.name && !STACKING.has(t.name)) last.set(t.name, i); });
    const kept = tags.filter((t, i) => !t.name || STACKING.has(t.name) || last.get(t.name) === i);
    const s = stringifyTags(kept);
    return s === "" ? "" : "{" + s + "}";
  });
}

/** Returns the value of the last occurrence of a tag in the text, or null. */
export function getTag(text, name) {
  let v = null;
  for (const m of text.matchAll(/\{([^}]*)\}/g)) for (const t of parseBlock(m[1])) if (t.name === name) v = t.value;
  return v;
}

/** Remove all occurrences of the given tag names from every block. */
export function removeTags(text, names) {
  const set = new Set(names);
  return text.replace(/\{([^}]*)\}/g, (_, inner) => {
    const s = stringifyTags(parseBlock(inner).filter((t) => !set.has(t.name)));
    return s === "" ? "" : "{" + s + "}";
  });
}

/** Prepend (or replace) a tag at the very start of the line. */
export function setLeadingTag(text, name, value) {
  const m = /^\{([^}]*)\}/.exec(text);
  const tags = m ? parseBlock(m[1]) : [];
  const rest = m ? text.slice(m[0].length) : text;
  const idx = tags.findIndex((t) => t.name === name);
  if (idx >= 0) tags[idx].value = value; else tags.unshift({ name, value });
  return "{" + stringifyTags(tags) + "}" + rest;
}

/** Parse &HBBGGRR& / &HAABBGGRR into {r,g,b,a}. */
export function parseColor(s) {
  const h = /([0-9a-f]{1,8})/i.exec(String(s).replace(/^&?H/i, ""))?.[1] ?? "0";
  const p = h.padStart(8, "0");
  return { a: parseInt(p.slice(0, 2), 16), b: parseInt(p.slice(2, 4), 16), g: parseInt(p.slice(4, 6), 16), r: parseInt(p.slice(6, 8), 16) };
}
export const toAssColor = ({ r, g, b }) => "&H" + [b, g, r].map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).toUpperCase().padStart(2, "0")).join("") + "&";
