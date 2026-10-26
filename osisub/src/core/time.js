// Time helpers. All internal times are integer milliseconds.

/** ASS time "H:MM:SS.cc" -> ms */
export function parseAssTime(s) {
  const m = /^\s*(-?\d+):(\d{1,2}):(\d{1,2})(?:[.,](\d{1,3}))?\s*$/.exec(s);
  if (!m) return 0;
  const frac = m[4] ? Number((m[4] + "00").slice(0, 3)) : 0;
  return ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + frac;
}

/** ms -> ASS time (centisecond precision, rounded like Aegisub) */
export function formatAssTime(ms) {
  ms = Math.max(0, Math.round(ms / 10) * 10);
  const cs = Math.floor((ms % 1000) / 10);
  const s = Math.floor(ms / 1000) % 60;
  const m = Math.floor(ms / 60000) % 60;
  const h = Math.floor(ms / 3600000);
  return `${h}:${pad(m)}:${pad(s)}.${pad(cs)}`;
}

/** "HH:MM:SS,mmm" or "HH:MM:SS.mmm" (also MM:SS.mmm for VTT) -> ms */
export function parseSrtTime(s) {
  const m = /^\s*(?:(\d+):)?(\d{1,2}):(\d{1,2})[.,](\d{1,3})\s*$/.exec(s);
  if (!m) return 0;
  return ((+(m[1] || 0) * 60 + +m[2]) * 60 + +m[3]) * 1000 + Number((m[4] + "00").slice(0, 3));
}

export function formatSrtTime(ms, sep = ",") {
  ms = Math.max(0, Math.round(ms));
  const h = Math.floor(ms / 3600000);
  const m = Math.floor(ms / 60000) % 60;
  const s = Math.floor(ms / 1000) % 60;
  return `${pad(h)}:${pad(m)}:${pad(s)}${sep}${String(ms % 1000).padStart(3, "0")}`;
}

export const pad = (n, w = 2) => String(n).padStart(w, "0");

/** Frame <-> ms with a constant frame rate (Aegisub semantics: frame start times, rounded). */
export function frameFromMs(ms, fps) {
  return Math.floor((ms * fps) / 1000 + 1e-6);
}
export function msFromFrame(frame, fps) {
  return Math.round((frame * 1000) / fps);
}
/** Aegisub-style start time for a frame: midpoint-safe rounding up to the next cs. */
export function startMsForFrame(frame, fps) {
  return Math.ceil(((frame * 1000) / fps) / 10) * 10;
}
