import { parseAssTime, formatAssTime } from './time.js';

export const STYLE_FIELDS = ['name', 'fontname', 'fontsize', 'color1', 'color2', 'color3', 'color4', 'bold', 'italic',
  'underline', 'strikeout', 'scale_x', 'scale_y', 'spacing', 'angle', 'borderstyle', 'outline', 'shadow', 'align',
  'margin_l', 'margin_r', 'margin_t', 'encoding'];
const STYLE_NUM = new Set(['fontsize', 'scale_x', 'scale_y', 'spacing', 'angle', 'borderstyle', 'outline', 'shadow', 'align', 'margin_l', 'margin_r', 'margin_t', 'encoding']);
const STYLE_BOOL = new Set(['bold', 'italic', 'underline', 'strikeout']);
const DEFAULT_STYLE_FORMAT = 'Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding';
const EVENT_FORMAT = 'Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text';

// Map v4+ style columns -> our field names
const COLMAP = { name: 'name', fontname: 'fontname', fontsize: 'fontsize', primarycolour: 'color1', secondarycolour: 'color2',
  outlinecolour: 'color3', tertiarycolour: 'color3', backcolour: 'color4', bold: 'bold', italic: 'italic', underline: 'underline',
  strikeout: 'strikeout', scalex: 'scale_x', scaley: 'scale_y', spacing: 'spacing', angle: 'angle', borderstyle: 'border' + 'style',
  outline: 'outline', shadow: 'shadow', alignment: 'align', marginl: 'margin_l', marginr: 'margin_r', marginv: 'margin_t', encoding: 'encoding' };

export function defaultStyle(name = 'Default') {
  return { class: 'style', name, fontname: 'Arial', fontsize: 48, color1: '&H00FFFFFF', color2: '&H000000FF', color3: '&H00000000', color4: '&H00000000',
    bold: false, italic: false, underline: false, strikeout: false, scale_x: 100, scale_y: 100, spacing: 0, angle: 0, borderstyle: 1,
    outline: 2, shadow: 2, align: 2, margin_l: 10, margin_r: 10, margin_t: 10, encoding: 1 };
}
export function defaultDialogue(o = {}) {
  return { class: 'dialogue', comment: false, layer: 0, start_time: 0, end_time: 0, style: 'Default', actor: '', margin_l: 0, margin_r: 0, margin_t: 0, effect: '', text: '', ...o };
}

/** Parse an ASS/SSA document into a flat line array (Aegisub `subs` layout). */
export function parseAss(text) {
  text = text.replace(/^﻿/, '');
  const lines = [];
  let section = '', styleFmt = null, evFmt = null;
  for (const raw of text.split(/\r\n|\n|\r/)) {
    const line = raw.trimEnd();
    if (!line.trim()) continue;
    const sec = /^\[(.+)\]$/.exec(line.trim());
    if (sec) { section = sec[1]; continue; }
    if (line.startsWith(';')) { lines.push(section.toLowerCase() === 'script info' ? { class: 'info', key: line, value: '', section } : { class: 'raw', raw: line, section }); continue; }
    const kv = /^([^:]+):\s?(.*)$/.exec(line);
    if (!kv) { lines.push({ class: 'raw', raw: line, section }); continue; }
    const key = kv[1].trim(), val = kv[2];
    const lsec = section.toLowerCase();
    if (lsec === 'script info') {
      lines.push({ class: 'info', key, value: val.trim(), section });
    } else if (lsec.startsWith('v4')) {
      if (key.toLowerCase() === 'format') styleFmt = val.split(',').map(s => s.trim().toLowerCase());
      else if (key.toLowerCase() === 'style') lines.push(parseStyle(val, styleFmt || DEFAULT_STYLE_FORMAT.toLowerCase().split(', '), section));
      else lines.push({ class: 'raw', raw: line, section });
    } else if (lsec === 'events') {
      const k = key.toLowerCase();
      if (k === 'format') evFmt = val.split(',').map(s => s.trim().toLowerCase());
      else if (k === 'dialogue' || k === 'comment') lines.push(parseEvent(val, evFmt || EVENT_FORMAT.toLowerCase().split(', '), k === 'comment', section));
      else lines.push({ class: 'raw', raw: line, section });
    } else lines.push({ class: 'raw', raw: line, section });
  }
  return lines;
}

function parseStyle(val, fmt, section) {
  const parts = splitN(val, fmt.length);
  const st = defaultStyle(''); st.section = section;
  fmt.forEach((col, i) => {
    const f = COLMAP[col]; if (!f) return;
    let v = (parts[i] ?? '').trim();
    if (STYLE_BOOL.has(f)) st[f] = v === '-1' || v === '1';
    else if (STYLE_NUM.has(f)) st[f] = parseFloat(v) || 0;
    else st[f] = v;
  });
  return st;
}
function parseEvent(val, fmt, comment, section) {
  const parts = splitN(val, fmt.length);
  const ev = defaultDialogue({ comment, section });
  fmt.forEach((col, i) => {
    const v = parts[i] ?? '';
    switch (col) {
      case 'layer': case 'marked': ev.layer = parseInt(v.replace(/\D/g, '')) || 0; break;
      case 'start': ev.start_time = parseAssTime(v); break;
      case 'end': ev.end_time = parseAssTime(v); break;
      case 'style': ev.style = v.trim().replace(/^\*/, ''); break;
      case 'name': case 'actor': ev.actor = v; break;
      case 'marginl': ev.margin_l = parseInt(v) || 0; break;
      case 'marginr': ev.margin_r = parseInt(v) || 0; break;
      case 'marginv': ev.margin_t = parseInt(v) || 0; break;
      case 'effect': ev.effect = v; break;
      case 'text': ev.text = v; break;
    }
  });
  return ev;
}
// split on commas into n parts; last part keeps remaining commas (Text field)
function splitN(s, n) {
  const out = []; let rest = s;
  for (let i = 0; i < n - 1; i++) {
    const k = rest.indexOf(','); if (k < 0) { out.push(rest); rest = ''; continue; }
    out.push(rest.slice(0, k)); rest = rest.slice(k + 1);
  }
  out.push(rest); return out;
}

const b = v => (v ? '-1' : '0');
export function styleToString(s) {
  return `Style: ${[s.name, s.fontname, s.fontsize, s.color1, s.color2, s.color3, s.color4, b(s.bold), b(s.italic), b(s.underline), b(s.strikeout),
    s.scale_x, s.scale_y, s.spacing, s.angle, s.borderstyle, s.outline, s.shadow, s.align, s.margin_l, s.margin_r, s.margin_t, s.encoding].join(',')}`;
}
export function eventToString(e) {
  return `${e.comment ? 'Comment' : 'Dialogue'}: ${e.layer},${formatAssTime(e.start_time)},${formatAssTime(e.end_time)},${e.style},${e.actor},${e.margin_l},${e.margin_r},${e.margin_t},${e.effect},${e.text}`;
}

/** Serialize lines back to ASS. Sections follow line classes; Format lines are always regenerated canonically. */
export function writeAss(lines) {
  const out = []; let cur = null;
  const header = name => { if (out.length) out.push(''); out.push(`[${name}]`); cur = name; };
  for (const l of lines) {
    const sec = l.class === 'info' ? 'Script Info' : l.class === 'style' ? (l.section || 'V4+ Styles') : l.class === 'dialogue' ? 'Events' : l.section;
    if (sec !== cur) {
      header(sec);
      if (l.class === 'style') out.push('Format: ' + DEFAULT_STYLE_FORMAT);
      if (l.class === 'dialogue') out.push('Format: ' + EVENT_FORMAT);
    }
    if (l.class === 'info') out.push(l.key.startsWith(';') ? l.key : `${l.key}: ${l.value}`);
    else if (l.class === 'style') out.push(styleToString(l));
    else if (l.class === 'dialogue') out.push(eventToString(l));
    else out.push(l.raw ?? '');
  }
  // an empty Events section still needs its Format line (new/empty scripts)
  if (!lines.some(l => l.class === 'dialogue')) { header('Events'); out.push('Format: ' + EVENT_FORMAT); }
  return out.join('\r\n') + '\r\n';
}

export function newScript() {
  return parseAss(`[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: TV.709\n\n[V4+ Styles]\n${'Format: ' + DEFAULT_STYLE_FORMAT}\n${styleToString(defaultStyle())}\n\n[Events]\n${'Format: ' + EVENT_FORMAT}\n`);
}
export const getInfo = (lines, key) => lines.find(l => l.class === 'info' && l.key.toLowerCase() === key.toLowerCase())?.value;
export function setInfo(lines, key, value) {
  const l = lines.find(l => l.class === 'info' && l.key.toLowerCase() === key.toLowerCase());
  if (l) l.value = String(value);
  else { let i = lines.map(x => x.class).lastIndexOf('info'); lines.splice(i + 1, 0, { class: 'info', key, value: String(value), section: 'Script Info' }); }
}
export const dialogues = lines => lines.filter(l => l.class === 'dialogue');
export const styles = lines => lines.filter(l => l.class === 'style');
