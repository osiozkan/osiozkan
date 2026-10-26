// Quality checks (Subtitle Edit style "Fix common errors" + anime typesetting sanity).
import { visibleText, isDrawing, splitLines, stripTags } from "./tags.js";

export const DEFAULT_RULES = {
  maxCps: 20, maxCpl: 42, maxLines: 2,
  minDuration: 833, maxDuration: 7000,
  minGap: 0, // ms between consecutive dialogue lines (same layer/style group)
};

/** @returns {{index:number, code:string, severity:'error'|'warn'|'info', message:string}[]} */
export function check(doc, rules = {}) {
  const r = { ...DEFAULT_RULES, ...rules };
  const out = [];
  const styles = new Set(doc.styles.map((s) => s.name));
  const dlg = doc.events.map((e, i) => ({ e, i })).filter((x) => !x.e.comment);
  const add = (i, code, severity, message) => out.push({ index: i, code, severity, message });
  for (const { e, i } of dlg) {
    const dur = e.end - e.start;
    if (dur <= 0) add(i, "bad-time", "error", "End time is not after start time");
    if (!styles.has(e.style)) add(i, "missing-style", "error", `Style "${e.style}" is not defined`);
    if (opens(e.text) !== closes(e.text)) add(i, "brace", "error", "Unbalanced { } override block");
    if (isDrawing(e.text)) continue; // vector drawings are exempt from text rules
    const vis = visibleText(e.text);
    if (!vis) { add(i, "empty", "warn", "Line has no visible text"); continue; }
    if (dur > 0) {
      const cps = vis.replace(/\s/g, "").length / (dur / 1000);
      if (cps > r.maxCps) add(i, "cps", "warn", `${cps.toFixed(1)} chars/s exceeds ${r.maxCps}`);
      if (dur < r.minDuration) add(i, "short", "warn", `Duration ${dur}ms below ${r.minDuration}ms`);
      if (dur > r.maxDuration) add(i, "long", "info", `Duration ${dur}ms above ${r.maxDuration}ms`);
    }
    const ls = splitLines(stripTags(e.text, { keepBreaks: true }));
    if (ls.length > r.maxLines) add(i, "lines", "warn", `${ls.length} lines (max ${r.maxLines})`);
    for (const l of ls) if ([...l.replace(/\\[nh]/g, " ")].length > r.maxCpl) { add(i, "cpl", "warn", `Line longer than ${r.maxCpl} chars`); break; }
    if (/ {2,}/.test(stripTags(e.text, { keepBreaks: true }))) add(i, "double-space", "info", "Double space");
    if (/^\s|\s$/.test(stripTags(e.text, { keepBreaks: true }).replace(/\\N/g, ""))) add(i, "edge-space", "info", "Leading/trailing space");
    if (/\\N\s*\\N/.test(e.text)) add(i, "blank-line", "warn", "Empty line inside text");
  }
  // overlaps within the same layer+style (typesetting on other layers is ignored)
  const byKey = new Map();
  for (const x of dlg) {
    if (isDrawing(x.e.text)) continue;
    const k = x.e.layer + "|" + x.e.style;
    (byKey.get(k) ?? byKey.set(k, []).get(k)).push(x);
  }
  for (const list of byKey.values()) {
    list.sort((a, b) => a.e.start - b.e.start);
    for (let k = 1; k < list.length; k++) {
      const a = list[k - 1], b = list[k];
      if (b.e.start < a.e.end) add(b.i, "overlap", "warn", `Overlaps previous line by ${a.e.end - b.e.start}ms`);
      else if (b.e.start - a.e.end < r.minGap) add(b.i, "gap", "info", `Gap ${b.e.start - a.e.end}ms below ${r.minGap}ms`);
    }
  }
  return out.sort((a, b) => a.index - b.index);
}
const opens = (t) => (t.match(/\{/g) || []).length;
const closes = (t) => (t.match(/\}/g) || []).length;

/** Fix helpers. Each returns number of events changed. */
export const fixes = {
  /** Trim overlaps by pulling the earlier line's end to the later line's start. */
  overlaps(doc, { minGap = 0 } = {}) {
    let n = 0;
    const groups = new Map();
    doc.events.forEach((e) => {
      if (e.comment || isDrawing(e.text)) return;
      const k = e.layer + "|" + e.style;
      (groups.get(k) ?? groups.set(k, []).get(k)).push(e);
    });
    for (const g of groups.values()) {
      g.sort((a, b) => a.start - b.start);
      for (let i = 1; i < g.length; i++) {
        const lim = g[i].start - minGap;
        if (g[i - 1].end > lim && lim > g[i - 1].start) { g[i - 1].end = lim; n++; }
      }
    }
    return n;
  },
  /** Collapse spaces, trim, remove empty \N. */
  whitespace(doc) {
    let n = 0;
    for (const e of doc.events) {
      const t = e.text.replace(/ {2,}/g, " ").replace(/\s*\\N\s*/g, "\\N").replace(/(\\N){2,}/g, "\\N").replace(/^(\s|\\N)+|(\s|\\N)+$/g, "");
      if (t !== e.text) { e.text = t; n++; }
    }
    return n;
  },
  /** Extend short durations up to minDuration if the gap to the next line allows. */
  minDuration(doc, { minDuration = 833 } = {}) {
    let n = 0;
    const evs = doc.events.filter((e) => !e.comment && !isDrawing(e.text)).sort((a, b) => a.start - b.start);
    evs.forEach((e, i) => {
      if (e.end - e.start >= minDuration) return;
      const next = evs.slice(i + 1).find((x) => x.layer === e.layer);
      const lim = next ? next.start : Infinity;
      const ne = Math.min(e.start + minDuration, lim);
      if (ne > e.end) { e.end = ne; n++; }
    });
    return n;
  },
  /** Re-balance two-line text so both lines have similar length. */
  balance(doc, { maxCpl = 42 } = {}) {
    let n = 0;
    for (const e of doc.events) {
      if (e.comment || isDrawing(e.text) || /\{[^}]*\}/.test(e.text)) continue;
      const flat = e.text.replace(/\\N/g, " ").replace(/ {2,}/g, " ").trim();
      if (flat.length <= maxCpl) { if (flat !== e.text && e.text.includes("\\N")) { e.text = flat; n++; } continue; }
      const mid = flat.length / 2;
      let best = -1, bd = Infinity;
      for (let i = 0; i < flat.length; i++) if (flat[i] === " " && Math.abs(i - mid) < bd) { bd = Math.abs(i - mid); best = i; }
      if (best < 0) continue;
      const t = flat.slice(0, best) + "\\N" + flat.slice(best + 1);
      if (t !== e.text) { e.text = t; n++; }
    }
    return n;
  },
  sort(doc) {
    const before = doc.events.map((e) => e);
    doc.events.sort((a, b) => a.start - b.start || a.layer - b.layer);
    return before.some((e, i) => e !== doc.events[i]) ? 1 : 0;
  },
};

export function shift(doc, ms, { indices = null, field = "both" } = {}) {
  doc.events.forEach((e, i) => {
    if (indices && !indices.includes(i)) return;
    if (field !== "end") e.start = Math.max(0, e.start + ms);
    if (field !== "start") e.end = Math.max(0, e.end + ms);
  });
}

/** Linear retime: map [a0,a1] -> [b0,b1] (sync two reference points). */
export function stretch(doc, a0, a1, b0, b1) {
  const k = (b1 - b0) / (a1 - a0);
  const f = (t) => Math.max(0, Math.round(b0 + (t - a0) * k));
  for (const e of doc.events) { e.start = f(e.start); e.end = f(e.end); }
}

/** Frame-rate change (e.g. 23.976 -> 25). */
export function changeFps(doc, from, to) {
  const k = from / to;
  for (const e of doc.events) { e.start = Math.round(e.start * k); e.end = Math.round(e.end * k); }
}

/** Snap start/end to the closest keyframe if within threshold (ms). Keyframes in ms, sorted. */
export function snapToKeyframes(doc, keyframes, { startBefore = 250, startAfter = 250, endBefore = 250, endAfter = 650 } = {}) {
  let n = 0;
  const near = (t, lo, hi) => {
    let best = null;
    for (const k of keyframes) {
      if (k < t - lo) continue;
      if (k > t + hi) break;
      if (best === null || Math.abs(k - t) < Math.abs(best - t)) best = k;
    }
    return best;
  };
  for (const e of doc.events) {
    if (e.comment) continue;
    const s = near(e.start, startBefore, startAfter), en = near(e.end, endBefore, endAfter);
    if (s !== null && s !== e.start) { e.start = s; n++; }
    if (en !== null && en !== e.end) { e.end = en; n++; }
  }
  return n;
}

export function runAllFixes(doc, rules = {}) {
  const r = { ...DEFAULT_RULES, ...rules };
  return {
    sort: fixes.sort(doc),
    whitespace: fixes.whitespace(doc),
    overlaps: fixes.overlaps(doc, { minGap: r.minGap }),
    minDuration: fixes.minDuration(doc, r),
  };
}
