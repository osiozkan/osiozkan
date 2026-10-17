"""Kare <-> milisaniye eşlemesi (Aegisub agi::vfr::Framerate anlamıyla).

Aegisub Lua'da frame_from_ms ve ms_from_frame ikisi de START kipini kullanır:
  FrameAtTime(ms, START) = FrameAtTime(ms - 1, EXACT) + 1
  TimeAtFrame(f, START)  = ts[f-1] ile ts[f] arasının yukarı yuvarlanmış ortası
Böylece bir satırın başlangıç zamanı her zaman kendi karesinin içine düşer.

Kaynak: bplayer kare dizini (pts listesi) ya da sabit fps.
"""

from __future__ import annotations

import bisect

EXACT, START, END = 0, 1, 2


class FrameMap:
    def __init__(self, timestamps_ms: list[int]):
        if len(timestamps_ms) < 2:
            raise ValueError("En az iki kare zamanı gerekir")
        self.ts = [int(round(t)) for t in timestamps_ms]
        last = self.ts[-1] - self.ts[-2]
        n = len(self.ts) - 1
        # Son karenin ötesi için ortalama kare süresi (Aegisub da böyle tahmin eder)
        self._avg = (self.ts[-1] - self.ts[0]) / n if n else last
        self.keyframes: list[int] = []

    @classmethod
    def from_fps(cls, fps: float, frame_count: int) -> "FrameMap":
        # Aegisub CFR'de tamsayı aritmetiği kullanır: kare * 1000 * pay / payda.
        # 23.976 kayan noktayla 24. kare 1000.99 -> 1000 olurdu; kesirle 1001.
        from fractions import Fraction
        fr = Fraction(fps).limit_denominator(100000)
        ts = [i * 1000 * fr.denominator // fr.numerator for i in range(max(2, frame_count + 1))]
        fm = cls(ts)
        fm.fps = fps
        return fm

    @property
    def frame_count(self) -> int:
        return len(self.ts)

    def _time_exact(self, frame: int) -> int:
        if frame < 0:
            return int(frame * self._avg)
        if frame < len(self.ts):
            return self.ts[frame]
        return int(self.ts[-1] + (frame - len(self.ts) + 1) * self._avg)

    def _frame_exact(self, ms: int) -> int:
        if ms < self.ts[0]:
            return -1 if ms < 0 else 0
        if ms >= self.ts[-1]:
            return len(self.ts) - 1 + int((ms - self.ts[-1]) // self._avg)
        return bisect.bisect_right(self.ts, ms) - 1

    def frame_at_time(self, ms: int, kind: int = EXACT) -> int:
        if kind == START:
            return self._frame_exact(ms - 1) + 1
        if kind == END:
            return self._frame_exact(ms - 1)
        return self._frame_exact(ms)

    def time_at_frame(self, frame: int, kind: int = EXACT) -> int:
        if kind == EXACT:
            return self._time_exact(frame)
        if kind == START:
            prev = self._time_exact(frame - 1)
            cur = self._time_exact(frame)
            return prev + (cur - prev + 1) // 2
        nxt = self._time_exact(frame + 1)
        cur = self._time_exact(frame)
        return cur + (nxt - cur + 1) // 2

    # Lua köprüsü için kısa adlar
    def frame_from_ms(self, ms: int) -> int:
        return self.frame_at_time(ms, START)

    def ms_from_frame(self, frame: int) -> int:
        return self.time_at_frame(frame, START)

    def snap(self, ms: int, kind: int = START) -> int:
        """Zamanı kare sınırına oturt (Aegisub 'kareye yasla')."""
        return self.time_at_frame(self.frame_at_time(ms, kind), kind)
