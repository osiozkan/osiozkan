from __future__ import annotations
import re
from typing import Optional

import numpy as np
from PySide6.QtCore import Qt, Signal, QRectF, QPointF, QTimer
from PySide6.QtGui import QPainter, QColor, QPen, QBrush, QImage, QPixmap, QFont
from PySide6.QtWidgets import (QLineEdit, QWidget, QSizePolicy, QGraphicsView, QGraphicsScene, QLabel)

from ..core.timeutil import ass_time, parse_ass_time


class TimeEdit(QLineEdit):
    """h:mm:ss.cc editor. Emits timeChanged(ms) on editingFinished."""
    timeChanged = Signal(int)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._ms = 0
        self.setMaximumWidth(96)
        self.setAlignment(Qt.AlignRight)
        self.editingFinished.connect(self._done)
        self.setText(ass_time(0))

    def ms(self) -> int: return self._ms

    def set_ms(self, ms: int):
        self._ms = int(ms); self.setText(ass_time(ms))

    def _done(self):
        t = self.text().strip()
        try:
            if re.fullmatch(r"\d+", t):  # plain number = seconds? treat as centiseconds-free ms
                ms = int(t)
            else:
                ms = parse_ass_time(t if t.count(":") == 2 else ("0:" + t if t.count(":") == 1 else t))
        except ValueError:
            self.setText(ass_time(self._ms)); return
        self._ms = ms; self.setText(ass_time(ms)); self.timeChanged.emit(ms)


class WaveformWidget(QWidget):
    """Waveform with subtitle spans. Click = seek, drag edges of the active line = retime."""
    seek = Signal(int)
    activeRetimed = Signal(int, int)   # start, end (ms)
    createRange = Signal(int, int)     # drag in empty area with ctrl

    def __init__(self, parent=None):
        super().__init__(parent)
        self.peaks: Optional[np.ndarray] = None   # (n,2) @ pps
        self.pps = 100
        self.ms_per_px = 10.0
        self.left_ms = 0
        self.spans: list[tuple[int, int, bool]] = []  # start,end,is_active
        self.active = (0, 0)
        self.playhead = 0
        self.keyframes: list[int] = []
        self.dark = True
        self._drag = None
        self.setMinimumHeight(90)
        self.setMouseTracking(True)
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)

    # -- data
    def set_peaks(self, peaks, pps=100):
        self.peaks, self.pps = peaks, pps; self.update()

    def set_active(self, start, end):
        self.active = (start, end)
        view = self.width() * self.ms_per_px
        if not (self.left_ms <= start and end <= self.left_ms + view):
            self.left_ms = max(0, int(start - view * 0.25))
        self.update()

    def set_playhead(self, ms):
        self.playhead = ms
        view = self.width() * self.ms_per_px
        if ms > self.left_ms + view * 0.95 or ms < self.left_ms:
            self.left_ms = max(0, int(ms - view * 0.1))
        self.update()

    def x_of(self, ms): return (ms - self.left_ms) / self.ms_per_px
    def ms_at(self, x): return int(self.left_ms + x * self.ms_per_px)

    # -- paint
    def paintEvent(self, ev):
        p = QPainter(self)
        bg, fg, mid = (QColor("#14171c"), QColor("#4aa3ff"), QColor("#2a2f3a")) if self.dark else \
                      (QColor("#f4f4f4"), QColor("#2b6cb0"), QColor("#d0d0d0"))
        p.fillRect(self.rect(), bg)
        w, h = self.width(), self.height()
        cy = h / 2
        # spans
        for s, e, act in self.spans:
            x0, x1 = self.x_of(s), self.x_of(e)
            if x1 < 0 or x0 > w: continue
            p.fillRect(QRectF(x0, 0, max(1, x1 - x0), h), QColor(255, 180, 40, 40) if act else QColor(120, 160, 255, 28))
        a0, a1 = self.x_of(self.active[0]), self.x_of(self.active[1])
        if self.active[1] > self.active[0]:
            p.fillRect(QRectF(a0, 0, a1 - a0, h), QColor(255, 140, 0, 60))
            p.setPen(QPen(QColor("#ffa500"), 2)); p.drawLine(a0, 0, a0, h)
            p.setPen(QPen(QColor("#ff5a5a"), 2)); p.drawLine(a1, 0, a1, h)
        p.setPen(QPen(mid)); p.drawLine(0, cy, w, cy)
        # waveform
        if self.peaks is not None and len(self.peaks):
            p.setPen(QPen(fg))
            for x in range(w):
                i0 = int((self.left_ms + x * self.ms_per_px) * self.pps / 1000)
                i1 = max(i0 + 1, int((self.left_ms + (x + 1) * self.ms_per_px) * self.pps / 1000))
                if i0 >= len(self.peaks): break
                seg = self.peaks[i0:i1]
                lo, hi = float(seg[:, 0].min()), float(seg[:, 1].max())
                p.drawLine(x, int(cy - hi * cy * 0.95), x, int(cy - lo * cy * 0.95))
        else:
            p.setPen(QColor("#777")); p.drawText(self.rect(), Qt.AlignCenter, "Ses yok — Ses → Videodan aç")
        # keyframes
        p.setPen(QPen(QColor(0, 200, 120, 120)))
        for k in self.keyframes:
            x = self.x_of(k)
            if 0 <= x <= w: p.drawLine(x, 0, x, h)
        # time ticks
        p.setPen(QColor("#889"))
        step = self._tick_step()
        t = (self.left_ms // step) * step
        while self.x_of(t) < w:
            x = self.x_of(t)
            p.drawLine(x, 0, x, 6); p.drawText(int(x) + 3, 14, ass_time(int(t))[:-3])
            t += step
        x = self.x_of(self.playhead)
        p.setPen(QPen(QColor("#fff"), 1)); p.drawLine(x, 0, x, h)

    def _tick_step(self):
        for s in (100, 500, 1000, 5000, 10000, 30000, 60000, 300000):
            if s / self.ms_per_px > 90: return s
        return 600000

    # -- interaction
    def wheelEvent(self, ev):
        if ev.modifiers() & Qt.ControlModifier:
            f = 0.8 if ev.angleDelta().y() > 0 else 1.25
            ms = self.ms_at(ev.position().x())
            self.ms_per_px = min(max(self.ms_per_px * f, 0.5), 400)
            self.left_ms = max(0, int(ms - ev.position().x() * self.ms_per_px))
        else:
            self.left_ms = max(0, self.left_ms - int(ev.angleDelta().y() * self.ms_per_px * 0.5))
        self.update()

    def mousePressEvent(self, ev):
        x = ev.position().x()
        s, e = self.active
        if e > s and abs(x - self.x_of(s)) < 5: self._drag = "start"
        elif e > s and abs(x - self.x_of(e)) < 5: self._drag = "end"
        elif ev.modifiers() & Qt.ControlModifier: self._drag = ("new", self.ms_at(x))
        else:
            self._drag = None; self.seek.emit(self.ms_at(x))

    def mouseMoveEvent(self, ev):
        x = ev.position().x()
        s, e = self.active
        if self._drag is None:
            near = e > s and (abs(x - self.x_of(s)) < 5 or abs(x - self.x_of(e)) < 5)
            self.setCursor(Qt.SizeHorCursor if near else Qt.ArrowCursor); return
        ms = self.ms_at(x)
        if self._drag == "start": self.active = (min(ms, e - 1), e)
        elif self._drag == "end": self.active = (s, max(ms, s + 1))
        else: self.active = (min(self._drag[1], ms), max(self._drag[1], ms))
        self.update()

    def mouseReleaseEvent(self, ev):
        if self._drag in ("start", "end"): self.activeRetimed.emit(*self.active)
        elif isinstance(self._drag, tuple): self.createRange.emit(*self.active)
        self._drag = None


class VideoView(QGraphicsView):
    """Shows frames (QImage) with libass overlay. Backend-agnostic: gets composited RGBA frames or video item."""
    clicked = Signal()
    toolPress = Signal(float, float, object)
    toolMove = Signal(float, float)
    toolRelease = Signal(float, float)
    tool_active = False

    def mousePressEvent(self, e):
        if self.tool_active:
            p = self.mapToScene(e.position().toPoint()); self.toolPress.emit(p.x(), p.y(), e.button()); return
        super().mousePressEvent(e)

    def mouseMoveEvent(self, e):
        if self.tool_active and e.buttons():
            p = self.mapToScene(e.position().toPoint()); self.toolMove.emit(p.x(), p.y()); return
        super().mouseMoveEvent(e)

    def mouseReleaseEvent(self, e):
        if self.tool_active:
            p = self.mapToScene(e.position().toPoint()); self.toolRelease.emit(p.x(), p.y()); return
        super().mouseReleaseEvent(e)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.scene_ = QGraphicsScene(self)
        self.setScene(self.scene_)
        self.setStyleSheet("background:#000; border:0")
        self.pix = self.scene_.addPixmap(QPixmap())
        self.sub_pix = self.scene_.addPixmap(QPixmap())
        self.sub_pix.setZValue(10)
        self.video_item = None
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.setMinimumSize(320, 180)
        self.vw, self.vh = 1280, 720
        self.set_video_size(1280, 720)

    def set_video_size(self, w, h):
        self.vw, self.vh = w, h
        self.scene_.setSceneRect(0, 0, w, h)
        self.fitInView(self.scene_.sceneRect(), Qt.KeepAspectRatio)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.fitInView(self.scene_.sceneRect(), Qt.KeepAspectRatio)

    def set_overlay(self, rgba: np.ndarray):
        h, w, _ = rgba.shape
        img = QImage(rgba.data, w, h, w * 4, QImage.Format_RGBA8888).copy()
        self.sub_pix.setPixmap(QPixmap.fromImage(img))

    def set_background(self, rgb: Optional[np.ndarray]):
        if rgb is None:
            self.pix.setPixmap(QPixmap()); return
        h, w, c = rgb.shape
        img = QImage(rgb.data, w, h, w * c, QImage.Format_RGB888 if c == 3 else QImage.Format_RGBA8888).copy()
        self.pix.setPixmap(QPixmap.fromImage(img))
