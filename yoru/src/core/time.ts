/** Zaman yardımcıları: tüm iç zamanlar milisaniye (tamsayı). */

export function parseAssTime(s: string): number {
  const m = /^\s*(-?\d+):(\d{1,2}):(\d{1,2})(?:[.,](\d{1,3}))?\s*$/.exec(s);
  if (!m) return 0;
  const frac = m[4] ? Number((m[4] + "00").slice(0, 3)) : 0;
  return ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + frac;
}

/** ASS santisaniye hassasiyeti (aşağı yuvarlama Aegisub ile aynı). */
export function formatAssTime(ms: number): string {
  ms = Math.max(0, Math.round(ms));
  const cs = Math.floor(ms / 10) % 100;
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600), m = Math.floor(total / 60) % 60, s = total % 60;
  return `${h}:${p2(m)}:${p2(s)}.${p2(cs)}`;
}

export function formatSrtTime(ms: number, sep = ","): string {
  ms = Math.max(0, Math.round(ms));
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600), m = Math.floor(total / 60) % 60, s = total % 60;
  return `${p2(h)}:${p2(m)}:${p2(s)}${sep}${String(ms % 1000).padStart(3, "0")}`;
}

export function parseSrtTime(s: string): number {
  const m = /^\s*(\d+):(\d{1,2}):(\d{1,2})[,.](\d{1,3})\s*$/.exec(s);
  if (!m) return NaN;
  return ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + Number((m[4] + "00").slice(0, 3));
}

/** VTT: hh:mm:ss.mmm veya mm:ss.mmm */
export function parseVttTime(s: string): number {
  const t = s.trim();
  const m = /^(?:(\d+):)?(\d{1,2}):(\d{1,2})\.(\d{1,3})$/.exec(t);
  if (!m) return NaN;
  return (((+(m[1] ?? 0)) * 60 + +m[2]) * 60 + +m[3]) * 1000 + Number((m[4] + "00").slice(0, 3));
}

const p2 = (n: number) => String(n).padStart(2, "0");

/** Kare <-> ms dönüşümü (Aegisub'daki gibi: kare başlangıcı + yarım kare ofseti ile merkezlenir). */
export function frameToMs(frame: number, fps: number): number {
  return Math.round((frame * 1000) / fps);
}
export function msToFrame(ms: number, fps: number): number {
  return Math.floor((ms * fps) / 1000 + 1e-6);
}
/** Başlangıç zamanı: ms'yi, o anda görünen karenin başlangıcına çeker (Aegisub davranışı: ceil). */
export function snapStartToFrame(ms: number, fps: number): number {
  return frameToMs(Math.ceil((ms * fps) / 1000 - 1e-6), fps);
}
/** Bitiş zamanı: son görünen karenin bitişine (floor). */
export function snapEndToFrame(ms: number, fps: number): number {
  return frameToMs(Math.floor((ms * fps) / 1000 + 1e-6), fps);
}
