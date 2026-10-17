"""Kare şeridi: geçerli karenin çevresindeki kareleri küçük resim olarak gösterir.

bplayer'daki gibi her karenin önceden çıkarıldığı bir atlas yerine (24 dk ≈ 100 sn)
yalnız GÖRÜNEN kareler, ayrı bir çözücüyle arka planda ve sırayla çözülür;
sıralı çözme anahtar kareden ileri gittiği için hızlıdır. Son ~600 küçük resim
bellekte tutulur.

Tık: o kareye git · Tekerlek: ±1 kare (Shift: ±10) · Mor şerit: anahtar kare ·
Turuncu dikey şerit: sahne değişimi ·
Mavi alt çizgi: etkin satırın kareleri · Sarı çerçeve: geçerli kare.
"""

from __future__ import annotations

import queue
import threading
from collections import OrderedDict

from PySide6.QtCore import QObject, QRectF, Qt, Signal
from PySide6.QtGui import QColor, QFont, QImage, QPainter, QPen
from PySide6.QtWidgets import QSizePolicy, QWidget

from . import theme

GAP = 4
LABEL_H = 14


class _ThumbWorker(QObject):
    ready = Signal(int, int, object)        # (nesil, kare, QImage)

    def __init__(self):
        super().__init__()
        self.q: queue.Queue = queue.Queue()
        self.gen = 0
        self.grabber = None
        self._stop = False
        self.th = threading.Thread(target=self._run, name="osisub-thumbs", daemon=True)
        self.th.start()

    def set_source(self, path, index):
        from ..media.frames import FrameGrabber
        if self.grabber is not None:
            self.grabber.close()
        self.grabber = FrameGrabber(path, index) if path else None
        self.gen += 1

    def request(self, frames: list[int], th: int):
        self.gen += 1
        g = self.gen
        # eski istekleri boşalt
        try:
            while True:
                self.q.get_nowait()
        except queue.Empty:
            pass
        for f in frames:
            self.q.put((g, f, th))

    def stop(self):
        self._stop = True
        self.q.put(None)

    def _run(self):
        while not self._stop:
            item = self.q.get()
            if item is None:
                return
            g, f, th = item
            if g != self.gen or self.grabber is None:
                continue
            try:
                arr = self.grabber.get(f)
            except Exception:  # noqa: BLE001
                continue
            h, w, _ = arr.shape
            img = QImage(arr.data, w, h, 3 * w, QImage.Format_RGB888)
            tw = max(1, int(round(w * th / h)))
            thumb = img.scaled(tw, th, Qt.IgnoreAspectRatio, Qt.SmoothTransformation).copy()
            self.ready.emit(g, f, thumb)


class FrameStrip(QWidget):
    frame_clicked = Signal(int)

    def __init__(self, ctx, media):
        super().__init__()
        self.ctx = ctx
        self.media = media
        self.cache: OrderedDict[int, QImage] = OrderedDict()
        self.cache_h = 0
        self.worker = _ThumbWorker()
        self.worker.ready.connect(self._got)
        self.setMinimumHeight(70)
        self.setMaximumHeight(160)          # yükseklik büyüyünce küçük resimler dev olmasın
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        self.setFocusPolicy(Qt.ClickFocus)
        self._slots: list[tuple[int, QRectF]] = []
        media.index_ready.connect(self._on_index)
        media.video_closed.connect(self._on_close)
        media.scenes_ready.connect(self.update)
        media.position_changed.connect(lambda *_: self._refresh())
        ctx.active_changed.connect(lambda *_: self.update())
        ctx.rows_changed.connect(lambda *_: self.update())

    # ------------------------------------------------------------ kaynak
    def _on_index(self):
        self.cache.clear()
        self.worker.set_source(self.media.video_path, self.media.index)
        self._refresh()

    def _on_close(self):
        self.cache.clear()
        self.worker.set_source(None, None)
        self.update()

    def shutdown(self):
        self.worker.stop()
        if self.worker.grabber is not None:
            self.worker.grabber.close()

    # ------------------------------------------------------------ yerleşim
    def _thumb_size(self):
        idx = self.media.index
        th = max(16, self.height() - LABEL_H - 6)
        ar = (idx.width * float(idx.sar) / idx.height) if (idx and idx.height) else 16 / 9
        return max(16, int(th * ar)), th

    def visible_frames(self) -> list[int]:
        idx = self.media.index
        if idx is None:
            return []
        tw, _ = self._thumb_size()
        n = max(1, (self.width() + GAP) // (tw + GAP))
        cur = self.media.frame
        first = max(0, min(cur - n // 2, len(idx) - n))
        return list(range(max(0, first), min(len(idx), first + n)))

    def _refresh(self):
        _, th = self._thumb_size()
        if th != self.cache_h:
            self.cache.clear()
            self.cache_h = th
        frames = self.visible_frames()
        missing = [f for f in frames if f not in self.cache]
        # önce geçerli kare, sonra sıralı (çözücü ileri doğru hızlı)
        cur = self.media.frame
        if cur in missing:
            missing.remove(cur)
            missing.insert(0, cur)
        if missing:
            self.worker.request(missing, th)
        self.update()

    def _got(self, gen, frame, img):
        if gen != self.worker.gen or img.height() != self.cache_h:
            # yeni istek geldi; yine de önbelleğe al (aynı boy ise)
            if img.height() != self.cache_h:
                return
        self.cache[frame] = img
        self.cache.move_to_end(frame)
        while len(self.cache) > 600:
            self.cache.popitem(last=False)
        self.update()

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._refresh()

    # ------------------------------------------------------------ çizim
    def paintEvent(self, _e):
        c = theme.colors()
        p = QPainter(self)
        p.fillRect(self.rect(), QColor(c["base"]))
        idx = self.media.index
        if idx is None:
            p.setPen(QColor(c["muted"]))
            p.drawText(self.rect(), Qt.AlignCenter, "Kare şeridi — video açınca dolar")
            p.end()
            return
        tw, th = self._thumb_size()
        kf = set(idx.keyframes)
        scenes = set(getattr(self.media, "scenes", []) or [])
        ev = self.ctx.active_event()
        fm = self.ctx.frame_map
        line_rng = None
        if ev is not None and fm is not None:
            line_rng = (fm.frame_at_time(ev.start, 1), fm.frame_at_time(ev.end, 2))
        f_ = QFont(self.font())
        f_.setPointSize(max(7, f_.pointSize() - 2))
        p.setFont(f_)
        self._slots = []
        x = 0
        for f in self.visible_frames():
            r = QRectF(x, 3, tw, th)
            self._slots.append((f, r))
            img = self.cache.get(f)
            if img is not None:
                p.drawImage(r, img)
            else:
                p.fillRect(r, QColor(c["alt"]))
            if f in kf:
                p.fillRect(QRectF(x, 3, tw, 4), QColor("#b46cff"))
            if f in scenes:
                p.fillRect(QRectF(x, 7, 4, th - 8), QColor("#ff9800"))
            if line_rng and line_rng[0] <= f <= line_rng[1]:
                p.fillRect(QRectF(x, 3 + th - 4, tw, 4), QColor("#4da3ff"))
            if f == self.media.frame:
                p.setPen(QPen(QColor("#ffd400"), 2))
                p.drawRect(r.adjusted(1, 1, -1, -1))
            p.setPen(QColor(c["text"] if f == self.media.frame else c["muted"]))
            p.drawText(QRectF(x, 3 + th, tw, LABEL_H), Qt.AlignCenter, str(f))
            x += tw + GAP
        p.end()

    # ------------------------------------------------------------ fare
    def frame_at(self, x: float) -> int | None:
        for f, r in self._slots:
            if r.left() <= x <= r.right() + GAP:
                return f
        return None

    def mousePressEvent(self, e):
        f = self.frame_at(e.position().x())
        if f is not None:
            self.frame_clicked.emit(f)

    def wheelEvent(self, e):
        if self.media.index is None:
            return
        d = 1 if (e.angleDelta().y() or e.angleDelta().x()) < 0 else -1
        step = 10 if e.modifiers() & Qt.ShiftModifier else 1
        self.frame_clicked.emit(max(0, min(self.media.frame + d * step, len(self.media.index) - 1)))
