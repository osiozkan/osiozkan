"""Görsel tipografi araçları (Aegisub'ın video üstü araçları).

Tuval, geçerli karenin ALTYAZILI görüntüsünü gösterir (libmpv varsa mpv'nin
kendi ekran görüntüsü — oynatmadaki libass çiziminin aynısı; yoksa altyazısız kare)
ve tutamaçları Qt ile üstüne çizer. Her değişiklik belgeye anında yazılır;
bir sürükleme tek geri al adımıdır. Görüntü sürükleme sırasında ~8 kez/sn,
bırakınca hemen tazelenir.

Araçlar:
  Artı      : betik koordinatını gösterir; çift tık satırı oraya taşır (\\pos)
  Taşı      : \\pos ya da \\move uçlarını sürükle; Ctrl+sürükle \\pos'u \\move'a çevirir;
              çift tık konumu tıklanan noktaya koyar; seçili TÜM satırlar birlikte kayar
  Döndür Z  : halka çevresinde sürükle -> \\frz (Shift: 15° adım); merkezdeki artıyı sürükle -> \\org
  Döndür XY : yatay sürükleme \\fry, dikey sürükleme \\frx; artı -> \\org
  Ölçek     : sağdaki tutamaç \\fscx, üstteki \\fscy, başka yer ikisi birden
  Clip      : dikdörtgen \\clip (Ters seçiliyse \\iclip); köşeleri/içini sürükle, boş yerde yeni çiz
  Vektör    : tık nokta ekler, nokta sürüklenir, sağ tık siler, Ctrl+tık en yakın kenara nokta sokar
  Damlalık  : tıklanan pikselin rengini seçili satırlara (\\1c/\\2c/\\3c/\\4c) yazar ve panoya kopyalar
  İzleme    : hareket takibi alanını çiz (sürükle); iz varsa alanın bu karedeki yeri (sarı dörtgen),
              izin yolu ve güvensiz kareler (kırmızı) gösterilir. Satır gerekmez.
              Ctrl+sürükle: izi bu karede elle düzelt; Ctrl+Shift+sürükle: bu kare ve referanstan
              uzaktaki tüm kareler (kayma düzeltme). Bölmedeki "İzi geri al" ile geri alınır.
"""

from __future__ import annotations

import math

from PySide6.QtCore import QPointF, QRectF, Qt, QTimer, Signal
from PySide6.QtGui import QBrush, QColor, QFont, QImage, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import QApplication, QSizePolicy, QWidget

from ..core import tags as T

TOOLS = [
    ("cross", "Artı", "Koordinat; çift tık = konumu buraya taşı"),
    ("drag", "Taşı", "\\pos / \\move (Ctrl+sürükle: \\move'a çevir; çift tık: buraya koy)"),
    ("rotz", "Döndür Z", "\\frz; merkezdeki artı \\org (Shift: 15° adım)"),
    ("rotxy", "Döndür XY", "\\frx / \\fry; merkezdeki artı \\org"),
    ("scale", "Ölçek", "\\fscx / \\fscy"),
    ("clip", "Clip", "Dikdörtgen \\clip / \\iclip"),
    ("vclip", "Vektör clip", "Tık ekle · sürükle · sağ tık sil · Ctrl+tık araya ekle"),
    ("eyedrop", "Damlalık", "Pikselin rengini seçili satırlara yaz"),
    ("track", "İzleme", "Hareket takibi alanını çiz; izin bu karedeki yerini göster"),
    ("seed", "Tohum", "Renk modeli için: sol sürükle = nesne (saç vb.), sağ sürükle = nesne değil"),
]

HANDLE = 6        # tutamaç yarıçapı (px)
HIT = 9           # tıklama hoşgörüsü (px)


class VisualCanvas(QWidget):
    values_changed = Signal(str)
    roi_selected = Signal(object)          # (x, y, w, h) video pikseli

    def __init__(self, ctx, media, video_panel):
        super().__init__()
        self.ctx = ctx
        self.media = media
        self.video = video_panel
        self.tool = "drag"
        self.inverse = False
        self.color_target = "1c"
        self.image: QImage | None = None
        self._img_frame = None
        self.mouse = None              # widget koordinatı
        self.drag = None               # sürükleme durumu sözlüğü
        self.motion = None             # MotionPanel (roi, track) — izleme aracı için
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.StrongFocus)
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.setMinimumSize(160, 90)
        self._timer = QTimer(self)
        self._timer.setSingleShot(True)
        self._timer.setInterval(120)
        self._timer.timeout.connect(self.refresh_image)
        self._retry = 0
        ctx.rows_changed.connect(lambda *_: self.schedule())
        ctx.document_reset.connect(self.schedule)
        ctx.styles_changed.connect(self.schedule)
        ctx.active_changed.connect(lambda *_: self.update())
        media.position_changed.connect(lambda *_: self.schedule(force=True))

    # ------------------------------------------------------------ görüntü
    def schedule(self, force: bool = False):
        if not self.isVisible():
            return
        if force or not self._timer.isActive():
            self._timer.start()

    def refresh_image(self):
        if self.media.index is None:
            self.image = None
            self.update()
            return
        frame = self.media.frame
        v = self.video
        arr = None
        if v.mpv is not None:
            try:
                v.refresh_subs()
                tp = v.mpv.time_pos
                if tp is None or self.media.index.frame_at_seconds(tp) != frame:
                    # mpv aramayı henüz bitirmedi: kısa süre sonra yeniden dene
                    if self._retry < 20:
                        self._retry += 1
                        self._timer.start(30)
                        return
                self._retry = 0
                arr = v.grab_frame(True)
            except Exception:  # noqa: BLE001
                arr = None
        if arr is None and self.media.grabber is not None:
            try:
                arr = self.media.grabber.get(frame)
            except Exception:  # noqa: BLE001
                arr = None
        if arr is not None:
            h, w, _ = arr.shape
            self._buf = arr.copy()
            self.image = QImage(self._buf.data, w, h, 3 * w, QImage.Format_RGB888)
            self._img_frame = frame
        self.update()

    def showEvent(self, e):
        super().showEvent(e)
        self.refresh_image()

    # ------------------------------------------------------------ koordinat
    def play_res(self):
        return self.ctx.doc.play_res()

    def frame_rect(self) -> QRectF:
        idx = self.media.index
        if idx is not None and idx.width and idx.height:
            ar = idx.width * float(idx.sar) / idx.height
        elif self.image is not None:
            ar = self.image.width() / max(1, self.image.height())
        else:
            ar = 16 / 9
        W, H = self.width(), self.height()
        if W / max(1, H) > ar:
            h = H
            w = h * ar
        else:
            w = W
            h = w / ar
        return QRectF((W - w) / 2, (H - h) / 2, w, h)

    def to_script(self, p: QPointF) -> tuple[float, float]:
        r = self.frame_rect()
        rx, ry = self.play_res()
        return (p.x() - r.x()) / r.width() * rx, (p.y() - r.y()) / r.height() * ry

    def to_widget(self, x: float, y: float) -> QPointF:
        r = self.frame_rect()
        rx, ry = self.play_res()
        return QPointF(r.x() + x / rx * r.width(), r.y() + y / ry * r.height())

    def px_per_script(self) -> float:
        return self.frame_rect().width() / self.play_res()[0]

    # ------------------------------------------------------------ satır yardımcıları
    def _ev(self):
        return self.ctx.active_event()

    def _style(self, ev):
        return self.ctx.doc.style_by_name(ev.style) if ev else None

    def _pos(self, ev):
        return T.position(ev, self._style(ev), self.play_res(), self.media.position_ms)

    def _org(self, ev):
        return T.get_org(ev.text) or T.get_pos(ev.text) or self._pos(ev)

    def _commit(self, changes: dict, desc: str):
        row = self.ctx.active
        self.ctx.update_events(changes, desc, coalesce=f"visual:{self.tool}:{row}")

    def _set_text(self, text: str, desc: str):
        self._commit({self.ctx.active: {"text": text}}, desc)

    # ------------------------------------------------------------ çizim
    def paintEvent(self, _e):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.fillRect(self.rect(), Qt.black)
        r = self.frame_rect()
        if self.image is not None:
            p.drawImage(r, self.image)
        ev = self._ev()
        if self.tool == "seed" and self.media.index is not None:
            self._paint_track(p)
            self._paint_seed(p)
        elif self.tool == "track" and self.media.index is not None:
            self._paint_track(p)
        elif ev is not None and self.media.index is not None:
            getattr(self, f"_paint_{self.tool}", lambda *_: None)(p, ev)
        self._paint_status(p, ev)
        p.end()

    def _handle(self, p, pt: QPointF, color="#ffd400", shape="square", size=HANDLE):
        p.setPen(QPen(QColor("#000000"), 1))
        p.setBrush(QBrush(QColor(color)))
        if shape == "circle":
            p.drawEllipse(pt, size, size)
        else:
            p.drawRect(QRectF(pt.x() - size, pt.y() - size, 2 * size, 2 * size))

    def _cross(self, p, pt, color="#ff5c5c", size=10):
        p.setPen(QPen(QColor(color), 2))
        p.drawLine(QPointF(pt.x() - size, pt.y()), QPointF(pt.x() + size, pt.y()))
        p.drawLine(QPointF(pt.x(), pt.y() - size), QPointF(pt.x(), pt.y() + size))

    def _paint_cross(self, p, ev):
        if self.mouse is None:
            return
        p.setPen(QPen(QColor(255, 255, 255, 140), 1, Qt.DashLine))
        r = self.frame_rect()
        p.drawLine(QPointF(r.left(), self.mouse.y()), QPointF(r.right(), self.mouse.y()))
        p.drawLine(QPointF(self.mouse.x(), r.top()), QPointF(self.mouse.x(), r.bottom()))

    def _paint_drag(self, p, ev):
        # seçili diğer satırların konumları
        for row in self.ctx.selected_rows():
            if row == self.ctx.active:
                continue
            o = self.ctx.doc.events[row]
            q = self.to_widget(*T.position(o, self._style(o), self.play_res(), self.media.position_ms))
            self._handle(p, q, "#9aa0a6", "circle", 4)
        mv = T.get_move(ev.text)
        if mv:
            a, b = self.to_widget(mv[0], mv[1]), self.to_widget(mv[2], mv[3])
            p.setPen(QPen(QColor("#ffd400"), 2, Qt.DashLine))
            p.drawLine(a, b)
            self._handle(p, a, "#4caf50")
            self._handle(p, b, "#ff5c5c")
            cur = self.to_widget(*self._pos(ev))
            self._handle(p, cur, "#ffffff", "circle", 3)
        else:
            self._handle(p, self.to_widget(*self._pos(ev)))

    def _paint_org(self, p, ev):
        o = self.to_widget(*self._org(ev))
        self._cross(p, o, "#ff9800", 12)
        return o

    def _paint_rotz(self, p, ev):
        o = self._paint_org(p, ev)
        rad = 70
        p.setPen(QPen(QColor(255, 212, 0, 200), 2))
        p.setBrush(Qt.NoBrush)
        p.drawEllipse(o, rad, rad)
        a = math.radians(T.get_float(ev.text, "frz", self._style_val(ev, "angle")))
        tip = QPointF(o.x() + rad * math.cos(a), o.y() - rad * math.sin(a))
        p.drawLine(o, tip)
        self._handle(p, tip, "#ffd400", "circle", 5)

    def _paint_rotxy(self, p, ev):
        o = self._paint_org(p, ev)
        frx = T.get_float(ev.text, "frx")
        fry = T.get_float(ev.text, "fry")
        p.setPen(QPen(QColor(0, 200, 255, 200), 2))
        p.setBrush(Qt.NoBrush)
        w = 70 * abs(math.cos(math.radians(fry)))
        h = 70 * abs(math.cos(math.radians(frx)))
        p.drawEllipse(o, max(2.0, w), 70)
        p.drawEllipse(o, 70, max(2.0, h))

    def _paint_scale(self, p, ev):
        o = self.to_widget(*self._pos(ev))
        sx = T.get_float(ev.text, "fscx", self._style_val(ev, "scale_x"))
        sy = T.get_float(ev.text, "fscy", self._style_val(ev, "scale_y"))
        hx = QPointF(o.x() + sx, o.y())
        hy = QPointF(o.x(), o.y() - sy)
        p.setPen(QPen(QColor("#ffd400"), 2))
        p.drawLine(o, hx)
        p.drawLine(o, hy)
        self._handle(p, hx, "#ff5c5c")
        self._handle(p, hy, "#4caf50")
        self._handle(p, o, "#ffffff", "circle", 4)

    def _clip_rect(self, ev):
        c = T.get_clip(ev.text)
        if c and c[0] == "rect":
            return c[2]
        return None

    def _paint_clip(self, p, ev):
        rc = self._clip_rect(ev)
        if self.drag and self.drag.get("new"):
            rc = self.drag["rect"]
        if not rc:
            return
        a, b = self.to_widget(rc[0], rc[1]), self.to_widget(rc[2], rc[3])
        rr = QRectF(a, b).normalized()
        path = QPainterPath()
        path.addRect(QRectF(self.rect()))
        path.addRect(rr)
        p.setBrush(QColor(0, 0, 0, 90))
        p.setPen(Qt.NoPen)
        if not self.inverse and not (T.get_clip(ev.text) or ("", False))[1]:
            p.drawPath(path)          # clip dışını karart
        else:
            p.drawRect(rr)             # iclip: içini karart
        p.setPen(QPen(QColor("#ffd400"), 1, Qt.DashLine))
        p.setBrush(Qt.NoBrush)
        p.drawRect(rr)
        for pt in (rr.topLeft(), rr.topRight(), rr.bottomLeft(), rr.bottomRight()):
            self._handle(p, pt, "#ffd400", "square", 4)

    def _vpoints(self, ev):
        c = T.get_clip(ev.text)
        if c and c[0] == "vector":
            return T.parse_drawing(c[2][1])
        return []

    def _paint_vclip(self, p, ev):
        pts = self._vpoints(ev)
        if not pts:
            return
        wp = [self.to_widget(x, y) for _c, x, y in pts]
        path = QPainterPath(wp[0])
        for q in wp[1:]:
            path.lineTo(q)
        path.closeSubpath()
        p.setBrush(QColor(255, 212, 0, 40))
        p.setPen(QPen(QColor("#ffd400"), 1))
        p.drawPath(path)
        for i, (c, _x, _y) in enumerate(pts):
            col = "#ff5c5c" if i == 0 else ("#00c8ff" if c == "b" else "#ffd400")
            self._handle(p, wp[i], col, "square", 4)

    def _paint_eyedrop(self, p, ev):
        if self.mouse is None:
            return
        col = self.pick_color(self.mouse)
        if col is None:
            return
        r, g, b = col
        box = QRectF(self.mouse.x() + 14, self.mouse.y() + 14, 34, 34)
        p.setPen(QPen(Qt.white, 1))
        p.setBrush(QColor(r, g, b))
        p.drawRect(box)

    # ------------------------------------------------------------ izleme aracı
    def _seed_radius(self) -> int:
        """Fırça yarıçapı: ekranda ~6 px (video pikseline çevrilmiş)."""
        r = self.frame_rect()
        vw, _vh = self.video_size()
        return max(1, int(round(6 * vw / max(1.0, r.width()))))

    def video_size(self):
        idx = self.media.index
        if idx is not None and idx.width:
            return idx.width, idx.height
        if self.image is not None:
            return self.image.width(), self.image.height()
        return self.play_res()

    def vid_to_widget(self, x: float, y: float) -> QPointF:
        r = self.frame_rect()
        vw, vh = self.video_size()
        return QPointF(r.x() + x / vw * r.width(), r.y() + y / vh * r.height())

    def widget_to_vid(self, p: QPointF) -> tuple[float, float]:
        r = self.frame_rect()
        vw, vh = self.video_size()
        x = (p.x() - r.x()) / r.width() * vw
        y = (p.y() - r.y()) / r.height() * vh
        return max(0.0, min(vw, x)), max(0.0, min(vh, y))

    def _paint_track(self, p):
        d = self.drag
        if d is not None and d.get("kind") == "roi":
            (x1, y1), (x2, y2) = d["a"], d["b"]
            p.setPen(QPen(QColor("#00e5ff"), 2, Qt.DashLine))
            p.setBrush(Qt.NoBrush)
            p.drawRect(QRectF(self.vid_to_widget(min(x1, x2), min(y1, y2)),
                              self.vid_to_widget(max(x1, x2), max(y1, y2))))
            return
        mo = self.motion
        if mo is None:
            return
        roi = mo.roi
        tr = mo.track
        frame = self.media.frame
        if tr is not None and tr.frames and roi:
            cx, cy = roi[0] + roi[2] / 2, roi[1] + roi[3] / 2
            path = QPainterPath()
            for i, f in enumerate(tr.frames):
                pt = self.vid_to_widget(*f.map(cx, cy))
                path.moveTo(pt) if i == 0 else path.lineTo(pt)
            p.setPen(QPen(QColor(255, 255, 255, 140), 1))
            p.setBrush(Qt.NoBrush)
            p.drawPath(path)
            for f in tr.frames:
                if not f.ok:
                    self._handle(p, self.vid_to_widget(*f.map(cx, cy)), "#ff4040", "circle", 2)
        if roi:
            x, y, w, h = roi
            corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
            if tr is not None and tr.covers(frame):
                f = tr.at(frame)
                corners = [f.map(*c) for c in corners]
                col = "#ffd400" if f.ok else "#ff4040"
            elif tr is not None:
                col = "#808080"
            else:
                col = "#00e5ff"
            poly = QPainterPath()
            pts = [self.vid_to_widget(*c) for c in corners]
            poly.moveTo(pts[0])
            for q in pts[1:]:
                poly.lineTo(q)
            poly.closeSubpath()
            p.setPen(QPen(QColor(col), 2))
            p.setBrush(Qt.NoBrush)
            p.drawPath(poly)
            if tr is not None and tr.covers(frame):
                f = tr.at(frame)
                c = self.vid_to_widget(*f.map(x + w / 2, y + h / 2))
                self._cross(p, c, col, 6)
        occ = getattr(mo, "occlusion", None)
        fo = occ.frames.get(frame) if occ is not None else None
        if fo is not None and fo.polys:
            p.setPen(QPen(QColor("#ff3b3b"), 1))
            p.setBrush(QColor(255, 40, 40, 90))
            for poly in fo.polys:
                path = QPainterPath()
                pts = [self.vid_to_widget(px, py) for px, py in poly]
                path.moveTo(pts[0])
                for q in pts[1:]:
                    path.lineTo(q)
                path.closeSubpath()
                p.drawPath(path)

    def _paint_seed(self, p):
        mo = self.motion
        if mo is None or mo.seed_pos is None or mo.seed_frame != self.media.frame:
            return
        import numpy as np
        h, w = mo.seed_pos.shape
        rgba = np.zeros((h, w, 4), np.uint8)
        rgba[mo.seed_pos > 0] = (40, 220, 80, 150)
        rgba[mo.seed_neg > 0] = (230, 50, 50, 150)
        self._seed_buf = rgba
        img = QImage(rgba.data, w, h, 4 * w, QImage.Format_RGBA8888)
        p.drawImage(self.frame_rect(), img)

    def _paint_status(self, p, ev):
        txt = self.status_text(ev)
        if not txt:
            return
        f = QFont(self.font())
        f.setPointSize(max(8, f.pointSize() - 1))
        p.setFont(f)
        fm = p.fontMetrics()
        w = fm.horizontalAdvance(txt) + 12
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(0, 0, 0, 170))
        p.drawRect(QRectF(4, 4, w, fm.height() + 6))
        p.setPen(QColor("#ffffff"))
        p.drawText(QPointF(10, 4 + fm.ascent() + 3), txt)

    def status_text(self, ev) -> str:
        name = dict((k, n) for k, n, _ in TOOLS).get(self.tool, self.tool)
        parts = [name]
        if self.mouse is not None:
            x, y = self.to_script(self.mouse)
            parts.append(f"{T.fmt(x, 0)},{T.fmt(y, 0)}")
        if self.tool == "seed":
            mo = self.motion
            parts.append("sol: nesne · sağ: nesne değil")
            if mo is not None and mo.seed_frame is not None:
                parts.append(f"tohum karesi {mo.seed_frame}")
            return "  ·  ".join(parts)
        if self.tool == "track":
            mo = self.motion
            if self.mouse is not None:
                vx, vy = self.widget_to_vid(self.mouse)
                parts[-1] = f"video {T.fmt(vx, 0)},{T.fmt(vy, 0)}"
            if mo is not None and mo.track is not None and mo.track.covers(self.media.frame):
                f = mo.track.at(self.media.frame)
                parts.append(f"kare {f.frame}: ölçek {f.scale * 100:.1f}% · dönüş {f.rot:.2f}° · "
                             f"güven {f.conf:.2f}{'' if f.ok else ' (GÜVENSİZ)'}")
                occ = getattr(mo, "occlusion", None)
                fo = occ.frames.get(self.media.frame) if occ is not None else None
                if fo is not None:
                    parts.append(f"görünürlük %{fo.visibility * 100:.0f}" + ("" if fo.ok else " (belirsiz)"))
            elif mo is None or not mo.roi:
                parts.append("izlenecek alanı sürükleyerek çizin")
            return "  ·  ".join(parts)
        if ev is not None:
            if self.tool == "drag":
                parts.append("pos " + ",".join(T.fmt(v) for v in self._pos(ev)))
            elif self.tool == "rotz":
                parts.append(f"frz {T.fmt(T.get_float(ev.text, 'frz', self._style_val(ev, 'angle')))}")
            elif self.tool == "rotxy":
                parts.append(f"frx {T.fmt(T.get_float(ev.text, 'frx'))} fry {T.fmt(T.get_float(ev.text, 'fry'))}")
            elif self.tool == "scale":
                parts.append(f"fscx {T.fmt(T.get_float(ev.text, 'fscx', self._style_val(ev, 'scale_x')))} "
                             f"fscy {T.fmt(T.get_float(ev.text, 'fscy', self._style_val(ev, 'scale_y')))}")
        return "  ·  ".join(parts)

    def _style_val(self, ev, attr):
        st = self._style(ev)
        return float(getattr(st, attr)) if st is not None else {"scale_x": 100.0, "scale_y": 100.0}.get(attr, 0.0)

    # ------------------------------------------------------------ renk
    def pick_color(self, wpt: QPointF):
        """Altyazısız karenin pikseli (r, g, b)."""
        if self.media.grabber is None:
            return None
        try:
            arr = self.media.grabber.get(self.media.frame)
        except Exception:  # noqa: BLE001
            return None
        r = self.frame_rect()
        if not r.contains(wpt):
            return None
        h, w, _ = arr.shape
        x = int((wpt.x() - r.x()) / r.width() * w)
        y = int((wpt.y() - r.y()) / r.height() * h)
        x, y = max(0, min(w - 1, x)), max(0, min(h - 1, y))
        return tuple(int(v) for v in arr[y, x])

    # ------------------------------------------------------------ fare
    def mousePressEvent(self, e):
        if self.tool == "seed" and self.media.index is not None and self.motion is not None:
            v = self.widget_to_vid(e.position())
            pos = e.button() != Qt.RightButton
            self.drag = {"kind": "seed", "last": v, "pos": pos}
            self.motion.paint_seed(v[0], v[1], v[0], v[1], pos, self._seed_radius())
            self.update()
            return
        if self.tool == "track" and self.media.index is not None:
            self.setFocus()
            v = self.widget_to_vid(e.position())
            mo = self.motion
            mods = e.modifiers()
            if (mods & Qt.ControlModifier) and mo is not None and mo.track is not None \
                    and mo.track.covers(self.media.frame):
                # elle düzeltme: bu kare (Shift: referanstan uzaktaki tüm kareler de)
                mo.begin_track_edit()
                self.drag = {"kind": "trackfix", "start": v, "base": mo.track,
                             "following": bool(mods & Qt.ShiftModifier)}
                return
            self.drag = {"kind": "roi", "a": v, "b": v}
            self.update()
            return
        ev = self._ev()
        if ev is None or self.media.index is None:
            return
        self.setFocus()
        m = e.position()
        sx, sy = self.to_script(m)
        mods = e.modifiers()
        tool = self.tool
        self.drag = None
        if tool == "drag":
            mv = T.get_move(ev.text)
            if mv:
                a, b = self.to_widget(mv[0], mv[1]), self.to_widget(mv[2], mv[3])
                which = "a" if _dist(a, m) <= HIT else ("b" if _dist(b, m) <= HIT else "both")
                self.drag = {"kind": "move", "which": which, "start": (sx, sy), "orig": mv,
                             "others": self._others_snapshot()}
            else:
                x0, y0 = self._pos(ev)
                self.drag = {"kind": "pos", "start": (sx, sy), "orig": (x0, y0),
                             "to_move": bool(mods & Qt.ControlModifier), "others": self._others_snapshot()}
        elif tool in ("rotz", "rotxy"):
            o = self._org(ev)
            if _dist(self.to_widget(*o), m) <= HIT + 4:
                self.drag = {"kind": "org", "start": (sx, sy), "orig": o}
            elif tool == "rotz":
                ow = self.to_widget(*o)
                self.drag = {"kind": "rotz", "a0": math.degrees(math.atan2(m.y() - ow.y(), m.x() - ow.x())),
                             "orig": T.get_float(ev.text, "frz", self._style_val(ev, "angle"))}
            else:
                self.drag = {"kind": "rotxy", "start": (m.x(), m.y()),
                             "orig": (T.get_float(ev.text, "frx"), T.get_float(ev.text, "fry"))}
        elif tool == "scale":
            o = self.to_widget(*self._pos(ev))
            sxv = T.get_float(ev.text, "fscx", self._style_val(ev, "scale_x"))
            syv = T.get_float(ev.text, "fscy", self._style_val(ev, "scale_y"))
            hx, hy = QPointF(o.x() + sxv, o.y()), QPointF(o.x(), o.y() - syv)
            which = "x" if _dist(hx, m) <= HIT else ("y" if _dist(hy, m) <= HIT else "both")
            self.drag = {"kind": "scale", "which": which, "start": (m.x(), m.y()), "orig": (sxv, syv)}
        elif tool == "clip":
            rc = self._clip_rect(ev)
            if rc:
                corners = {"tl": (rc[0], rc[1]), "tr": (rc[2], rc[1]), "bl": (rc[0], rc[3]), "br": (rc[2], rc[3])}
                hit = next((k for k, c in corners.items() if _dist(self.to_widget(*c), m) <= HIT), None)
                if hit:
                    self.drag = {"kind": "clipcorner", "corner": hit, "orig": rc}
                elif rc[0] <= sx <= rc[2] and rc[1] <= sy <= rc[3]:
                    self.drag = {"kind": "clipmove", "start": (sx, sy), "orig": rc}
            if self.drag is None:
                self.drag = {"kind": "clipnew", "new": True, "start": (sx, sy), "rect": (sx, sy, sx, sy)}
        elif tool == "vclip":
            self._vclip_press(ev, m, sx, sy, e.button(), mods)
        elif tool == "eyedrop":
            self._eyedrop(m)
        self.update()

    def _others_snapshot(self):
        out = {}
        for row in self.ctx.selected_rows():
            if row == self.ctx.active:
                continue
            o = self.ctx.doc.events[row]
            out[row] = (o.text, T.position(o, self._style(o), self.play_res(), None))
        return out

    def mouseMoveEvent(self, e):
        self.mouse = e.position()
        d = self.drag
        if d is not None and d.get("kind") == "roi":
            d["b"] = self.widget_to_vid(e.position())
            self.update()
            return
        if d is not None and d.get("kind") == "seed":
            v = self.widget_to_vid(e.position())
            self.motion.paint_seed(d["last"][0], d["last"][1], v[0], v[1], d["pos"], self._seed_radius())
            d["last"] = v
            self.update()
            return
        if d is not None and d.get("kind") == "trackfix":
            vx, vy = self.widget_to_vid(e.position())
            delta = complex(vx - d["start"][0], vy - d["start"][1])
            self.motion.shift_track(self.media.frame, delta, d["following"], base=d["base"])
            self.update()
            return
        ev = self._ev()
        if d is None or ev is None:
            self.update()
            return
        m = e.position()
        sx, sy = self.to_script(m)
        k = d["kind"]
        text = ev.text
        if k == "pos":
            dx, dy = sx - d["start"][0], sy - d["start"][1]
            x0, y0 = d["orig"]
            if d["to_move"]:
                text = T.remove(text, "pos")
                text = T.set_move(text, x0, y0, x0 + dx, y0 + dy)
            else:
                text = T.set_tag(text, "pos", f"({T.fmt(x0 + dx)},{T.fmt(y0 + dy)})")
            self._apply_with_others(text, dx, dy, d, "Görsel: taşı")
            return
        if k == "move":
            dx, dy = sx - d["start"][0], sy - d["start"][1]
            x1, y1, x2, y2, t1, t2 = d["orig"]
            if d["which"] in ("a", "both"):
                x1, y1 = x1 + dx, y1 + dy
            if d["which"] in ("b", "both"):
                x2, y2 = x2 + dx, y2 + dy
            text = T.set_move(text, x1, y1, x2, y2, t1, t2)
            self._apply_with_others(text, dx, dy, d if d["which"] == "both" else {"others": {}}, "Görsel: \\move")
            return
        if k == "org":
            dx, dy = sx - d["start"][0], sy - d["start"][1]
            text = T.set_tag(text, "org", f"({T.fmt(d['orig'][0] + dx)},{T.fmt(d['orig'][1] + dy)})")
            self._set_text(text, "Görsel: \\org")
        elif k == "rotz":
            ow = self.to_widget(*self._org(ev))
            a = math.degrees(math.atan2(m.y() - ow.y(), m.x() - ow.x()))
            val = T.norm_angle(d["orig"] - (a - d["a0"]))
            if e.modifiers() & Qt.ShiftModifier:
                val = round(val / 15.0) * 15.0
            self._set_text(T.set_tag(text, "frz", T.fmt(val)), "Görsel: \\frz")
        elif k == "rotxy":
            dx, dy = m.x() - d["start"][0], m.y() - d["start"][1]
            frx = T.norm_angle(d["orig"][0] - dy * 0.5)
            fry = T.norm_angle(d["orig"][1] + dx * 0.5)
            text = T.set_tag(T.set_tag(text, "frx", T.fmt(frx)), "fry", T.fmt(fry))
            self._set_text(text, "Görsel: \\frx \\fry")
        elif k == "scale":
            dx, dy = m.x() - d["start"][0], d["start"][1] - m.y()
            sxv, syv = d["orig"]
            if d["which"] == "x":
                sxv = max(0.0, sxv + dx)
            elif d["which"] == "y":
                syv = max(0.0, syv + dy)
            else:
                f = max(0.0, 1 + dx / 100.0)
                sxv, syv = sxv * f, syv * f
            text = T.set_tag(T.set_tag(text, "fscx", T.fmt(sxv)), "fscy", T.fmt(syv))
            self._set_text(text, "Görsel: ölçek")
        elif k == "clipnew":
            x0, y0 = d["start"]
            d["rect"] = (min(x0, sx), min(y0, sy), max(x0, sx), max(y0, sy))
            if abs(sx - x0) >= 2 and abs(sy - y0) >= 2:
                self._set_text(T.set_rect_clip(text, *d["rect"], inverse=self.inverse), "Görsel: clip")
        elif k == "clipmove":
            dx, dy = sx - d["start"][0], sy - d["start"][1]
            x1, y1, x2, y2 = d["orig"]
            self._set_text(T.set_rect_clip(text, x1 + dx, y1 + dy, x2 + dx, y2 + dy, inverse=self._inv(ev)),
                           "Görsel: clip")
        elif k == "clipcorner":
            x1, y1, x2, y2 = d["orig"]
            c = d["corner"]
            if "l" in c:
                x1 = sx
            else:
                x2 = sx
            if "t" in c:
                y1 = sy
            else:
                y2 = sy
            self._set_text(T.set_rect_clip(text, x1, y1, x2, y2, inverse=self._inv(ev)), "Görsel: clip")
        elif k == "vpoint":
            pts = d["pts"]
            pts[d["i"]][1], pts[d["i"]][2] = round(sx), round(sy)
            self._set_text(T.set_vector_clip(text, pts, inverse=self._inv(ev), scale=d["scale"]),
                           "Görsel: vektör clip")
        self.update()

    def _inv(self, ev):
        c = T.get_clip(ev.text)
        return c[1] if c else self.inverse

    def _apply_with_others(self, text, dx, dy, d, desc):
        changes = {self.ctx.active: {"text": text}}
        for row, (otext, (ox, oy)) in d.get("others", {}).items():
            mv = T.get_move(otext)
            if mv:
                x1, y1, x2, y2, t1, t2 = mv
                changes[row] = {"text": T.set_move(otext, x1 + dx, y1 + dy, x2 + dx, y2 + dy, t1, t2)}
            else:
                changes[row] = {"text": T.set_tag(otext, "pos", f"({T.fmt(ox + dx)},{T.fmt(oy + dy)})")}
        self._commit(changes, desc)
        self.update()

    def mouseReleaseEvent(self, _e):
        d = self.drag
        if d is not None and d.get("kind") in ("trackfix", "seed"):
            self.drag = None
            self.update()
            return
        if d is not None and d.get("kind") == "roi":
            self.drag = None
            (x1, y1), (x2, y2) = d["a"], d["b"]
            x, y, w, h = min(x1, x2), min(y1, y2), abs(x2 - x1), abs(y2 - y1)
            if w >= 4 and h >= 4:
                self.roi_selected.emit((round(x, 1), round(y, 1), round(w, 1), round(h, 1)))
            self.update()
            return
        if self.drag is not None:
            self.drag = None
            self.ctx.undo.break_coalesce()
            self._timer.start(0)
        self.update()

    def mouseDoubleClickEvent(self, e):
        ev = self._ev()
        if ev is None or self.tool not in ("drag", "cross"):
            return
        sx, sy = self.to_script(e.position())
        text = T.remove(ev.text, "move")
        self._set_text(T.set_tag(text, "pos", f"({T.fmt(sx)},{T.fmt(sy)})"), "Görsel: konum")
        self.ctx.undo.break_coalesce()

    def keyPressEvent(self, e):
        if e.key() == Qt.Key_Escape:
            self.video.set_tool("video")
        elif e.key() in (Qt.Key_Left, Qt.Key_Right):
            self.video.step(-1 if e.key() == Qt.Key_Left else 1)
        else:
            super().keyPressEvent(e)

    def leaveEvent(self, _e):
        self.mouse = None
        self.update()

    # ------------------------------------------------------------ vektör clip
    def _vclip_press(self, ev, m, sx, sy, button, mods):
        c = T.get_clip(ev.text)
        scale = c[2][0] if c and c[0] == "vector" else 1
        pts = self._vpoints(ev)
        inv = c[1] if c else self.inverse
        hit = next((i for i, (_c, x, y) in enumerate(pts) if _dist(self.to_widget(x, y), m) <= HIT), None)
        if button == Qt.RightButton:
            if hit is not None:
                del pts[hit]
                if pts:
                    pts[0][0] = "m"
                    text = T.set_vector_clip(ev.text, pts, inverse=inv, scale=scale)
                else:
                    text = T.remove(T.remove(ev.text, "clip"), "iclip")
                self._set_text(text, "Görsel: vektör clip")
                self.ctx.undo.break_coalesce()
            return
        if hit is not None:
            self.drag = {"kind": "vpoint", "i": hit, "pts": [list(p) for p in pts], "scale": scale}
            return
        new = ["l", round(sx), round(sy)]
        if not pts:
            new[0] = "m"
            pts = [new]
            idx = 0
        elif mods & Qt.ControlModifier and len(pts) >= 2:
            # en yakın kenara sok
            best, bd = 0, 1e18
            for i in range(len(pts)):
                a = pts[i]
                b = pts[(i + 1) % len(pts)]
                dd = _seg_dist((sx, sy), (a[1], a[2]), (b[1], b[2]))
                if dd < bd - 1e-9:          # eşitlikte önceki kenar (iki noktada iki kenar aynıdır)
                    best, bd = i, dd
            pts.insert(best + 1, new)
            idx = best + 1
        else:
            pts.append(new)
            idx = len(pts) - 1
        self._set_text(T.set_vector_clip(ev.text, pts, inverse=inv, scale=scale), "Görsel: vektör clip")
        self.drag = {"kind": "vpoint", "i": idx, "pts": [list(p) for p in pts], "scale": scale}

    # ------------------------------------------------------------ damlalık
    def _eyedrop(self, m):
        col = self.pick_color(m)
        if col is None:
            return
        r, g, b = col
        val = f"&H{b:02X}{g:02X}{r:02X}&"
        changes = {}
        for row in self.ctx.selected_rows():
            changes[row] = {"text": T.set_tag(self.ctx.doc.events[row].text, self.color_target, val)}
        self.ctx.update_events(changes, f"Damlalık \\{self.color_target}")
        self.ctx.undo.break_coalesce()
        try:
            QApplication.clipboard().setText(val)
        except Exception:  # noqa: BLE001
            pass
        self.ctx.status.emit(f"Renk {val} (R{r} G{g} B{b}) → \\{self.color_target}, panoya kopyalandı")


def _dist(a: QPointF, b: QPointF) -> float:
    return math.hypot(a.x() - b.x(), a.y() - b.y())


def _seg_dist(p, a, b) -> float:
    (px, py), (ax, ay), (bx, by) = p, a, b
    dx, dy = bx - ax, by - ay
    L = dx * dx + dy * dy
    t = 0.0 if L == 0 else max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / L))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))
