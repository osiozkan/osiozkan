// Batch text/timing fixes (Subtitle Edit-style). Pure functions on the shared line model.
import { stripTags, splitBlocks } from './tags.js';

const isEv = l => l.class === 'dialogue';

/** Re-break text into at most `maxLines` balanced lines of <= maxLen visible chars. Keeps override tags attached to their text. */
export function balanceText(text, maxLen = 42, maxLines = 2) {
  const flat = text.replace(/\\N|\\n/g, ' ').replace(/\s+/g, ' ').trim();
  const plain = stripTags(flat);
  if (plain.length <= maxLen && !/\\N/.test(text)) return flat;
  const words = flat.split(' '); // tags have no spaces in practice; if they do the split is still lossless on rejoin
  const vis = w => w.replace(/\{[^}]*\}/g, '').length;
  const total = words.reduce((a, w) => a + vis(w), 0) + words.length - 1;
  const n = Math.min(maxLines, Math.max(1, Math.ceil(total / maxLen)));
  if (n === 1) return words.join(' ');
  const target = total / n; const out = []; let cur = [], len = 0;
  for (const w of words) {
    const add = vis(w) + (cur.length ? 1 : 0);
    if (cur.length && out.length < n - 1 && Math.abs(len + add - target) > Math.abs(len - target)) { out.push(cur.join(' ')); cur = [w]; len = vis(w); }
    else { cur.push(w); len += add; }
  }
  out.push(cur.join(' '));
  return out.join('\\N');
}

/** Split lines longer than maxDur or maxChars at sentence/space boundaries, distributing time by character count. Returns new array. */
export function splitLongLines(lines, { maxChars = 84 } = {}) {
  const out = [];
  for (const l of lines) {
    const plain = stripTags(l.text);
    if (!isEv(l) || l.comment || plain.length <= maxChars || /\\p\d/.test(l.text) || /\\k/.test(l.text)) { out.push(l); continue; }
    const parts = []; let rest = l.text.replace(/\\N/g, ' ');
    while (stripTags(rest).length > maxChars) {
      let cut = -1; const s = rest.slice(0, Math.min(rest.length, maxChars + 20));
      for (const re of [/[.!?…]["')\]]?\s/g, /[,;:]\s/g, /\s/g]) { let m, best = -1; while ((m = re.exec(s))) if (m.index + m[0].length <= maxChars + 10) best = m.index + m[0].length; if (best > maxChars * 0.3) { cut = best; break; } }
      if (cut < 0) cut = Math.min(rest.length, maxChars);
      parts.push(rest.slice(0, cut).trim()); rest = rest.slice(cut).trim();
    }
    if (rest) parts.push(rest);
    const weights = parts.map(p => Math.max(1, stripTags(p).length)); const sum = weights.reduce((a, b) => a + b, 0); let t = l.start_time;
    parts.forEach((p, i) => { const d = Math.round(((l.end_time - l.start_time) * weights[i]) / sum); const end = i === parts.length - 1 ? l.end_time : t + d; out.push({ ...l, text: p, start_time: t, end_time: end }); t = end; });
  }
  return out;
}

/** Merge consecutive same-style lines that are short, close together and have the same actor. */
export function mergeShortLines(lines, { maxGap = 250, maxChars = 60, maxDur = 6000 } = {}) {
  const out = [];
  for (const l of lines) {
    const p = out[out.length - 1];
    if (p && isEv(l) && isEv(p) && !l.comment && !p.comment && p.style === l.style && p.actor === l.actor && l.start_time - p.end_time >= 0 && l.start_time - p.end_time <= maxGap
      && stripTags(p.text).length + stripTags(l.text).length + 1 <= maxChars && l.end_time - p.start_time <= maxDur && !/\\k|\\p\d|\\pos|\\move/.test(p.text + l.text)) {
      out[out.length - 1] = { ...p, end_time: l.end_time, text: p.text + '\\N' + l.text }; continue;
    }
    out.push(l);
  }
  return out;
}

/** Remove hearing-impaired annotations: [music], (laughs), ♪ lines, and leading "NAME:" speaker labels. */
export function removeHI(text, { speakers = true } = {}) {
  let t = text.replace(/\s*[\[(（［][^\])）］]*[\])）］]\s*/g, ' ').replace(/^\s*[♪♫].*?[♪♫]?\s*$/g, '');
  if (speakers) t = t.replace(/(^|\\N)\s*[\p{Lu}][\p{Lu}\s.'-]{1,20}:\s*/gu, '$1');
  return t.replace(/\\N\s*\\N/g, '\\N').replace(/^(\\N|\s)+|(\\N|\s)+$/g, '').replace(/ {2,}/g, ' ').trim();
}

/** Typography fixes: spacing around punctuation, ellipsis, repeated punctuation, dash style. */
export function fixPunctuation(text, { lang = 'en' } = {}) {
  return text.split(/(\{[^}]*\})/).map((seg, i) => (i % 2 ? seg : seg
    .replace(/\s+([,.;:!?…])/g, '$1').replace(/([,;:!?])(?=[\p{L}])/gu, '$1 ')
    .replace(/\.{3,}|…{2,}/g, '…').replace(/!{2,}\?*|\?{2,}!*/g, m => (m.includes('!') && m.includes('?') ? '?!' : m[0]))
    .replace(/ {2,}/g, ' ').replace(/(^|\\N)\s+/g, '$1').replace(/\s+(\\N)/g, '$1')
    .replace(/^-(?=\S)/g, '- ').replace(/(\\N)-(?=\S)/g, '$1- '))).join('');
}

/** Remove exact duplicate dialogue lines (same time, style, text). */
export function removeDuplicates(lines) {
  const seen = new Set();
  return lines.filter(l => { if (!isEv(l)) return true; const k = `${l.start_time}|${l.end_time}|${l.layer}|${l.style}|${l.comment}|${l.text}`; if (seen.has(k)) return false; seen.add(k); return true; });
}

/** Ensure at least `minGap` ms between consecutive same-layer lines by trimming the earlier line's end (never below minDur). */
export function enforceGap(lines, minGap = 42, minDur = 100) {
  const ev = lines.filter(l => isEv(l) && !l.comment).sort((a, b) => a.start_time - b.start_time);
  for (let i = 1; i < ev.length; i++) { const p = ev[i - 1], c = ev[i];
    if (p.layer === c.layer && p.style === c.style && c.start_time - p.end_time < minGap && c.start_time > p.start_time) p.end_time = Math.max(p.start_time + minDur, c.start_time - minGap); }
  return lines;
}

/** Snap line boundaries to nearest keyframe within `threshold` ms (start) / lead-out (end), like Aegisub's timing post-processor. */
export function snapToKeyframes(lines, keyframes, { startThreshold = 250, endThreshold = 250 } = {}) {
  if (!keyframes?.length) return lines;
  const near = (t, th) => { let best = null, d = th + 1; for (const k of keyframes) { const dd = Math.abs(k - t); if (dd < d) { d = dd; best = k; } } return d <= th ? best : null; };
  for (const l of lines) if (isEv(l) && !l.comment) { const s = near(l.start_time, startThreshold); if (s != null) l.start_time = s; const e = near(l.end_time, endThreshold); if (e != null && e > l.start_time) l.end_time = e; }
  return lines;
}

export { splitBlocks };
