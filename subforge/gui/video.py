"""Video panel: QMediaPlayer playback with libass subtitle overlay (or standalone subtitle preview)."""
from __future__ import annotations

import numpy as np
from PySide6.QtCore import Qt, QRectF, QTimer, QUrl, Signal
from PySide6.QtGui import QColor, QImage, QPainter, QFont
from PySide6.QtWidgets import (QHBoxLayout, QLabel, QPushButton, QSlider, QToolButton, QVBoxLayout, QWidget,
                               QDoubleSpinBox, QCheckBox)

from ..core.timecode import ms_to_ass
from .. import render

try:
    from PySide6.QtMultimedia import QAudioOutput, QMediaPlayer, QVideoSink
    HAVE_MM = True
except Exception:  # pragma: no cover
    HAVE_MM = False


class VideoCanvas(QWidget):
    def __init__(self, panel):
        super().__init__()
        self.panel = panel
        self.setMinimumSize(320, 180)
        self.setAttribute(Qt.WidgetAttribute.WA_OpaquePaintEvent)

    def _video_rect(self) -> QRectF:
        p = self.panel
        w, h = p.native_size()
        if w <= 0 or h <= 0:
            w, h = 16, 9
        s = min(self.width() / w, self.height() / h)
        vw, vh = w * s, h * s
        return QRectF((self.width() - vw) / 2, (self.height() - vh) / 2, vw, vh)

    def paintEvent(self, e):
        p = QPainter(self)
        p.fillRect(self.rect(), QColor("#000000"))
        r = self._video_rect()
        pan = self.panel
        if pan.frame_image is not None:
            p.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
            p.drawImage(r, pan.frame_image)
        else:
            p.fillRect(r, QColor("#10131c"))
        ov = pan.overlay_for(int(r.width()), int(r.height()))
        if ov is not None:
            p.drawImage(r.topLeft(), ov)
        elif not render.available():
            p.setPen(QColor("#ffffff"))
            p.drawText(r, Qt.AlignmentFlag.AlignCenter, "libass bulunamadı — altyazı önizlemesi kapalı")
        p.end()


class VideoPanel(QWidget):
    positionChanged = Signal(int)
    durationChanged = Signal(int)
    loaded = Signal(str)

    def __init__(self, project, parent=None):
        super().__init__(parent)
        self.project = project
        self.frame_image: QImage | None = None
        self._pos = 0
        self._dur = 0
        self._ov_key = None
        self._ov_img = None
        self._renderer = render.make_renderer()
        self._doc_dirty = True
        self._seeking = False
        self.canvas = VideoCanvas(self)

        self.player = None
        if HAVE_MM:
            self.player = QMediaPlayer(self)
            self.audio_out = QAudioOutput(self)
            self.player.setAudioOutput(self.audio_out)
            self.sink = QVideoSink(self)
            self.player.setVideoSink(self.sink)
            self.sink.videoFrameChanged.connect(self._on_frame)
            self.player.durationChanged.connect(self._on_duration)
            self.player.positionChanged.connect(self._on_pos)
            self.player.playbackStateChanged.connect(lambda *_: self._sync_btn())
            self.player.errorOccurred.connect(lambda err, msg: self.status.setText(f"Medya hatası: {msg}"))

        self.btn_play = QToolButton(); self.btn_play.setText("▶"); self.btn_play.clicked.connect(self.toggle)
        self.btn_prev = QToolButton(); self.btn_prev.setText("◀|"); self.btn_prev.setToolTip("Önceki kare (,)")
        self.btn_next = QToolButton(); self.btn_next.setText("|▶"); self.btn_next.setToolTip("Sonraki kare (.)")
        self.btn_prev.clicked.connect(lambda: self.step(-1))
        self.btn_next.clicked.connect(lambda: self.step(1))
        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(0, 1000)
        self.slider.sliderPressed.connect(lambda: setattr(self, "_seeking", True))
        self.slider.sliderReleased.connect(self._slider_release)
        self.slider.sliderMoved.connect(lambda v: self.set_position(int(v * max(1, self._dur) / 1000)))
        self.time_lbl = QLabel("0:00:00.00 / 0:00:00.00")
        self.time_lbl.setFont(QFont("monospace"))
        self.speed = QDoubleSpinBox(); self.speed.setRange(0.25, 4.0); self.speed.setSingleStep(0.25)
        self.speed.setValue(1.0); self.speed.setSuffix("×"); self.speed.valueChanged.connect(self._speed)
        self.vol = QSlider(Qt.Orientation.Horizontal); self.vol.setRange(0, 100); self.vol.setValue(80)
        self.vol.setFixedWidth(70); self.vol.valueChanged.connect(self._volume)
        self._volume(80)
        self.status = QLabel("Video yok — altyazı önizleme modu")
        self.status.setStyleSheet("color:#7a85b8; font-size:11px")

        bar = QHBoxLayout()
        for w in (self.btn_prev, self.btn_play, self.btn_next):
            bar.addWidget(w)
        bar.addWidget(self.slider, 1)
        bar.addWidget(self.time_lbl)
        bar.addWidget(self.speed)
        bar.addWidget(QLabel("🔊")); bar.addWidget(self.vol)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.addWidget(self.canvas, 1)
        lay.addLayout(bar)
        lay.addWidget(self.status)

        self._debounce = QTimer(self); self._debounce.setSingleShot(True); self._debounce.setInterval(120)
        self._debounce.timeout.connect(self._rebuild_doc)
        project.reset.connect(self.mark_doc_dirty)
        project.rowsChanged.connect(lambda a, b: self.mark_doc_dirty())

    # ------------------------------------------------------------ media
    def load(self, path: str):
        if not self.player:
            self.status.setText("QtMultimedia kullanılamıyor")
            return
        self.frame_image = None
        self.player.setSource(QUrl.fromLocalFile(path))
        self.player.pause()
        self.status.setText(path)
        self.loaded.emit(path)

    def native_size(self):
        if self.frame_image is not None:
            return self.frame_image.width(), self.frame_image.height()
        if self.project.video_size:
            return self.project.video_size
        return self.project.doc.play_res

    def _on_frame(self, frame):
        if not frame.isValid():
            return
        img = frame.toImage()
        if img.isNull():
            return
        self.frame_image = img
        st = frame.startTime()
        if st >= 0:
            self._pos = int(st / 1000)
            self._update_time()
            self.positionChanged.emit(self._pos)
        self.canvas.update()

    def _on_pos(self, ms):
        if self.frame_image is None:
            self._pos = ms
            self._update_time()
            self.positionChanged.emit(ms)
            self.canvas.update()

    def _on_duration(self, d):
        self._dur = d
        self.durationChanged.emit(d)
        self._update_time()

    def _update_time(self):
        self.time_lbl.setText(f"{ms_to_ass(self._pos)} / {ms_to_ass(self._dur)}")
        if not self._seeking and self._dur:
            self.slider.blockSignals(True)
            self.slider.setValue(int(self._pos * 1000 / self._dur))
            self.slider.blockSignals(False)

    def _slider_release(self):
        self._seeking = False

    def position(self) -> int:
        return self._pos

    def set_position(self, ms: int):
        ms = max(0, int(ms))
        self._pos = ms
        if self.player and self.player.source().isValid():
            self.player.setPosition(ms)
        self._update_time()
        self.positionChanged.emit(ms)
        self.canvas.update()

    def is_playing(self):
        return bool(self.player and self.player.playbackState() == QMediaPlayer.PlaybackState.PlayingState)

    def toggle(self):
        if not self.player or not self.player.source().isValid():
            return
        if self.is_playing():
            self.player.pause()
        else:
            self.player.play()

    def play_range(self, start: int, end: int):
        if not self.player or not self.player.source().isValid():
            return
        self.set_position(start)
        self.player.play()
        self._stop_at = end
        t = QTimer(self); t.setInterval(20)
        def chk():
            if self._pos >= end or not self.is_playing():
                self.player.pause(); t.stop(); t.deleteLater()
        t.timeout.connect(chk); t.start()

    def step(self, n: int):
        if self.is_playing():
            self.player.pause()
        self.set_position(self._pos + int(round(n * self.project.frame_ms)))

    def _sync_btn(self):
        self.btn_play.setText("⏸" if self.is_playing() else "▶")

    def _speed(self, v):
        if self.player:
            self.player.setPlaybackRate(v)

    def _volume(self, v):
        if self.player:
            self.audio_out.setVolume(v / 100.0)

    # ------------------------------------------------------------ subtitles overlay
    def mark_doc_dirty(self):
        self._doc_dirty = True
        self._debounce.start()

    def _rebuild_doc(self):
        if self._renderer is None:
            return
        self._renderer.set_document(self.project.ass_text_for_render())
        self._doc_dirty = False
        self._ov_key = None
        self.canvas.update()

    def overlay_for(self, w: int, h: int):
        if self._renderer is None or w <= 0 or h <= 0:
            return None
        if self._doc_dirty:
            self._rebuild_doc()
        key = (self._pos // 10, w, h, id(self._renderer), self.project.version)
        if key != self._ov_key:
            arr = self._renderer.render(self._pos, w, h, self.project.doc.play_res)
            self._ov_key = key
            if arr is None:
                self._ov_img = None
            else:
                arr = np.ascontiguousarray(arr)
                self._ov_img = QImage(arr.data, w, h, w * 4, QImage.Format.Format_RGBA8888).copy()
        return self._ov_img
