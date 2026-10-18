"""Time helpers. All internal times are integer milliseconds."""
from __future__ import annotations
import re

_ASS_T = re.compile(r"^\s*(-?\d+):(\d+):(\d+)[.:](\d+)\s*$")


def parse_ass_time(s: str) -> int:
    m = _ASS_T.match(s)
    if not m:
        return 0
    h, mi, se, cs = m.groups()
    frac = (cs + "00")[:2] if len(cs) < 2 else cs[:2]
    ms = int(h) * 3600000 + int(mi) * 60000 + int(se) * 1000 + int(frac) * 10
    return ms


def format_ass_time(ms: int) -> str:
    ms = max(0, int(ms))
    cs = (ms + 5) // 10  # round to centiseconds like Aegisub
    h, r = divmod(cs, 360000)
    m, r = divmod(r, 6000)
    s, c = divmod(r, 100)
    return f"{h}:{m:02d}:{s:02d}.{c:02d}"


def format_srt_time(ms: int) -> str:
    ms = max(0, int(ms))
    h, r = divmod(ms, 3600000)
    m, r = divmod(r, 60000)
    s, r = divmod(r, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{r:03d}"


def format_vtt_time(ms: int) -> str:
    return format_srt_time(ms).replace(",", ".")


_SRT_T = re.compile(r"(\d+):(\d+):(\d+)[,.](\d{1,3})")


def parse_srt_time(s: str) -> int:
    m = _SRT_T.search(s)
    if not m:
        return 0
    h, mi, se, f = m.groups()
    return int(h) * 3600000 + int(mi) * 60000 + int(se) * 1000 + int((f + "00")[:3])


class FrameRate:
    """Constant frame rate helper (supports NTSC rationals)."""

    def __init__(self, fps: float = 23.976):
        self.fps = float(fps)

    def frame_to_ms_start(self, frame: int) -> int:
        # Aegisub style: start times land in the middle between previous and this frame
        if frame <= 0:
            return 0
        return int(((frame - 0.5) * 1000.0 / self.fps) + 0.5)

    def frame_to_ms_end(self, frame: int) -> int:
        return self.frame_to_ms_start(frame + 1) if frame >= 0 else 0

    def frame_to_ms(self, frame: int) -> int:
        return int(frame * 1000.0 / self.fps + 0.5)

    def ms_to_frame(self, ms: int) -> int:
        return int(ms * self.fps / 1000.0 + 1e-6)

    def snap(self, ms: int) -> int:
        return self.frame_to_ms(round(ms * self.fps / 1000.0))
