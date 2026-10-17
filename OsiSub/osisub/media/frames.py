"""Tek kareyi tam doğrulukla çözme (aegisub.get_frame, renk damlalığı, dışa aktarma).

Arama: hedef karenin pts'inden geriye en yakın anahtar kareye atlanır, sonra
pts >= hedef olan ilk kareye kadar çözülür. Ardışık kareler istenirse
(ileri adım) kap açık tutulur ve aramadan devam edilir.
"""

from __future__ import annotations

import threading

import numpy as np


class FrameGrabber:
    def __init__(self, path: str, index):
        self.path = path
        self.index = index           # FrameIndex
        self._lock = threading.Lock()
        self._c = None
        self._vs = None
        self._iter = None
        self._last_pts = None
        self._cache: dict[int, np.ndarray] = {}

    def close(self):
        with self._lock:
            if self._c is not None:
                self._c.close()
            self._c = self._vs = self._iter = None

    def _open(self):
        import av
        if self._c is None:
            self._c = av.open(self.path)
            self._vs = self._c.streams.video[0]
            self._vs.thread_type = "AUTO"

    def get(self, frame: int) -> np.ndarray:
        """(h, w, 3) uint8 RGB."""
        frame = max(0, min(frame, len(self.index) - 1))
        hit = self._cache.get(frame)
        if hit is not None:
            return hit
        with self._lock:
            self._open()
            target = self.index.pts[frame]
            need_seek = self._iter is None or self._last_pts is None or not (
                self._last_pts < target <= self._last_pts + 50 * self._frame_step())
            if need_seek:
                self._c.seek(int(target), stream=self._vs, backward=True, any_frame=False)
                self._iter = self._c.decode(self._vs)
            img = None
            for fr in self._iter:
                if fr.pts is None:
                    continue
                self._last_pts = fr.pts
                if fr.pts >= target:
                    img = fr.to_ndarray(format="rgb24")
                    break
            if img is None:
                raise RuntimeError(f"Kare {frame} çözülemedi")
            if len(self._cache) > 32:
                self._cache.clear()
            self._cache[frame] = img
            return img

    def _frame_step(self) -> int:
        p = self.index.pts
        return max(1, (p[-1] - p[0]) // max(1, len(p) - 1)) if len(p) > 1 else 1
