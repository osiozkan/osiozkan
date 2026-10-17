"""Ses bölmesi: dalga formu / spektrogram, satır blokları ve zamanlama.

Fare:
  * Sol tık      : etkin satırın BAŞLANGICI
  * Sağ tık      : etkin satırın BİTİŞİ
  * Sınırı sürükle (6 px yakın): o sınırı taşı
  * Ctrl + sınırı sürükle: bitişik satırın ortak ucu da birlikte taşınır
  * Çift tık     : tıklanan satırı etkin yap
  * Orta tık     : videoyu oraya götür
  * Tekerlek     : kaydır · Ctrl+tekerlek: yakınlaştır · Shift+tekerlek: genlik
Yaslama (açıksa, Shift basılıyken kapalı): 8 px içindeki anahtar kareye,
diğer satırların sınırlarına ve video konumuna yapışır.

Klavye (ses bölmesi odaktayken):
  Boşluk/S seçimi çal-durdur · Q başlangıç öncesi 500 ms · W bitiş sonrası 500 ms
  E ilk 500 ms · D son 500 ms · Z/X önceki/sonraki satır · Enter sonraki satır
"""

from __future__ import annotations

import bisect

import numpy as np
from PySide6.QtCore import QBuffer, QByteArray, QIODevice, QPointF, QRectF, Qt, QTimer, Signal
from PySide6.QtGui import QColor, QFont, QImage, QPainter, QPen
from PySide6.QtWidgets import (QCheckBox, QHBoxLayout, QLabel, QScrollBar, QSlider, QToolButton,
                               QVBoxLayout, QWidget)

from ..core.asstime import format_time
from . import theme

SNAP_PX = 8
EDGE_PX = 6
RULER_H = 18


# ---------------------------------------------------------------------------
# Oynatıcı (Qt Multimedia — ek DLL gerekmez)
# ---------------------------------------------------------------------------

class AudioPlayer:
    """Seçim çalma. Aygıt sorgusu İLK ÇALMADA yapılır (açılışta değil): bazı
    Linux ortamlarında ses sunucusu yokken Qt Multimedia aygıt sorgusu
    çökebiliyor; açılışı asla riske atmasın. OSISUB_NO_AUDIO=1 ile tamamen kapanır."""

    def __init__(self):
        import os
        self.sink = None
        self.buf = None
        self.start_ms = 0
        self.end_ms = 0
        self._checked = False
        self._available = os.environ.get("OSISUB_NO_AUDIO") != "1"
        self.error = "" if self._available else "OSISUB_NO_AUDIO=1"

    @property
    def available(self) -> bool:
        return self._available

    def _check(self) -> bool:
        if self._checked:
            return self._available
        self._checked = True
        if not self._available:
            return False
        try:
            from PySide6.QtMultimedia import QMediaDevices
            self._available = not QMediaDevices.defaultAudioOutput().isNull()
            if not self._available:
                self.error = "Ses çıkış aygıtı bulunamadı"
        except Exception as e:  # noqa: BLE001
            self._available = False
            self.error = str(e)
        return self._available

    def play(self, audio, start_ms: int, end_ms: int):
        self.stop()
        self.start_ms, self.end_ms = int(start_ms), int(end_ms)
        if audio is None or end_ms <= start_ms or not self._check():
            return False
        from PySide6.QtMultimedia import QAudioFormat, QAudioSink, QMediaDevices
        fmt = QAudioFormat()
        fmt.setSampleRate(audio.rate)
        fmt.setChannelCount(1)
        fmt.setSampleFormat(QAudioFormat.Int16)
        self.sink = QAudioSink(QMediaDevices.defaultAudioOutput(), fmt)
        self.buf = QBuffer()
        self.buf.setData(QByteArray(audio.slice_bytes(start_ms, end_ms)))
        self.buf.open(QIODevice.ReadOnly)
        self.sink.start(self.buf)
        return True

    def stop(self):
        if self.sink is not None:
            self.sink.stop()
            self.sink = None
        if self.buf is not None:
            self.buf.close()
            self.buf = None

    @property
    def playing(self) -> bool:
        if self.sink is None:
            return False
        from PySide6.QtMultimedia import QAudio
        return self.sink.state() == QAudio.State.ActiveState

    def position_ms(self) -> int | None:
        if self.sink is None:
            return None
        return self.start_ms + int(self.sink.processedUSecs() / 1000)


# ---------------------------------------------------------------------------
# Görünüm
# ---------------------------------------------------------------------------

def _spectro_lut():
    """Siyah → mor → turuncu → sarı → beyaz renk tablosu."""
    stops = [(0, (0, 0, 0)), (60, (40, 10, 90)), (130, (170, 30, 90)), (190, (240, 120, 30)),
             (235, (255, 220, 80)), (255, (255, 255, 255))]
    lut = []
    for i in range(256):
        for (a, ca), (b, cb) in zip(stops, stops[1:]):
            if a <= i <= b:
                t = (i - a) / max(1, b - a)
                lut.append(QColor(*[int(x + (y - x) * t) for x, y in zip(ca, cb)]).rgb())
                break
    return lut


class AudioView(QWidget):
    view_changed = Signal()
    seek_request = Signal(int)        # orta tık: videoyu bu ms'ye götür

    def __init__(self, ctx, media, player: AudioPlayer):
        super().__init__()
        self.ctx = ctx
        self.media = media
        self.player = player
        self.t0 = 0.0                 # görünür başlangıç (ms)
        self.ms_per_px = 10.0
        self.amp = 1.0
        self.spectrum = False
        self.snap = True
        self.drag = None              # "start" | "end" | None
        self.playhead = None
        self._lut = _spectro_lut()
        self.setMinimumHeight(120)
        self.setFocusPolicy(Qt.StrongFocus)
        self.setMouseTracking(True)
        ctx.active_changed.connect(lambda _r: self.ensure_active_visible())
        ctx.rows_changed.connect(lambda *_: self.update())
        ctx.document_reset.connect(self.update)
        media.position_changed.connect(lambda *_: self.update())
        media.audio_ready.connect(self._on_ready)
        media.index_ready.connect(self.update)
        media.spectrum_ready.connect(self.update)
        media.scenes_ready.connect(self.update)
        media.speech_ready.connect(self.update)

    # ---------------------------------------------------------- koordinat
    @property
    def duration_ms(self) -> int:
        a = self.media.audio
        return a.duration_ms if a is not None else 0

    def x_of(self, ms: float) -> float:
        return (ms - self.t0) / self.ms_per_px

    def ms_of(self, x: float) -> float:
        return self.t0 + x * self.ms_per_px

    @property
    def t1(self) -> float:
        return self.t0 + self.width() * self.ms_per_px

    def set_view(self, t0: float, ms_per_px: float | None = None):
        if ms_per_px is not None:
            self.ms_per_px = float(max(0.05, min(ms_per_px, 2000.0)))
        span = self.width() * self.ms_per_px
        maxt = max(0.0, self.duration_ms - span * 0.5)
        self.t0 = float(max(-span * 0.1, min(t0, maxt)))
        self.update()
        self.view_changed.emit()

    def _on_ready(self):
        ev = self.ctx.active_event()
        self.set_view(0, 10.0)
        if ev:
            self.ensure_active_visible(force_center=True)
        self.update()

    def ensure_active_visible(self, force_center: bool = False):
        ev = self.ctx.active_event()
        if ev is None or self.media.audio is None:
            self.update()
            return
        span = self.t1 - self.t0
        if force_center or ev.start < self.t0 or ev.end > self.t1:
            if ev.end - ev.start > span * 0.9:
                self.set_view(ev.start - span * 0.05)
            else:
                self.set_view((ev.start + ev.end) / 2 - span / 2)
        else:
            self.update()

    # ---------------------------------------------------------- çizim
    def paintEvent(self, _e):
        c = theme.colors()
        p = QPainter(self)
        w, h = self.width(), self.height()
        p.fillRect(self.rect(), QColor(c["base"]))
        a = self.media.audio
        if a is None:
            p.setPen(QColor(c["muted"]))
            msg = "Ses yükleniyor…" if self.media.audio_path else "Ses yok — Ses menüsünden aç (video açınca kendiliğinden yüklenir)."
            p.drawText(self.rect(), Qt.AlignCenter, msg)
            p.end()
            return
        top, wave_h = RULER_H, h - RULER_H
        mid = top + wave_h / 2
        evs = self.ctx.doc.events
        active = self.ctx.active
        sel = self.ctx.selection

        # satır blokları (arka plan)
        for i, ev in enumerate(evs):
            if ev.end < self.t0 or ev.start > self.t1 or ev.comment and i != active:
                continue
            x0, x1 = self.x_of(ev.start), self.x_of(ev.end)
            if i == active:
                col = QColor(c["highlight"])
                col.setAlpha(70)
            elif i in sel:
                col = QColor(c["highlight"])
                col.setAlpha(35)
            else:
                col = QColor(c["mid"])
                col.setAlpha(45)
            p.fillRect(QRectF(x0, top, max(1.0, x1 - x0), wave_h), col)

        # dalga formu ya da spektrogram
        if self.spectrum and a.spectrogram is None:
            p.setPen(QColor(c["muted"]))
            p.drawText(QRectF(0, top, w, 20), Qt.AlignCenter, "Spektrogram hesaplanıyor…")
        if self.spectrum and a.spectrogram is not None:
            img = a.spectrogram.window(self.t0, self.t1, w)
            img = np.ascontiguousarray(img[::-1])           # alçak frekans altta
            qi = QImage(img.data, img.shape[1], img.shape[0], img.shape[1], QImage.Format_Indexed8)
            qi.setColorTable(self._lut)
            p.setOpacity(0.92)
            p.drawImage(QRectF(0, top, w, wave_h), qi)
            p.setOpacity(1.0)
        else:
            mins, maxs = a.peaks.window(self.t0, self.t1, w)
            amp = wave_h / 2 * self.amp
            ys0 = np.clip(mid - maxs * amp, top, h)
            ys1 = np.clip(mid - mins * amp, top, h)
            p.setPen(QPen(QColor(c["ok_fg"]), 1))
            from PySide6.QtCore import QLineF
            p.drawLines([QLineF(x, ys0[x], x, ys1[x] + 0.5) for x in range(w)])
            p.setPen(QPen(QColor(c["border"]), 1))
            p.drawLine(0, int(mid), w, int(mid))

        # anahtar kareler
        fm = self.ctx.frame_map
        if fm is not None and fm.keyframes:
            pen = QPen(QColor("#b46cff"), 1, Qt.DashLine)
            p.setPen(pen)
            for kf in fm.keyframes:
                t = fm.time_at_frame(kf)
                if self.t0 <= t <= self.t1:
                    x = self.x_of(t)
                    p.drawLine(QPointF(x, top), QPointF(x, h))

        # konuşma bölümleri (VAD): altta ince şerit
        for a0, b0 in getattr(self.media, "speech", []) or []:
            if b0 < self.t0 or a0 > self.t1:
                continue
            xa, xb = self.x_of(a0), self.x_of(b0)
            p.fillRect(QRectF(xa, h - 5, max(1.0, xb - xa), 5), QColor(0, 200, 170, 170))

        # sahne değişimleri (ffmpeg scdet)
        if fm is not None and getattr(self.media, "scenes", None):
            p.setPen(QPen(QColor("#ff9800"), 1, Qt.DotLine))
            for sf in self.media.scenes:
                t = fm.time_at_frame(sf)
                if self.t0 <= t <= self.t1:
                    x = self.x_of(t)
                    p.drawLine(QPointF(x, top), QPointF(x, h))

        # diğer satırların numaraları ve etkin satırın sınırları
        f = QFont(self.font())
        f.setPointSize(max(7, f.pointSize() - 1))
        p.setFont(f)
        for i, ev in enumerate(evs):
            if ev.end < self.t0 or ev.start > self.t1 or (ev.comment and i != active):
                continue
            x0, x1 = self.x_of(ev.start), self.x_of(ev.end)
            if i == active:
                p.setPen(QPen(QColor("#ff5c5c"), 2))
                p.drawLine(QPointF(x0, top), QPointF(x0, h))
                p.setPen(QPen(QColor("#4da3ff"), 2))
                p.drawLine(QPointF(x1, top), QPointF(x1, h))
            else:
                p.setPen(QPen(QColor(c["muted"]), 1, Qt.DotLine))
                p.drawLine(QPointF(x0, top), QPointF(x0, h))
                p.drawLine(QPointF(x1, top), QPointF(x1, h))
            p.setPen(QColor(c["text"] if i == active else c["muted"]))
            p.drawText(QPointF(x0 + 3, top + 12), str(i + 1))

        # video konumu
        if self.media.index is not None:
            vx = self.x_of(self.media.position_ms)
            if 0 <= vx <= w:
                p.setPen(QPen(QColor("#ffd400"), 1))
                p.drawLine(QPointF(vx, top), QPointF(vx, h))
        # çalma imleci
        if self.playhead is not None:
            px = self.x_of(self.playhead)
            p.setPen(QPen(QColor("#ffffff"), 1))
            p.drawLine(QPointF(px, top), QPointF(px, h))

        self._draw_ruler(p, w)
        p.end()

    def _draw_ruler(self, p, w):
        c = theme.colors()
        p.fillRect(QRectF(0, 0, w, RULER_H), QColor(c["button"]))
        span = self.t1 - self.t0
        for step in (10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 30000, 60000):
            if span / step <= w / 70:
                break
        p.setPen(QColor(c["muted"]))
        t = (self.t0 // step) * step
        while t <= self.t1:
            x = self.x_of(t)
            if x >= 0:
                p.drawLine(QPointF(x, RULER_H - 5), QPointF(x, RULER_H))
                lbl = format_time(int(max(0, t)))[2:] if step >= 1000 else format_time(int(max(0, t)))[5:]
                p.drawText(QPointF(x + 2, RULER_H - 6), lbl)
            t += step

    # ---------------------------------------------------------- yaslama
    def snap_ms(self, ms: float, mods) -> int:
        if not self.snap or (mods & Qt.ShiftModifier):
            return int(round(ms))
        cands = []
        fm = self.ctx.frame_map
        if fm is not None:
            lo, hi = self.ms_of(-SNAP_PX), self.ms_of(self.width() + SNAP_PX)
            for kf in fm.keyframes:
                t = fm.time_at_frame(kf)
                if lo <= t <= hi:
                    cands.append(t)
            for sf in getattr(self.media, "scenes", []) or []:
                t = fm.time_at_frame(sf)
                if lo <= t <= hi:
                    cands.append(t)
        if self.media.index is not None:
            cands.append(self.media.position_ms)
        skip = {r for r, _f in getattr(self, "linked", [])}
        for i, ev in enumerate(self.ctx.doc.events):
            if i != self.ctx.active and i not in skip and not ev.comment:
                cands.extend((ev.start, ev.end))
        best, bd = None, SNAP_PX * self.ms_per_px
        for t in cands:
            d = abs(t - ms)
            if d <= bd:
                best, bd = t, d
        return int(round(best if best is not None else ms))

    # ---------------------------------------------------------- fare
    def _edge_at(self, x):
        ev = self.ctx.active_event()
        if ev is None:
            return None
        ds = abs(self.x_of(ev.start) - x)
        de = abs(self.x_of(ev.end) - x)
        if min(ds, de) <= EDGE_PX:
            return "start" if ds <= de else "end"
        return None

    def _set_times(self, start=None, end=None):
        row = self.ctx.active
        ev = self.ctx.active_event()
        if ev is None:
            return
        s = ev.start if start is None else start
        e = ev.end if end is None else end
        if start is not None and s > e:
            e = s
        if end is not None and e < s:
            s = e
        changes = {row: {"start": max(0, s), "end": max(0, e)}}
        # Ctrl ile tutulan bağlı sınır: komşu satırın ortak ucu da birlikte taşınır
        for r, fld in getattr(self, "linked", []):
            other = self.ctx.doc.events[r]
            if fld == "start":
                changes[r] = {"start": min(max(0, e), other.end)}
            else:
                changes[r] = {"end": max(max(0, s), other.start)}
        self.ctx.update_events(changes, "Ses: zamanlama", coalesce=f"audio:{row}")

    def mousePressEvent(self, e):
        if self.media.audio is None:
            return
        self.setFocus()
        x = e.position().x()
        edge = self._edge_at(x)
        self.linked = []
        if e.button() == Qt.LeftButton and edge:
            self.drag = edge
            if e.modifiers() & Qt.ControlModifier:
                self.linked = self._linked_rows(edge)
            return
        if e.button() == Qt.MiddleButton:
            self.seek_request.emit(int(self.ms_of(x)))
            return
        t = self.snap_ms(self.ms_of(x), e.modifiers())
        if e.button() == Qt.LeftButton:
            self._set_times(start=t)
            self.drag = "start"
        elif e.button() == Qt.RightButton:
            self._set_times(end=t)
            self.drag = "end"
        elif e.button() == Qt.MiddleButton:
            self.seek_request.emit(int(self.ms_of(x)))

    def mouseMoveEvent(self, e):
        x = e.position().x()
        if self.drag:
            t = self.snap_ms(self.ms_of(x), e.modifiers())
            self._set_times(**{self.drag: t})
        else:
            self.setCursor(Qt.SizeHorCursor if self._edge_at(x) else Qt.ArrowCursor)

    def _linked_rows(self, edge):
        ev = self.ctx.active_event()
        out = []
        for i, o in enumerate(self.ctx.doc.events):
            if i == self.ctx.active or o.comment:
                continue
            if edge == "end" and abs(o.start - ev.end) <= 10:
                out.append((i, "start"))
            elif edge == "start" and abs(o.end - ev.start) <= 10:
                out.append((i, "end"))
        return out

    def mouseReleaseEvent(self, _e):
        self.linked = []
        if self.drag:
            self.drag = None
            self.ctx.undo.break_coalesce()

    def mouseDoubleClickEvent(self, e):
        t = self.ms_of(e.position().x())
        best = None
        for i, ev in enumerate(self.ctx.doc.events):
            if ev.start <= t <= ev.end and not ev.comment:
                if best is None or (ev.end - ev.start) < (self.ctx.doc.events[best].end - self.ctx.doc.events[best].start):
                    best = i
        if best is not None:
            self.ctx.set_active(best, select_only=True)

    def wheelEvent(self, e):
        d = e.angleDelta().y() or e.angleDelta().x()
        if e.modifiers() & Qt.ControlModifier:
            mx = e.position().x()
            anchor = self.ms_of(mx)
            factor = 0.8 if d > 0 else 1.25
            new = max(0.05, min(self.ms_per_px * factor, 2000.0))
            self.set_view(anchor - mx * new, new)
        elif e.modifiers() & Qt.ShiftModifier:
            self.amp = float(max(0.25, min(self.amp * (1.15 if d > 0 else 1 / 1.15), 20.0)))
            self.update()
        else:
            self.set_view(self.t0 - d / 120 * self.width() * self.ms_per_px * 0.15)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.view_changed.emit()


# ---------------------------------------------------------------------------
# Bölme
# ---------------------------------------------------------------------------

class AudioPanel(QWidget):
    def __init__(self, ctx, media):
        super().__init__()
        self.ctx = ctx
        self.media = media
        self.player = AudioPlayer()
        self.view = AudioView(ctx, media, self.player)
        self.scroll = QScrollBar(Qt.Horizontal)
        self.scroll.valueChanged.connect(self._scrolled)
        self.view.view_changed.connect(self._sync_scroll)

        def btn(text, tip, slot):
            b = QToolButton()
            b.setText(text)
            b.setToolTip(tip)
            b.setAutoRaise(True)
            b.clicked.connect(slot)
            return b

        self.b_play = btn("▶ Seçim", "Seçimi çal / durdur (Boşluk, S)", self.toggle_play_selection)
        self.b_before = btn("◁500", "Başlangıçtan önceki 500 ms (Q)", lambda: self.play_rel("before"))
        self.b_after = btn("500▷", "Bitişten sonraki 500 ms (W)", lambda: self.play_rel("after"))
        self.b_first = btn("İlk 500", "Seçimin ilk 500 ms'si (E)", lambda: self.play_rel("first"))
        self.b_last = btn("Son 500", "Seçimin son 500 ms'si (D)", lambda: self.play_rel("last"))
        self.b_stop = btn("■", "Durdur", self.stop)
        self.spec = QCheckBox("Spektrogram")
        self.spec.toggled.connect(self._spec)
        self.snap = QCheckBox("Yasla")
        self.snap.setChecked(True)
        self.snap.setToolTip("Anahtar kare, diğer satır sınırları ve video konumuna 8 px içinde yapış "
                             "(Shift basılıyken kapalı)")
        self.snap.toggled.connect(lambda v: setattr(self.view, "snap", v))
        self.zoom = QSlider(Qt.Horizontal)
        self.zoom.setRange(0, 100)
        self.zoom.setFixedWidth(140)
        self.zoom.setToolTip("Yakınlaştırma (Ctrl+tekerlek)")
        self.zoom.valueChanged.connect(self._zoom_slider)
        self.info = QLabel("")
        bar = QHBoxLayout()
        bar.setContentsMargins(4, 2, 4, 2)
        for w in (self.b_play, self.b_stop, self.b_before, self.b_after, self.b_first, self.b_last):
            bar.addWidget(w)
        bar.addSpacing(10)
        bar.addWidget(self.spec)
        bar.addWidget(self.snap)
        bar.addWidget(QLabel("Yakınlık"))
        bar.addWidget(self.zoom)
        bar.addStretch(1)
        bar.addWidget(self.info)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addLayout(bar)
        lay.addWidget(self.view, 1)
        lay.addWidget(self.scroll)
        self._timer = QTimer(self)
        self._timer.setInterval(30)
        self._timer.timeout.connect(self._tick)
        media.audio_ready.connect(self._ready)
        media.audio_closed.connect(self.view.update)
        ctx.active_changed.connect(lambda _r: self._update_info())
        ctx.rows_changed.connect(lambda *_: self._update_info())
        self.view.installEventFilter(self)
        self._set_enabled(False)
        if not self.player.available:
            self.b_play.setToolTip(f"Çalma kullanılamıyor: {self.player.error}")

    def _set_enabled(self, v):
        for w in (self.b_play, self.b_before, self.b_after, self.b_first, self.b_last, self.b_stop):
            w.setEnabled(v and self.player.available)
        for w in (self.spec, self.snap, self.zoom):
            w.setEnabled(v)

    def _ready(self):
        self._set_enabled(True)
        self._sync_scroll()
        self._sync_zoom()
        self._update_info()

    def _update_info(self):
        ev = self.ctx.active_event()
        if ev is None:
            self.info.setText("")
            return
        self.info.setText(f"{format_time(ev.start)} → {format_time(ev.end)}  ({(ev.end - ev.start) / 1000:.2f} sn)")

    # ---------------------------------------------------------- kaydırma/zoom
    def _sync_scroll(self):
        v = self.view
        span = int(v.width() * v.ms_per_px)
        self.scroll.blockSignals(True)
        self.scroll.setRange(0, max(0, v.duration_ms - span // 2))
        self.scroll.setPageStep(max(1, span))
        self.scroll.setSingleStep(max(1, span // 10))
        self.scroll.setValue(int(max(0, v.t0)))
        self.scroll.blockSignals(False)
        self._sync_zoom()

    def _scrolled(self, val):
        self.view.set_view(float(val))

    def _sync_zoom(self):
        import math
        v = self.view.ms_per_px
        pos = int(round((math.log(2000.0) - math.log(v)) / (math.log(2000.0) - math.log(0.05)) * 100))
        self.zoom.blockSignals(True)
        self.zoom.setValue(max(0, min(100, pos)))
        self.zoom.blockSignals(False)

    def _zoom_slider(self, val):
        import math
        v = math.exp(math.log(2000.0) - val / 100 * (math.log(2000.0) - math.log(0.05)))
        centre = (self.view.t0 + self.view.t1) / 2
        self.view.set_view(centre - self.view.width() * v / 2, v)

    def _spec(self, on):
        self.view.spectrum = on
        self.view.update()

    # ---------------------------------------------------------- çalma
    def play_range(self, a, b):
        if self.player.play(self.media.audio, a, b):
            self._timer.start()

    def toggle_play_selection(self):
        if self.player.playing:
            self.stop()
            return
        ev = self.ctx.active_event()
        if ev:
            self.play_range(ev.start, ev.end)

    def play_rel(self, which):
        ev = self.ctx.active_event()
        if ev is None:
            return
        rng = {"before": (ev.start - 500, ev.start), "after": (ev.end, ev.end + 500),
               "first": (ev.start, min(ev.end, ev.start + 500)), "last": (max(ev.start, ev.end - 500), ev.end)}
        self.play_range(*rng[which])

    def stop(self):
        self.player.stop()
        self._timer.stop()
        self.view.playhead = None
        self.view.update()

    def _tick(self):
        pos = self.player.position_ms()
        if pos is None or not self.player.playing or pos >= self.player.end_ms:
            if pos is not None and pos >= self.player.end_ms:
                self.stop()
            elif not self.player.playing:
                self.stop()
            return
        self.view.playhead = pos
        if pos > self.view.t1:
            self.view.set_view(pos - self.view.width() * self.view.ms_per_px * 0.1)
        self.view.update()

    # ---------------------------------------------------------- klavye
    def eventFilter(self, obj, ev):
        from PySide6.QtCore import QEvent
        if obj is self.view and ev.type() == QEvent.KeyPress:
            k = ev.key()
            if k in (Qt.Key_Space, Qt.Key_S):
                self.toggle_play_selection()
                return True
            keymap = {Qt.Key_Q: "before", Qt.Key_W: "after", Qt.Key_E: "first", Qt.Key_D: "last"}
            if k in keymap:
                self.play_rel(keymap[k])
                return True
            if k in (Qt.Key_Z, Qt.Key_X, Qt.Key_Return, Qt.Key_Enter):
                d = -1 if k == Qt.Key_Z else 1
                self.ctx.undo.break_coalesce()
                self.ctx.set_active(self.ctx.active + d, select_only=True)
                return True
        return super().eventFilter(obj, ev)
