/** Override-tag helpers. */
export const stripTags = t => t.replace(/\{[^}]*\}/g, '').replace(/\\[Nn]/g, ' ').replace(/\\h/g, ' ');
export const visibleLines = t => t.replace(/\{[^}]*\}/g, '').split(/\\N|\\n/);
export function splitBlocks(text) {
  const out = []; const re = /\{([^}]*)\}/g; let last = 0, m;
  while ((m = re.exec(text))) { if (m.index > last) out.push({ type: 'text', value: text.slice(last, m.index) }); out.push({ type: 'tags', value: m[1] }); last = re.lastIndex; }
  if (last < text.length) out.push({ type: 'text', value: text.slice(last) });
  return out;
}
export function parseTags(block) {
  const tags = []; const re = /\\(\d?[a-zA-Z]+)(\([^)]*\)|[^\\(]*)/g; let m;
  while ((m = re.exec(block))) tags.push({ name: m[1], arg: m[2] });
  return tags;
}
export function charCount(text) { return stripTags(text).replace(/\s/g, '').length; }
export function cps(text, ms) { return ms > 0 ? (charCount(text) * 1000) / ms : Infinity; }
/** Karaoke syllables from \k \kf \ko tags: [{dur(ms), text}] */
export function karaokeSyllables(text) {
  const out = []; const re = /\{([^}]*)\}([^{]*)/g; let m;
  while ((m = re.exec(text))) { const k = /\\k[fo]?(\d+)/.exec(m[1]); if (k) out.push({ dur: +k[1] * 10, text: m[2] }); }
  return out;
}
