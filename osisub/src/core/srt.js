import { parseSrtTime, formatSrtTime } from './time.js';
import { defaultDialogue } from './ass.js';

const htmlToAss = t => t.replace(/\r?\n/g, '\\N')
  .replace(/<i>/gi, '{\\i1}').replace(/<\/i>/gi, '{\\i0}').replace(/<b>/gi, '{\\b1}').replace(/<\/b>/gi, '{\\b0}')
  .replace(/<u>/gi, '{\\u1}').replace(/<\/u>/gi, '{\\u0}').replace(/<s>/gi, '{\\s1}').replace(/<\/s>/gi, '{\\s0}')
  .replace(/<font[^>]*color="?#?([0-9a-f]{6})"?[^>]*>/gi, (_, c) => `{\\c&H${c.slice(4, 6)}${c.slice(2, 4)}${c.slice(0, 2)}&}`).replace(/<\/font>/gi, '')
  .replace(/<[^>]+>/g, '');
const assToHtml = t => t.replace(/\\[Nn]/g, '\n').replace(/\\h/g, ' ')
  .replace(/\{\\i1\}/g, '<i>').replace(/\{\\i0\}/g, '</i>').replace(/\{\\b1\}/g, '<b>').replace(/\{\\b0\}/g, '</b>')
  .replace(/\{\\u1\}/g, '<u>').replace(/\{\\u0\}/g, '</u>').replace(/\{[^}]*\}/g, '');

export function parseSrt(text) {
  text = text.replace(/^﻿/, '').replace(/^WEBVTT[^\n]*\n/, '');
  const out = [];
  for (const block of text.split(/\r?\n\r?\n+/)) {
    const ls = block.split(/\r?\n/).filter((x, i, a) => !(i === 0 && /^\d+$/.test(x.trim()) && /-->/.test(a[1] || '')));
    const ti = ls.findIndex(l => l.includes('-->')); if (ti < 0) continue;
    const [a, b] = ls[ti].split('-->');
    out.push(defaultDialogue({ start_time: parseSrtTime(a), end_time: parseSrtTime(b), text: htmlToAss(ls.slice(ti + 1).join('\n')) }));
  }
  return out;
}
export function writeSrt(lines) {
  return lines.filter(l => l.class === 'dialogue' && !l.comment).sort((a, b) => a.start_time - b.start_time)
    .map((l, i) => `${i + 1}\r\n${formatSrtTime(l.start_time)} --> ${formatSrtTime(l.end_time)}\r\n${assToHtml(l.text)}\r\n`).join('\r\n');
}
export function writeVtt(lines) {
  return 'WEBVTT\n\n' + lines.filter(l => l.class === 'dialogue' && !l.comment).sort((a, b) => a.start_time - b.start_time)
    .map(l => `${formatSrtTime(l.start_time, '.')} --> ${formatSrtTime(l.end_time, '.')}\n${assToHtml(l.text)}\n`).join('\n');
}
export const writePlain = lines => lines.filter(l => l.class === 'dialogue' && !l.comment).map(l => assToHtml(l.text).replace(/<[^>]+>/g, '')).join('\n');
