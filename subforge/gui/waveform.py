"""Audio waveform / spectrogram timeline with subtitle boxes, keyframes and drag-timing."""
from __future__ import annotations

import numpy as np
from PySide6.QtCore import Qt, QRectF, Signal, QPointF, QTimer
from PySide6.QtGui import QColor, QImage, QPainter, QPen, QBrush, QFont, QCursor
from PySide6.QtWidgets import QWidget

from ..core.timecode import ms_to_ass


class WaveformWidget(QWidget):
    seekRequested = Signal(int)
    boundaryDragging = Signal(int, str, int)   # row, "start"|"end", ms
    boundaryCommitted = Signal(int, str, int)
    rowClicked = Signal(int)
    rangeCreated = Signal(int, int)

    HANDLE_PX = 6

    def __init__(self, project, parent=None):
        super().__init__(parent)
        self.project = project
        self.audio = None
        self.model = None
        self.setMinimumHeight(110)
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.view_start = 0.0          # ms
        self.ms_per_px = 10.0
        self.play_ms = 0
        self.active_row = -1
        self.selected_rows: set[int] = set()
        self.spectrum_mode = False
        self.follow = True
        self.snap = True
        self.dark = True
        self._drag = None              # (row, which)
        self._range = None             # (anchor_ms, cur_ms)
        self._mouse_ms = None
        self._spec_cache = (None, None)
        self._hover = None

    # ---------------------------------------------------------- state
    def set_audio(self, audio):
        self.audio = audio
        self._spec_cache = (None, None)
        self.update()

    def duration(self) -> int:
        d = self.audio.duration_ms if self.audio else 0
        if self.project.events:
            d = max(d, max(e.end_time for e in self.project.events))
        return max(d, 60000)

    def set_play_ms(self, ms: int):
        self.play_ms = ms
        if self.follow and self._drag is None:
            span = self.width() * self.ms_per_px
            if not (self.view_start + span * 0.05 <= ms <= self.view_start + span * 0.9):
                self.view_start = max(0.0, ms - span * 0.25)
        self.update()

    def center_on(self, ms: int, ratio=0.5):
        span = self.width() * self.ms_per_px
        self.view_start = max(0.0, ms - span * ratio)
        self.update()

    def ensure_visible(self, a: int, b: int):
        span = self.width() * self.ms_per_px
        if a < self.view_start or b > self.view_start + span:
            self.view_start = max(0.0, a - span * 0.1)
            self.update()

    def x_of(self, ms: float) -> float:
        return (ms - self.view_start) / self.ms_per_px

    def ms_of(self, x: float) -> float:
        return self.view_start + x * self.ms_per_px

    # ---------------------------------------------------------- painting
    def _colors(self):
        if self.dark:
            return dict(bg=QColor("#14151e"), wave=QColor("#7aa2f7"), center=QColor("#2a2e45"),
                        box=QColor(158, 206, 106, 50), boxsel=QColor(122, 162, 247, 80), boxact=QColor(255, 199, 119, 90),
                        kf=QColor("#bb9af7"), play=QColor("#f7768e"), text=QColor("#c0caf5"))
        return dict(bg=QColor("#fafafa"), wave=QColor("#2563eb"), center=QColor("#d4d4d8"),
                    box=QColor(34, 139, 34, 40), boxsel=QColor(37, 99, 235, 60), boxact=QColor(245, 158, 11, 90),
                    kf=QColor("#7c3aed"), play=QColor("#dc2626"), text=QColor("#18181b"))

    def paintEvent(self, ev):
        p = QPainter(self)
        col = self._colors()
        W, H = self.width(), self.height()
        p.fillRect(self.rect(), col["bg"])
        t0, t1 = self.view_start, self.view_start + W * self.ms_per_px
        ruler_h = 16
        body = QRectF(0, ruler_h, W, H - ruler_h)
        # ruler
        p.setPen(col["text"])
        f = QFont(); f.setPointSize(7); p.setFont(f)
        step = _nice_step(self.ms_per_px * 90)
        first = int(t0 // step) * step
        t = first
        while t < t1:
            x = self.x_of(t)
            p.setPen(QPen(col["center"]))
            p.drawLine(int(x), 0, int(x), ruler_h)
            p.setPen(col["text"])
            p.drawText(int(x) + 3, 11, ms_to_ass(int(t)))
            t += step
        # audio
        mid = body.top() + body.height() / 2
        if self.audio:
            if self.spectrum_mode:
                self._paint_spectrum(p, body, t0, t1)
            else:
                mins, maxs = self.audio.peaks(t0, t1, W)
                p.setPen(QPen(col["center"])); p.drawLine(0, int(mid), W, int(mid))
                p.setPen(QPen(col["wave"]))
                amp = body.height() / 2 * 0.95
                for x in range(len(mins)):
                    p.drawLine(x, int(mid - maxs[x] * amp), x, int(mid - mins[x] * amp))
        else:
            p.setPen(col["text"])
            p.drawText(body, Qt.AlignmentFlag.AlignCenter, "Ses yok — Ses > Videodan/dosyadan ses aç")
        # keyframes
        p.setPen(QPen(col["kf"], 1, Qt.PenStyle.DashLine))
        for k in self.project.keyframes:
            if t0 <= k <= t1:
                x = int(self.x_of(k)); p.drawLine(x, ruler_h, x, H)
        # subtitle boxes
        evs = self.project.events
        if self.model is not None:
            evs = self.model.events
        for row, e in enumerate(evs):
            if e.end_time < t0 or e.start_time > t1:
                continue
            x0, x1 = self.x_of(e.start_time), self.x_of(e.end_time)
            r = QRectF(x0, ruler_h, max(1.0, x1 - x0), body.height())
            c = col["boxact"] if row == self.active_row else col["boxsel"] if row in self.selected_rows else col["box"]
            if e.comment:
                c = QColor(120, 120, 140, 35)
            p.fillRect(r, c)
            pen = QPen(col["boxact"] if row == self.active_row else col["wave"], 2 if row == self.active_row else 1)
            p.setPen(pen)
            p.drawLine(int(x0), ruler_h, int(x0), H)
            p.drawLine(int(x1), ruler_h, int(x1), H)
            if x1 - x0 > 28:
                p.setPen(col["text"])
                from ..core.tags import plain_text
                p.drawText(QRectF(x0 + 3, ruler_h + 1, x1 - x0 - 6, 14), Qt.AlignmentFlag.AlignLeft,
                           plain_text(e.text).replace("\n", " "))
        # new range
        if self._range:
            a, b = sorted(self._range)
            p.fillRect(QRectF(self.x_of(a), ruler_h, max(1.0, self.x_of(b) - self.x_of(a)), body.height()),
                       QColor(255, 255, 255, 45))
        # play cursor
        x = self.x_of(self.play_ms)
        if 0 <= x <= W:
            p.setPen(QPen(col["play"], 2)); p.drawLine(int(x), 0, int(x), H)
        # hover time
        if self._mouse_ms is not None:
            p.setPen(col["text"])
            p.drawText(QPointF(W - 90, H - 4), ms_to_ass(int(self._mouse_ms)))
        p.end()

    def _paint_spectrum(self, p: QPainter, body: QRectF, t0, t1):
        W = self.width()
        key = (int(t0), int(t1), W, int(body.height()))
        if self._spec_cache[0] != key:
            cols = max(1, W // 2)
            rows = max(16, int(body.height() // 2))
            s = self.audio.spectrogram(t0, t1, cols, rows)
            v = (s * 255).astype(np.uint8)[::-1]
            r = np.empty((rows, cols, 4), dtype=np.uint8)
            r[..., 0] = np.clip(v * 1.6, 0, 255)
            r[..., 1] = np.clip(v.astype(np.int16) * 2 - 120, 0, 255)
            r[..., 2] = np.clip(255 - v.astype(np.int16) * 2, 0, 255) // 2 + v // 4
            r[..., 3] = 255
            img = QImage(r.data, cols, rows, cols * 4, QImage.Format.Format_RGBA8888).copy()
            self._spec_cache = (key, img)
        p.drawImage(body, self._spec_cache[1])

    # ---------------------------------------------------------- interaction
    def _boundary_at(self, x: float):
        evs = self.model.events if self.model is not None else self.project.events
        best = None
        for row, e in enumerate(evs):
            for which, t in (("start", e.start_time), ("end", e.end_time)):
                d = abs(self.x_of(t) - x)
                if d <= self.HANDLE_PX and (best is None or d < best[0] or (row == self.active_row and d <= best[0])):
                    best = (d, row, which)
        return best

    def _row_at(self, ms: float):
        evs = self.model.events if self.model is not None else self.project.events
        hits = [r for r, e in enumerate(evs) if e.start_time <= ms <= e.end_time]
        if self.active_row in hits:
            return self.active_row
        return hits[0] if hits else -1

    def _snap(self, ms: float, row: int, which: str) -> int:
        if not self.snap or (QCursorMods() & Qt.KeyboardModifier.ShiftModifier):
            return int(round(ms))
        tol = self.ms_per_px * 8
        cands = []
        cands += [k for k in self.project.keyframes if abs(k - ms) <= tol]
        evs = self.model.events if self.model is not None else self.project.events
        for r, e in enumerate(evs):
            if r == row:
                continue
            for t in (e.start_time, e.end_time):
                if abs(t - ms) <= tol:
                    cands.append(t)
        if cands:
            return int(min(cands, key=lambda t: abs(t - ms)))
        return int(round(ms))

    def mousePressEvent(self, e):
        ms = self.ms_of(e.position().x())
        self.setFocus()
        if e.button() == Qt.MouseButton.LeftButton:
            b = self._boundary_at(e.position().x())
            if b and not (e.modifiers() & Qt.KeyboardModifier.ControlModifier):
                self._drag = (b[1], b[2])
                self.rowClicked.emit(b[1])
                return
            if e.modifiers() & Qt.KeyboardModifier.ControlModifier:
                self._range = (ms, ms)
                return
            row = self._row_at(ms)
            self.seekRequested.emit(int(ms))
            if row >= 0:
                self.rowClicked.emit(row)
            else:
                self._range = (ms, ms)
        elif e.button() == Qt.MouseButton.MiddleButton:
            self.seekRequested.emit(int(ms))

    def mouseMoveEvent(self, e):
        ms = self.ms_of(e.position().x())
        self._mouse_ms = ms
        if self._drag:
            row, which = self._drag
            t = max(0, self._snap(ms, row, which))
            self.boundaryDragging.emit(row, which, t)
        elif self._range:
            self._range = (self._range[0], max(0, ms))
        else:
            b = self._boundary_at(e.position().x())
            self.setCursor(Qt.CursorShape.SizeHorCursor if b else Qt.CursorShape.ArrowCursor)
        self.update()

    def mouseReleaseEvent(self, e):
        if self._drag and e.button() == Qt.MouseButton.LeftButton:
            row, which = self._drag
            self._drag = None
            t = max(0, self._snap(self.ms_of(e.position().x()), row, which))
            self.boundaryCommitted.emit(row, which, t)
        elif self._range and e.button() == Qt.MouseButton.LeftButton:
            a, b = sorted(self._range)
            self._range = None
            if b - a >= 100:
                self.rangeCreated.emit(int(a), int(b))
        self.update()

    def leaveEvent(self, e):
        self._mouse_ms = None
        self.update()

    def wheelEvent(self, e):
        d = e.angleDelta().y() or e.angleDelta().x()
        mods = e.modifiers()
        if mods & Qt.KeyboardModifier.ControlModifier:
            x = e.position().x()
            anchor = self.ms_of(x)
            f = 0.8 if d > 0 else 1.25
            self.ms_per_px = min(max(self.ms_per_px * f, 0.5), 600000 / max(1, self.width()))
            self.view_start = max(0.0, anchor - x * self.ms_per_px)
        else:
            self.view_start = max(0.0, self.view_start - d / 120 * self.width() * self.ms_per_px * 0.1)
        self.update()

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Left:
            self.view_start = max(0.0, self.view_start - self.width() * self.ms_per_px * 0.1); self.update()
        elif e.key() == Qt.Key.Key_Right:
            self.view_start += self.width() * self.ms_per_px * 0.1; self.update()
        else:
            super().keyPressEvent(e)


def QCursorMods():
    from PySide6.QtGui import QGuiApplication
    return QGuiApplication.keyboardModifiers()


def _nice_step(ms: float) -> int:
    for s in (10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000, 600000):
        if s >= ms:
            return s
    return 600000
