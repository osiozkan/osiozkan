import { stripTags, visibleLines, charCount, parseTags, splitBlocks } from './tags.js';

export const DEFAULT_QC = { maxCps: 20, minDur: 500, maxDur: 7000, maxLineLen: 42, maxLines: 2, minGap: 0 };

/** Returns [{index, severity, code, message}] for dialogue lines. */
export function runQC(lines, styleNames, opts = {}) {
  const o = { ...DEFAULT_QC, ...opts }; const issues = [];
  const names = new Set(styleNames ?? lines.filter(l => l.class === 'style').map(l => l.name));
  const ds = lines.map((l, i) => [l, i]).filter(([l]) => l.class === 'dialogue' && !l.comment);
  const add = (i, severity, code, message) => issues.push({ index: i, severity, code, message });
  const sorted = [...ds].sort((a, b) => a[0].start_time - b[0].start_time);
  ds.forEach(([l, i]) => {
    const dur = l.end_time - l.start_time, plain = stripTags(l.text);
    if (dur <= 0) add(i, 'error', 'dur-neg', 'Zero or negative duration');
    else {
      if (dur < o.minDur) add(i, 'warn', 'dur-short', `Short duration (${dur} ms)`);
      if (dur > o.maxDur) add(i, 'warn', 'dur-long', `Long duration (${dur} ms)`);
      const c = charCount(l.text) * 1000 / dur;
      if (c > o.maxCps && !/\\p\d/.test(l.text)) add(i, 'warn', 'cps', `${c.toFixed(1)} chars/sec`);
    }
    if (!plain.trim() && !/\\p\d/.test(l.text)) add(i, 'warn', 'empty', 'Empty text');
    const vl = visibleLines(l.text);
    if (vl.length > o.maxLines) add(i, 'warn', 'lines', `${vl.length} lines`);
    if (vl.some(x => x.length > o.maxLineLen)) add(i, 'info', 'linelen', `Line longer than ${o.maxLineLen} chars`);
    if (!names.has(l.style)) add(i, 'error', 'style', `Unknown style "${l.style}"`);
    if (/ {2,}/.test(plain)) add(i, 'info', 'spaces', 'Double spaces');
    if (/^\s|\s$/.test(plain) && plain.trim()) add(i, 'info', 'trim', 'Leading/trailing space');
    let depth = 0; for (const ch of l.text) { if (ch === '{') depth++; else if (ch === '}') depth--; if (depth < 0 || depth > 1) break; }
    if (depth !== 0) add(i, 'error', 'braces', 'Unbalanced { }');
    for (const bl of splitBlocks(l.text)) if (bl.type === 'tags' && /\\(i|b|u|s)[^0-9a-zA-Z]/.test(bl.value + ' ')) add(i, 'info', 'tag-arg', 'Tag missing argument');
  });
  for (let k = 1; k < sorted.length; k++) {
    const [p] = sorted[k - 1], [c, ci] = sorted[k];
    if (c.layer === p.layer && c.style === p.style && c.start_time < p.end_time) add(ci, 'warn', 'overlap', 'Overlaps previous line');
    else if (c.start_time - p.end_time < o.minGap && c.start_time >= p.end_time) add(ci, 'info', 'gap', 'Gap smaller than minimum');
  }
  return issues;
}
