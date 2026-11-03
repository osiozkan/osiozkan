"""Timecode (VFR) and keyframe file support."""
from __future__ import annotations
import bisect
import re

from .timeutil import FrameRate


class Timecodes(FrameRate):
    """Variable frame rate from a list of frame start times (ms)."""
    def __init__(self, times: list[float]):
        super().__init__(1000 * (len(times) - 1) / max(1.0, times[-1] - times[0]) if len(times) > 1 else 24.0)
        self.times = [float(t) for t in times]

    def time_at(self, frame: int) -> int:
        if frame < len(self.times):
            return int(round(self.times[max(0, frame)]))
        extra = frame - (len(self.times) - 1)
        step = 1000 / self.fps
        return int(round(self.times[-1] + extra * step))

    def frame_at(self, ms: int) -> int:
        # frame f spans [mid(f-1,f), mid(f,f+1))
        i = bisect.bisect_right(self.times, ms) - 1
        i = max(0, i)
        if i + 1 < len(self.times) and ms >= (self.times[i] + self.times[i + 1]) / 2:
            i += 1
        elif i > 0 and ms < (self.times[i - 1] + self.times[i]) / 2:
            i -= 1
        if ms > self.times[-1]:
            i = len(self.times) - 1 + int((ms - self.times[-1]) * self.fps / 1000 + 0.5)
        return i


def parse_timecodes(text: str) -> Timecodes | FrameRate:
    lines = [l.strip() for l in text.splitlines() if l.strip()]
    head = lines[0].lower()
    if head.startswith("# timecode format v2"):
        return Timecodes([float(l) for l in lines[1:] if not l.startswith("#")])
    if head.startswith("# timecode format v1"):
        assume = 24.0
        segs = []
        for l in lines[1:]:
            if l.lower().startswith("assume"):
                assume = float(l.split()[1])
            elif not l.startswith("#"):
                a, b, f = l.split(","); segs.append((int(a), int(b), float(f)))
        times, t, frame = [], 0.0, 0
        last = max([s[1] for s in segs] + [0])
        for fr in range(last + 1):
            fps = next((s[2] for s in segs if s[0] <= fr <= s[1]), assume)
            times.append(t); t += 1000.0 / fps
        times.append(t)
        return Timecodes(times)
    raise ValueError("unknown timecode format")


def parse_keyframes(text: str) -> list[int]:
    """Aegisub keyframes ('# keyframe format v1', fps, frames...) or x264 stats / plain frame lists -> frame numbers."""
    out = []
    lines = text.splitlines()
    if lines and lines[0].lower().startswith("# keyframe format v1"):
        for l in lines[2:]:
            l = l.strip()
            if l.isdigit(): out.append(int(l))
        return out
    if lines and lines[0].lower().startswith("# xvid 2pass"):
        for n, l in enumerate(l for l in lines if not l.startswith("#")):
            if l.startswith("i"): out.append(n)
        return out
    # x264 stats: "in:N out:N type:I ..."
    for l in lines:
        m = re.search(r"in:(\d+).*type:([IPB])", l)
        if m and m.group(2) == "I": out.append(int(m.group(1)))
    if out: return out
    return [int(l) for l in lines if l.strip().isdigit()]


def write_keyframes(frames: list[int], fps: float) -> str:
    return "# keyframe format v1\nfps " + repr(fps) + "\n" + "\n".join(map(str, frames)) + "\n"
