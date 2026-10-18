from __future__ import annotations
from PySide6.QtCore import QObject, QRunnable, QThreadPool, QTimer, Qt, Signal, QRectF, QPointF
from PySide6.QtGui import QColor, QImage, QPainter, QPen, QPixmap
from PySide6.QtWidgets import QLabel, QSizePolicy, QWidget
from .. import media


class _FrameSignals(QObject):
    done = Signal(int, QImage, str)


class _FrameJob(QRunnable):
    def __init__(self, serial, fn):
        super().__init__()
        self.serial, self.fn = serial, fn
        self.signals = _FrameSignals()

    def run(self):
        try:
            img = QImage.fromData(self.fn())
            self.signals.done.emit(self.serial, img, "")
        except Exception as e:  # pragma: no cover
            self.signals.done.emit(self.serial, QImage(), str(e))


class _StreamSignals(QObject):
    frame = Signal(QImage, int)
    finished = Signal()


class VideoPlayback:
    """Runs a media.VideoStream in a thread and emits QImage frames."""

    def __init__(self, info, ass_text, start_ms, width, on_frame, on_done):
        import threading
        self.stream = media.VideoStream(info, ass_text, start_ms, width)
        self.sig = _StreamSignals()
        self.sig.frame.connect(on_frame)
        self.sig.finished.connect(on_done)
        self.stop_flag = False
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()

    def _run(self):
        w, h = self.stream.width, self.stream.height
        try:
            for ms, buf in self.stream.frames():
                if self.stop_flag:
                    break
                img = QImage(buf, w, h, w * 3, QImage.Format_RGB888).copy()
                self.sig.frame.emit(img, ms)
        finally:
            self.sig.finished.emit()

    def stop(self):
        self.stop_flag = True
        self.stream.close()


class PreviewWidget(QLabel):
    """Renders the current frame (optional video) with subtitles via ffmpeg/libass, asynchronously."""
    clicked = Signal(float, float)  # script-resolution coordinates

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumSize(320, 180)
        self.setAlignment(Qt.AlignCenter)
        self.setStyleSheet("background:#15181c;color:#8a93a0;")
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.setText("Önizleme")
        self.pool = QThreadPool(self)
        self.pool.setMaxThreadCount(1)
        self.serial = 0
        self._pix = None
        self._timer = QTimer(self, singleShot=True, interval=150)
        self._timer.timeout.connect(self._kick)
        self._req = None
        self.video = None  # VideoInfo
        self.script_res = (1920, 1080)
        self.pick_mode = False

    def request(self, ass_text: str, ms: int, res: tuple[int, int]):
        self._req = (ass_text, ms, res)
        self._timer.start()

    def _kick(self):
        if not self._req or not media.available():
            if not media.available():
                self.setText("ffmpeg bulunamadı — önizleme devre dışı")
            return
        text, ms, (rx, ry) = self._req
        self.script_res = (rx or 1920, ry or 1080)
        self.serial += 1
        w = max(320, min(self.width(), 1280))
        video = self.video
        import os, tempfile

        def work():
            if video:
                fd, p = tempfile.mkstemp(suffix=".ass"); os.close(fd)
                try:
                    open(p, "w", encoding="utf-8").write(text)
                    return media.grab_frame(video.path, ms, ass_path=p, width=w)
                finally:
                    os.unlink(p)
            h = int(w * ry / max(rx, 1))
            return media.render_blank_with_subs(text, ms, w - w % 2, h - h % 2)

        job = _FrameJob(self.serial, work)
        job.signals.done.connect(self._done)
        self.pool.start(job)

    def show_image(self, img: QImage):
        self.serial += 1  # invalidate pending async renders
        self._pix = QPixmap.fromImage(img)
        self._fit()

    def _done(self, serial, img, err):
        if serial != self.serial:
            return
        if err or img.isNull():
            self.setText(err or "önizleme hatası")
            return
        self._pix = QPixmap.fromImage(img)
        self._fit()

    def _fit(self):
        if self._pix:
            self.setPixmap(self._pix.scaled(self.size(), Qt.KeepAspectRatio, Qt.SmoothTransformation))

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._fit()

    def mousePressEvent(self, ev):
        pm = self.pixmap()
        if self.pick_mode and pm is not None and not pm.isNull():
            pw, ph = pm.width() / pm.devicePixelRatio(), pm.height() / pm.devicePixelRatio()
            ox, oy = (self.width() - pw) / 2, (self.height() - ph) / 2
            x, y = ev.position().x() - ox, ev.position().y() - oy
            if 0 <= x <= pw and 0 <= y <= ph:
                self.clicked.emit(x / pw * self.script_res[0], y / ph * self.script_res[1])
                return
        super().mousePressEvent(ev)


class WaveformWidget(QWidget):
    """Audio waveform with subtitle bars; drag start/end handles of the active line to retime."""
    seek = Signal(int)
    timing_changed = Signal(int, int, int)  # event index, start, end (live)
    timing_committed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(110)
        self.peaks = None
        self.rate = 100
        self.events = []
        self.active = -1
        self.pos_ms = 0
        self.view_start = 0.0  # ms
        self.ms_per_px = 20.0
        self._drag = None
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.StrongFocus)

    def set_peaks(self, peaks, rate):
        self.peaks, self.rate = peaks, rate
        self.update()

    def set_events(self, events, active):
        self.events, self.active = events, active
        self.update()

    def center_on(self, ms):
        w = self.width() * self.ms_per_px
        if not (self.view_start + w * 0.05 < ms < self.view_start + w * 0.95):
            self.view_start = max(0.0, ms - w / 2)
        self.update()

    def x_of(self, ms):
        return (ms - self.view_start) / self.ms_per_px

    def ms_of(self, x):
        return int(self.view_start + x * self.ms_per_px)

    def paintEvent(self, ev):
        p = QPainter(self)
        p.fillRect(self.rect(), QColor("#101317"))
        h = self.height()
        mid = h / 2
        if self.peaks is not None and len(self.peaks):
            p.setPen(QPen(QColor("#3b82c4")))
            mx = max(int(self.peaks.max()), 1)
            for x in range(self.width()):
                a = int((self.view_start + x * self.ms_per_px) * self.rate / 1000)
                b = int((self.view_start + (x + 1) * self.ms_per_px) * self.rate / 1000) + 1
                if a >= len(self.peaks):
                    break
                v = int(self.peaks[a:b].max()) / mx * (h / 2 - 4)
                p.drawLine(x, int(mid - v), x, int(mid + v))
        for i, e in enumerate(self.events):
            x1, x2 = self.x_of(e.start), self.x_of(e.end)
            if x2 < 0 or x1 > self.width():
                continue
            act = i == self.active
            c = QColor(240, 180, 60, 70 if act else 30)
            p.fillRect(QRectF(x1, 0, x2 - x1, h), c)
            p.setPen(QPen(QColor("#f0b43c" if act else "#7a6a40"), 2 if act else 1))
            p.drawLine(int(x1), 0, int(x1), h)
            p.drawLine(int(x2), 0, int(x2), h)
        p.setPen(QPen(QColor("#ff5555")))
        px = self.x_of(self.pos_ms)
        p.drawLine(int(px), 0, int(px), h)
        p.end()

    def _handle_at(self, x):
        if 0 <= self.active < len(self.events):
            e = self.events[self.active]
            for which, ms in (("start", e.start), ("end", e.end)):
                if abs(self.x_of(ms) - x) < 6:
                    return which
        return None

    def mousePressEvent(self, ev):
        x = ev.position().x()
        h = self._handle_at(x)
        if h:
            self._drag = h
        else:
            self.pos_ms = self.ms_of(x)
            self.seek.emit(self.pos_ms)
            self.update()

    def mouseMoveEvent(self, ev):
        x = ev.position().x()
        if self._drag and 0 <= self.active < len(self.events):
            e = self.events[self.active]
            ms = max(0, self.ms_of(x))
            if self._drag == "start":
                e.start = min(ms, e.end)
            else:
                e.end = max(ms, e.start)
            self.timing_changed.emit(self.active, e.start, e.end)
            self.update()
        else:
            self.setCursor(Qt.SizeHorCursor if self._handle_at(x) else Qt.ArrowCursor)

    def mouseReleaseEvent(self, ev):
        if self._drag:
            self._drag = None
            self.timing_committed.emit()

    def wheelEvent(self, ev):
        d = ev.angleDelta().y()
        if ev.modifiers() & Qt.ControlModifier:
            x = ev.position().x()
            anchor = self.ms_of(x)
            self.ms_per_px = min(400.0, max(1.0, self.ms_per_px * (0.85 if d > 0 else 1 / 0.85)))
            self.view_start = max(0.0, anchor - x * self.ms_per_px)
        else:
            self.view_start = max(0.0, self.view_start - d * self.ms_per_px / 2)
        self.update()
