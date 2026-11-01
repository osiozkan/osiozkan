"""Time and frame helpers. All internal times are integer milliseconds."""
from __future__ import annotations
import re

_ASS_T = re.compile(r"^\s*(-?\d+):(\d{1,2}):(\d{1,2})(?:[.:](\d{1,3}))?\s*$")


def ass_to_ms(s: str) -> int:
    m = _ASS_T.match(s)
    if not m:
        raise ValueError(f"bad ASS time: {s!r}")
    h, mi, se, frac = m.groups()
    ms = 0
    if frac:
        ms = int(frac.ljust(3, "0")[:3]) if len(frac) != 2 else int(frac) * 10
    return ((int(h) * 60 + int(mi)) * 60 + int(se)) * 1000 + ms


def ms_to_ass(ms: int) -> str:
    """H:MM:SS.cc, rounded to nearest centisecond like Aegisub."""
    ms = max(0, int(ms))
    cs = (ms + 5) // 10
    h, rem = divmod(cs, 360000)
    m, rem = divmod(rem, 6000)
    s, c = divmod(rem, 100)
    return f"{h:d}:{m:02d}:{s:02d}.{c:02d}"


def ms_to_srt(ms: int, sep: str = ",") -> str:
    ms = max(0, int(ms))
    h, rem = divmod(ms, 3600000)
    m, rem = divmod(rem, 60000)
    s, x = divmod(rem, 1000)
    return f"{h:02d}:{m:02d}:{s:02d}{sep}{x:03d}"


def srt_to_ms(s: str) -> int:
    m = re.match(r"^\s*(\d+):(\d{1,2}):(\d{1,2})[,.](\d{1,3})\s*$", s)
    if not m:
        raise ValueError(f"bad SRT time: {s!r}")
    h, mi, se, f = m.groups()
    return ((int(h) * 60 + int(mi)) * 60 + int(se)) * 1000 + int(f.ljust(3, "0"))


def ms_to_frames(ms: int, fps: float) -> int:
    """Frame whose display interval contains ms (frame n shows [n/fps, (n+1)/fps))."""
    return int(ms * fps / 1000.0 + 1e-9)


def frames_to_ms(frame: int, fps: float) -> int:
    return int(round(frame * 1000.0 / fps))


def start_ms_for_frame(frame: int, fps: float) -> int:
    """Aegisub convention: start times land halfway inside the frame -> exact display."""
    return int((frames_to_ms(frame, fps) + frames_to_ms(frame - 1, fps)) / 2 + 0.5) if frame > 0 else 0


def end_ms_for_frame(frame: int, fps: float) -> int:
    return int((frames_to_ms(frame, fps) + frames_to_ms(frame + 1, fps)) / 2 + 0.5) if frame >= 0 else 0


def parse_time_any(s: str, fps: float | None = None) -> int:
    s = s.strip()
    if re.fullmatch(r"-?\d+", s) and fps:
        return frames_to_ms(int(s), fps)
    if "," in s:
        return srt_to_ms(s)
    return ass_to_ms(s)
