"""Frame <-> time mapping for CFR and VFR video (Matroska timecode v1/v2)."""
from __future__ import annotations

import bisect
from dataclasses import dataclass


class Timecodes:
    """frame n is displayed during [t[n], t[n+1])."""

    def __init__(self, times_ms: list[float] | None = None, fps: float | None = None):
        self.fps = fps
        self.times = times_ms  # None -> CFR via fps
        if times_ms is None and not fps:
            raise ValueError("need fps or times")

    # -- constructors
    @classmethod
    def cfr(cls, fps: float) -> "Timecodes":
        return cls(None, fps)

    @classmethod
    def from_v2(cls, text_or_list) -> "Timecodes":
        if isinstance(text_or_list, str):
            ts = []
            for l in text_or_list.splitlines():
                l = l.strip()
                if not l or l.startswith("#"):
                    continue
                ts.append(float(l))
        else:
            ts = [float(x) for x in text_or_list]
        ts.sort()
        if len(ts) < 2:
            raise ValueError("timecode file too short")
        avg = (ts[-1] - ts[0]) * 1000.0 / (len(ts) - 1) if False else (len(ts) - 1) * 1000.0 / (ts[-1] - ts[0])
        return cls(ts, avg)

    @classmethod
    def from_v1(cls, text: str) -> "Timecodes":
        assume = None
        ranges = []
        for l in text.splitlines():
            l = l.strip()
            if not l or l.startswith("#"):
                continue
            if l.lower().startswith("assume"):
                assume = float(l.split()[1])
                continue
            a, b, f = l.split(",")
            ranges.append((int(a), int(b), float(f)))
        if assume is None:
            raise ValueError("v1 timecodes need 'Assume FPS'")
        ts = []
        t = 0.0
        frame = 0
        ranges.sort()
        last = ranges[-1][1] if ranges else -1
        ri = 0
        while frame <= last:
            while ri < len(ranges) and ranges[ri][1] < frame:
                ri += 1
            f = ranges[ri][2] if ri < len(ranges) and ranges[ri][0] <= frame <= ranges[ri][1] else assume
            ts.append(t)
            t += 1000.0 / f
            frame += 1
        ts.append(t)
        return cls(ts, assume)

    @classmethod
    def from_text(cls, text: str) -> "Timecodes":
        head = text.lstrip().lower()
        if "timecode format v1" in head[:60] or head.startswith("assume"):
            return cls.from_v1(text)
        return cls.from_v2(text)

    # -- queries
    @property
    def is_vfr(self) -> bool:
        return self.times is not None

    def frame_count(self) -> int | None:
        return len(self.times) if self.times is not None else None

    def time_of(self, frame: int) -> int:
        """Start time (ms) of `frame`; extrapolates beyond known frames."""
        if self.times is None:
            return int(round(frame * 1000.0 / self.fps))
        n = len(self.times)
        if frame < 0:
            return int(round(frame * 1000.0 / self.fps))
        if frame >= n:
            return int(round(self.times[-1] + (frame - (n - 1)) * 1000.0 / self.fps))
        return int(round(self.times[frame]))

    def frame_at(self, ms: float) -> int:
        """Frame being displayed at time ms."""
        if self.times is None:
            return int(ms * self.fps / 1000.0 + 1e-9) if ms >= 0 else -int((-ms * self.fps / 1000.0) + 1 - 1e-9)
        if ms < 0:
            return int(ms * self.fps / 1000.0) - 1
        if ms >= self.times[-1]:
            return len(self.times) - 1 + int((ms - self.times[-1]) * self.fps / 1000.0 + 1e-9)
        return bisect.bisect_right(self.times, ms + 1e-6) - 1

    # Aegisub semantics: start times round to the middle between previous frame and this one so the
    # line appears exactly on `frame`; end times to the middle between `frame` and the next.
    def start_ms_for_frame(self, frame: int) -> int:
        if frame <= 0:
            return 0
        return int((self.time_of(frame - 1) + self.time_of(frame)) / 2.0 + 0.5)

    def end_ms_for_frame(self, frame: int) -> int:
        return int((self.time_of(frame) + self.time_of(frame + 1)) / 2.0 + 0.5)

    def frame_start_aligned(self, ms: int) -> int:
        """Snap an arbitrary start time to the Aegisub-style time of the frame it appears on."""
        return self.start_ms_for_frame(self.frame_at(ms))
