"""Time helpers. All internal times are integer milliseconds."""
from __future__ import annotations
import re

_ASS_T = re.compile(r"^\s*(-?\d+):(\d+):(\d+)(?:[.,](\d+))?\s*$")


def parse_ass_time(s: str) -> int:
    m = _ASS_T.match(s)
    if not m:
        raise ValueError(f"bad ASS time: {s!r}")
    h, mi, se, frac = m.groups()
    ms = 0
    if frac:
        ms = int((frac + "00")[:3]) if len(frac) <= 3 else int(frac[:3])
        if len(frac) == 2:
            ms = int(frac) * 10
    return ((int(h) * 60 + int(mi)) * 60 + int(se)) * 1000 + ms


def ass_time(ms: int) -> str:
    """h:mm:ss.cc with Aegisub style rounding to centiseconds."""
    ms = max(0, int(ms))
    cs = (ms + 5) // 10
    h, cs = divmod(cs, 360000)
    m, cs = divmod(cs, 6000)
    s, cs = divmod(cs, 100)
    return f"{h}:{m:02d}:{s:02d}.{cs:02d}"


def srt_time(ms: int) -> str:
    ms = max(0, int(ms))
    h, ms = divmod(ms, 3600000)
    m, ms = divmod(ms, 60000)
    s, ms = divmod(ms, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def vtt_time(ms: int) -> str:
    return srt_time(ms).replace(",", ".")


_SRT_T = re.compile(r"(\d+):(\d+):(\d+)[,.](\d{1,3})")


def parse_srt_time(s: str) -> int:
    m = _SRT_T.search(s)
    if not m:
        raise ValueError(f"bad time: {s!r}")
    h, mi, se, f = m.groups()
    return ((int(h) * 60 + int(mi)) * 60 + int(se)) * 1000 + int((f + "00")[:3])


# --- frame conversion (Aegisub semantics) ---------------------------------
# Aegisub maps a time to the frame that is *displayed* at that time, and for
# start times it uses the first frame whose display begins at/after the time.
class FrameRate:
    def __init__(self, fps: float = 23.976023976023978):
        self.fps = float(fps)

    def frame_at(self, ms: int) -> int:
        """Frame displayed at time ms (frame f spans start_ms(f)..start_ms(f+1))."""
        f = max(0, int(round(ms * self.fps / 1000.0)))
        while f > 0 and self.start_ms(f) > ms:
            f -= 1
        while self.start_ms(f + 1) <= ms:
            f += 1
        return f

    def time_at(self, frame: int) -> int:
        """Start time of a frame (ms, rounded)."""
        return int(round(frame * 1000.0 / self.fps))

    # Aegisub: START uses (frame start + next frame start)/2 so that the
    # time lies safely inside the frame; END uses the same on frame-1.
    def start_ms(self, frame: int) -> int:
        a = self.time_at(frame - 1) if frame > 0 else 0
        b = self.time_at(frame)
        return (a + b + 1) // 2 if frame > 0 else 0

    def end_ms(self, frame: int) -> int:
        return self.start_ms(frame + 1)

    def snap_start(self, ms: int) -> int:
        """Nearest frame-aligned start time at or after ms."""
        f = self.frame_at(ms)
        while self.start_ms(f) < ms:
            f += 1
        return self.start_ms(f)

    def snap_end(self, ms: int) -> int:
        """Nearest frame-aligned end time (by distance)."""
        f = self.frame_at(ms)
        return min((self.end_ms(x) for x in (f - 1, f, f + 1) if x >= 0),
                   key=lambda t: abs(t - ms))
