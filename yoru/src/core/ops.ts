/** Toplu düzenleme işlemleri: kaydır, böl, birleştir, sırala, bul-değiştir, zamanlama. */
import { AssFile, Event, newEvent } from "./ass";
import { replaceInVisibleText, stripTags } from "./tags";
import { snapEndToFrame, snapStartToFrame } from "./time";

export function sortEvents(f: AssFile, by: "start" | "end" | "style" | "actor" = "start") {
  const idx = new Map(f.events.map((e, i) => [e, i]));
  f.events.sort((a, b) => {
    const k = by === "style" ? a.style.localeCompare(b.style) : by === "actor" ? a.actor.localeCompare(b.actor) : by === "end" ? a.end - b.end : a.start - b.start;
    return k || a.start - b.start || idx.get(a)! - idx.get(b)!;
  });
}

export interface ShiftOpts { ms: number; start?: boolean; end?: boolean; selection?: number[]; fromTime?: number }
export function shiftTimes(f: AssFile, o: ShiftOpts) {
  const sel = o.selection ? new Set(o.selection) : null;
  f.events.forEach((e, i) => {
    if (sel && !sel.has(i)) return;
    if (o.fromTime !== undefined && e.start < o.fromTime) return;
    if (o.start !== false) e.start = Math.max(0, e.start + o.ms);
    if (o.end !== false) e.end = Math.max(e.start, e.end + o.ms);
  });
}

/** İki referans noktasından doğrusal senkron (ofset + hız düzeltmesi). */
export function linearSync(f: AssFile, a: { from: number; to: number }, b: { from: number; to: number }) {
  const k = (b.to - a.to) / (b.from - a.from);
  const map = (t: number) => Math.max(0, Math.round(a.to + (t - a.from) * k));
  for (const e of f.events) { e.start = map(e.start); e.end = Math.max(e.start, map(e.end)); }
}

export function splitEvent(f: AssFile, index: number, at: number, charPos?: number) {
  const e = f.events[index];
  const t = Math.min(Math.max(at, e.start + 1), e.end - 1);
  const plain = e.text;
  let a = plain, b = plain;
  if (charPos !== undefined) { a = plain.slice(0, charPos).trimEnd(); b = plain.slice(charPos).trimStart(); }
  const second: Event = { ...e, start: t, text: b };
  e.end = t; e.text = a;
  f.events.splice(index + 1, 0, second);
}

export function mergeEvents(f: AssFile, indices: number[], sep = "\\N") {
  const idx = [...indices].sort((a, b) => a - b);
  if (idx.length < 2) return;
  const first = f.events[idx[0]];
  first.text = idx.map((i) => f.events[i].text).join(sep);
  first.end = Math.max(...idx.map((i) => f.events[i].end));
  first.start = Math.min(...idx.map((i) => f.events[i].start));
  for (const i of idx.slice(1).reverse()) f.events.splice(i, 1);
}

/** Çakışan ardışık satırları düzeltir (önceki satırın bitişi sonrakinin başlangıcına çekilir). */
export function fixOverlaps(f: AssFile, minGap = 0): number {
  const evs = f.events.filter((e) => !e.comment).sort((a, b) => a.start - b.start);
  let n = 0;
  for (let i = 0; i < evs.length - 1; i++) {
    const a = evs[i], b = evs[i + 1];
    if (a.style === b.style && a.end > b.start - minGap) { a.end = Math.max(a.start + 1, b.start - minGap); n++; }
  }
  return n;
}

/** Kare hassasiyetine hizala. */
export function snapToFrames(f: AssFile, fps: number) {
  for (const e of f.events) { e.start = snapStartToFrame(e.start, fps); e.end = Math.max(e.start, snapEndToFrame(e.end, fps)); }
}

/** Komşu satırlar arasındaki küçük boşlukları (≤ maxGap) kapatır — "chaining". */
export function chainGaps(f: AssFile, maxGap: number): number {
  const evs = f.events.filter((e) => !e.comment).sort((a, b) => a.start - b.start);
  let n = 0;
  for (let i = 0; i < evs.length - 1; i++) {
    const gap = evs[i + 1].start - evs[i].end;
    if (gap > 0 && gap <= maxGap) { evs[i].end = evs[i + 1].start; n++; }
  }
  return n;
}

export interface FindOpts { find: string; replace?: string; regex?: boolean; matchCase?: boolean; wholeWord?: boolean; field?: "text" | "actor" | "style" | "effect"; skipTags?: boolean }
function makeRe(o: FindOpts): RegExp {
  let src = o.regex ? o.find : o.find.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (o.wholeWord) src = `\\b(?:${src})\\b`;
  return new RegExp(src, o.matchCase ? "g" : "gi");
}
export function findAll(f: AssFile, o: FindOpts): number[] {
  const re = makeRe(o); const field = o.field ?? "text";
  const out: number[] = [];
  f.events.forEach((e, i) => {
    const s = field === "text" && o.skipTags ? stripTags(e.text, true) : (e as any)[field] as string;
    re.lastIndex = 0; if (re.test(s)) out.push(i);
  });
  return out;
}
export function replaceAll(f: AssFile, o: FindOpts): number {
  const re = makeRe(o); const field = o.field ?? "text"; let n = 0;
  for (const e of f.events) {
    const cur = (e as any)[field] as string;
    const next = field === "text" && o.skipTags ? replaceInVisibleText(cur, re, o.replace ?? "") : cur.replace(re, o.replace ?? "");
    if (next !== cur) { (e as any)[field] = next; n++; }
  }
  return n;
}

export function duplicateEvent(f: AssFile, index: number) { f.events.splice(index + 1, 0, { ...f.events[index] }); }
export function insertEventAfter(f: AssFile, index: number, durMs = 2000): Event {
  const prev = f.events[index];
  const e = newEvent({ start: prev ? prev.end : 0, end: (prev ? prev.end : 0) + durMs, style: prev?.style ?? "Default" });
  f.events.splice(index + 1, 0, e); return e;
}
