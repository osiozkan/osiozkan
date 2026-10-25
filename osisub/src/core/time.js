// Time helpers. All internal times are integer milliseconds.
export function parseAssTime(s) {
  const m = /^\s*(\d+):(\d{1,2}):(\d{1,2})[.](\d{1,3})\s*$/.exec(s);
  if (!m) return 0;
  const frac = m[4].padEnd(3, '0');
  return ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + +frac;
}
export function formatAssTime(ms) {
  ms = Math.max(0, Math.round(ms));
  const cs = Math.round(ms / 10); // ASS has centisecond precision
  const h = Math.floor(cs / 360000), m = Math.floor(cs / 6000) % 60, s = Math.floor(cs / 100) % 60;
  return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}.${String(cs % 100).padStart(2, '0')}`;
}
export function parseSrtTime(s) {
  const m = /(\d+):(\d{2}):(\d{2})[,.](\d{1,3})/.exec(s);
  if (!m) return 0;
  return ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + +m[4].padEnd(3, '0');
}
export function formatSrtTime(ms, sep = ',') {
  ms = Math.max(0, Math.round(ms));
  const h = Math.floor(ms / 3600000), m = Math.floor(ms / 60000) % 60, s = Math.floor(ms / 1000) % 60;
  const p = (n, w = 2) => String(n).padStart(w, '0');
  return `${p(h)}:${p(m)}:${p(s)}${sep}${p(ms % 1000, 3)}`;
}
export const frameToMs = (f, fps) => Math.round((f * 1000) / fps);
export const msToFrame = (ms, fps) => Math.round((ms * fps) / 1000);
