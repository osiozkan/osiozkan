"""Zamanlama araçları: kaydırma, kare yakalama, CPS, çakışma/boşluk düzeltme, scene-snap."""
from __future__ import annotations
from typing import Iterable, Sequence
from .ass import Line


def frame_to_ms(frame: int, fps: float) -> int:
    return int(round(frame * 1000.0 / fps))


def ms_to_frame(ms: int, fps: float) -> int:
    return int(round(ms * fps / 1000.0))


def shift(lines: Iterable[Line], delta_ms: int, *, start=True, end=True) -> None:
    for l in lines:
        if start:
            l.start = max(0, l.start + delta_ms)
        if end:
            l.end = max(0, l.end + delta_ms)


def snap_to_frames(lines: Iterable[Line], fps: float) -> None:
    for l in lines:
        l.start = frame_to_ms(ms_to_frame(l.start, fps), fps)
        l.end = frame_to_ms(ms_to_frame(l.end, fps), fps)


def cps(line: Line) -> float:
    n = len(line.plain.replace("\n", "").replace(" ", ""))
    d = max(line.duration, 1) / 1000.0
    return n / d


def fix_overlaps(lines: Sequence[Line], min_gap_ms: int = 0) -> int:
    """Aynı stildeki ardışık satırlar çakışıyorsa önceki satırın bitişini kısaltır."""
    n = 0
    ordered = sorted((l for l in lines if not l.comment), key=lambda l: (l.style, l.start))
    for a, b in zip(ordered, ordered[1:]):
        if a.style == b.style and a.end > b.start - min_gap_ms and a.start < b.start:
            a.end = max(a.start + 1, b.start - min_gap_ms)
            n += 1
    return n


def enforce_min_duration(lines: Iterable[Line], min_ms: int = 500, cps_target: float | None = None) -> int:
    n = 0
    for l in lines:
        want = min_ms
        if cps_target:
            chars = len(l.plain.replace("\n", "").replace(" ", ""))
            want = max(want, int(chars / cps_target * 1000))
        if l.duration < want:
            l.end = l.start + want
            n += 1
    return n


def snap_to_keyframes(lines: Iterable[Line], keyframes_ms: Sequence[int], fps: float,
                      lead_in=0, lead_out=0, thresh_start=300, thresh_end=900) -> int:
    """Aegisub 'Snap to scene' benzeri: başlangıç/bitişi yakın anahtar kareye yapıştırır."""
    import bisect
    kf = sorted(keyframes_ms)
    if not kf:
        return 0
    frame = 1000.0 / fps
    n = 0

    def nearest(t):
        i = bisect.bisect_left(kf, t)
        c = [kf[j] for j in (i - 1, i) if 0 <= j < len(kf)]
        return min(c, key=lambda k: abs(k - t))
    for l in lines:
        k = nearest(l.start)
        if abs(k - l.start) <= thresh_start:
            l.start = k - lead_in; n += 1
        k = nearest(l.end)
        if abs(k - l.end) <= thresh_end:
            l.end = int(k - frame) + lead_out; n += 1   # bitiş, anahtar karenin bir kare öncesi
    return n


def parse_keyframes(text: str) -> list[int]:
    """Aegisub 'keyframes v1' ve XviD/x264 stats dosyaları (kare numarası listesi)."""
    out = []
    for l in text.splitlines():
        l = l.strip()
        if l.isdigit():
            out.append(int(l))
    return out
