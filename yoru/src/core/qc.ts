/** Kalite kontrol (Subtitle Edit'in "Fix common errors" + Aegisub zamanlama denetimleri). */
import { AssFile } from "./ass";
import { stripTags, visibleLength } from "./tags";

export type Severity = "error" | "warn" | "info";
export interface Issue { index: number; code: string; severity: Severity; message: string }

export interface QcOptions {
  maxCps: number; minDuration: number; maxDuration: number;
  maxLineLength: number; maxLines: number; minGap: number; fps?: number;
}
export const DEFAULT_QC: QcOptions = { maxCps: 20, minDuration: 500, maxDuration: 7000, maxLineLength: 42, maxLines: 2, minGap: 0 };

export function cps(text: string, durMs: number): number {
  const chars = stripTags(text, true).replace(/\\N/g, "").replace(/\s/g, "").length;
  return durMs > 0 ? (chars * 1000) / durMs : Infinity;
}

export function runQc(f: AssFile, o: QcOptions = DEFAULT_QC): Issue[] {
  const issues: Issue[] = [];
  const styleNames = new Set(f.styles.map((s) => s.name));
  const push = (index: number, code: string, severity: Severity, message: string) => issues.push({ index, code, severity, message });
  f.events.forEach((e, i) => {
    if (e.comment) return;
    const dur = e.end - e.start;
    const plain = stripTags(e.text, true);
    if (dur < 0) push(i, "negative", "error", "Bitiş başlangıçtan önce");
    if (dur === 0 && plain.trim()) push(i, "zero", "error", "Süre sıfır");
    if (!plain.trim() && !/\\p1/.test(e.text) && !e.effect) push(i, "empty", "warn", "Boş satır");
    if (!styleNames.has(e.style) && e.style !== "*Default") push(i, "style", "error", `Tanımsız stil: ${e.style}`);
    if (dur > 0 && dur < o.minDuration && plain.trim()) push(i, "short", "warn", `Çok kısa (${dur} ms)`);
    if (dur > o.maxDuration) push(i, "long", "warn", `Çok uzun (${dur} ms)`);
    const c = cps(e.text, dur);
    if (Number.isFinite(c) && c > o.maxCps && !/\\p1/.test(e.text)) push(i, "cps", "warn", `Okuma hızı ${c.toFixed(1)} cps`);
    const lines = plain.split("\\N");
    if (lines.length > o.maxLines) push(i, "lines", "warn", `${lines.length} satır`);
    for (const l of lines) if (l.length > o.maxLineLength) { push(i, "linelen", "warn", `Satır ${l.length} karakter`); break; }
    if (/ {2,}/.test(plain)) push(i, "spaces", "info", "Ardışık boşluk");
    if (/^\s|\s$/.test(plain)) push(i, "trim", "info", "Baş/son boşluk");
    if ((e.text.match(/\{/g) || []).length !== (e.text.match(/\}/g) || []).length) push(i, "braces", "error", "Eşleşmeyen { }");
  });
  const evs = f.events.map((e, i) => ({ e, i })).filter((x) => !x.e.comment && x.e.text.trim()).sort((a, b) => a.e.start - b.e.start);
  for (let k = 0; k < evs.length - 1; k++) {
    const a = evs[k], b = evs[k + 1];
    if (a.e.style === b.e.style && a.e.layer === b.e.layer && a.e.end > b.e.start && !/\\pos|\\move/.test(a.e.text + b.e.text))
      push(b.i, "overlap", "warn", `Önceki satırla çakışıyor (${a.e.end - b.e.start} ms)`);
    else if (o.minGap > 0 && b.e.start - a.e.end >= 0 && b.e.start - a.e.end < o.minGap && a.e.style === b.e.style)
      push(b.i, "gap", "info", `Satırlar arası boşluk ${b.e.start - a.e.end} ms`);
  }
  return issues.sort((a, b) => a.index - b.index);
}

/** Otomatik düzeltmeler (güvenli olanlar). */
export function autoFix(f: AssFile): number {
  let n = 0;
  for (const e of f.events) {
    const t = e.text;
    let r = t.replace(/ {2,}/g, " ").replace(/^(\s+)(?!\{)/, "").replace(/(\s+)$/, "").replace(/\s+\\N/g, "\\N").replace(/\\N\s+/g, "\\N");
    if (r !== t) { e.text = r; n++; }
    if (e.end < e.start) { e.end = e.start; n++; }
  }
  return n;
}
export { visibleLength };
