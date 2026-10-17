"""Hareket takibi bölmesi (assmotion'ın uygulama içi hali).

Akış: 1) aralığı seçili satırlardan al, referans kareyi ayarla, alanı videoda çiz
(İzleme aracı) ya da satırın \\clip'inden al  2) İzle (arka planda; ilerleme + iptal)
3) eğrilere bak, güvensiz kareler kırmızı; tıklayınca video o kareye gider
4) isteğe bağlı: önden geçen nesneleri bul (örtme -> \\iclip, görünürlük eğrisi)
5) seçili satırlara uygula (tek geri al adımı). İz .assmotiontrack olarak saklanır (örtme
sonucu dahil); Mocha/After Effects verisi içe ve dışa aktarılır.
Toplu: seçili her satır kendi aralığı ve alanıyla (\\clip ya da metin kutusu) izlenip uygulanır.
"""

from __future__ import annotations

import os
import threading

from PySide6.QtCore import QPointF, Qt, QTimer, Signal
from PySide6.QtGui import QColor, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QFileDialog, QFormLayout, QGroupBox,
                               QHBoxLayout, QLabel, QProgressBar, QPushButton, QSpinBox,
                               QVBoxLayout, QWidget)

from ..core import motion as M
from ..core import tags as T

METHODS = [("points", "similarity", "Noktalar: konum + ölçek + dönüş"),
           ("points", "translation", "Noktalar: yalnız konum"),
           ("ecc", "similarity", "ECC: konum + ölçek + dönüş (desenli, örtmesiz planlarda en hassas)"),
           ("color", "scale", "Renk anahtarı: konum + ölçek (düz renkli tabela)"),
           ("template", "translation", "Şablon: yalnız konum (düz tabelalar)")]


class CurvePlot(QWidget):
    """x/y kayması, ölçek, dönüş eğrileri; tıklama kareye gider."""
    frame_clicked = Signal(int)

    SERIES = [("Δx", "#4fc3f7"), ("Δy", "#81c784"), ("ölçek", "#ffb74d"), ("dönüş", "#e57373")]

    def __init__(self):
        super().__init__()
        self.track = None
        self.roi = None
        self.current = None
        self.visibility = None          # {kare: 0..1}
        self.setMinimumHeight(110)

    def set_visibility(self, vis):
        self.visibility = vis
        self.update()

    def set_track(self, tr, roi):
        self.track, self.roi = tr, roi
        self.update()

    def set_current(self, f):
        self.current = f
        self.update()

    def _series(self):
        tr = self.track
        roi = self.roi or (0, 0, 0, 0)
        cx, cy = roi[0] + roi[2] / 2, roi[1] + roi[3] / 2
        xs, ys, ss, rs = [], [], [], []
        for f in tr.frames:
            x, y = f.map(cx, cy)
            xs.append(x - cx)
            ys.append(y - cy)
            ss.append(f.scale * 100)
            rs.append(f.rot)
        return [xs, ys, ss, rs]

    def paintEvent(self, _e):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.fillRect(self.rect(), self.palette().base())
        tr = self.track
        if tr is None or not tr.frames:
            p.setPen(self.palette().text().color())
            p.drawText(self.rect(), Qt.AlignCenter, "Henüz iz yok")
            return
        W, H = self.width(), self.height()
        L, R, TOP, B = 6, 6, 16, 6
        n = len(tr.frames)

        def X(i):
            return L + (W - L - R) * (i / max(1, n - 1))
        for k, (vals, (name, col)) in enumerate(zip(self._series(), self.SERIES)):
            lo, hi = min(vals), max(vals)
            span = hi - lo if hi - lo > 1e-6 else 1.0
            path = QPainterPath()
            for i, v in enumerate(vals):
                y = TOP + (H - TOP - B) * (1 - (v - lo) / span) if hi - lo > 1e-6 else (H + TOP) / 2
                pt = QPointF(X(i), y)
                path.moveTo(pt) if i == 0 else path.lineTo(pt)
            p.setPen(QPen(QColor(col), 1.5))
            p.drawPath(path)
            p.drawText(QPointF(L + k * (W - L - R) / 4, 12), f"{name} {lo:.1f}…{hi:.1f}")
        if self.visibility:
            path = QPainterPath()
            first = True
            for i, f in enumerate(tr.frames):
                v = self.visibility.get(f.frame)
                if v is None:
                    continue
                pt = QPointF(X(i), TOP + (H - TOP - B) * (1 - v))
                path.moveTo(pt) if first else path.lineTo(pt)
                first = False
            p.setPen(QPen(QColor("#ba68c8"), 2, Qt.DotLine))
            p.drawPath(path)
            p.drawText(QPointF(W - 120, H - 8), "görünürlük 0…100%")
        p.setPen(QPen(QColor("#ff4040"), 2))
        for i, f in enumerate(tr.frames):
            if not f.ok:
                p.drawLine(QPointF(X(i), H - 6), QPointF(X(i), H - 1))
        if self.current is not None and tr.covers(self.current):
            i = self.current - tr.start_frame
            p.setPen(QPen(self.palette().text().color(), 1, Qt.DashLine))
            p.drawLine(QPointF(X(i), TOP), QPointF(X(i), H))
        i = tr.ref_frame - tr.start_frame
        p.setPen(QPen(QColor("#d09000"), 3))
        p.drawLine(QPointF(X(i), TOP), QPointF(X(i), TOP + 8))

    def mousePressEvent(self, e):
        tr = self.track
        if tr is None or not tr.frames:
            return
        n = len(tr.frames)
        u = (e.position().x() - 6) / max(1, self.width() - 12)
        i = int(round(max(0.0, min(1.0, u)) * (n - 1)))
        self.frame_clicked.emit(tr.frames[i].frame)


class MotionPanel(QWidget):
    def __init__(self, ctx, media, video_panel, win=None):
        super().__init__()
        self.ctx = ctx
        self.media = media
        self.video = video_panel
        self.win = win
        self.track: M.MotionTrack | None = None
        self.occlusion = None           # media.occlude.OcclusionResult
        self.batch_jobs = None
        self.track_undo: list = []      # elle düzeltme / yumuşatma öncesi izler
        self.seed_pos = None            # tohum maskeleri (video pikseli, uint8)
        self.seed_neg = None
        self.seed_frame = None
        self.qc_result = None
        self.roi = None
        self.busy = False
        self.error = None
        self._result = None
        self._cancel = False
        self._prog = (0.0, 0)
        self.last_warnings: list[str] = []
        video_panel.canvas.motion = self
        video_panel.canvas.roi_selected.connect(self.set_roi)
        cfg = dict(ctx.config["motion"] or {})

        # ---- 1. aralık + alan
        g1 = QGroupBox("1 · Aralık ve alan")
        self.sp_start, self.sp_end, self.sp_ref = QSpinBox(), QSpinBox(), QSpinBox()
        for sp in (self.sp_start, self.sp_end, self.sp_ref):
            sp.setRange(0, 9_999_999)
            sp.setKeyboardTracking(False)
        b_from_lines = QPushButton("Seçili satırlardan")
        b_from_lines.setToolTip("Başlangıç/bitiş = seçili satırların kare aralığı; referans = geçerli kare")
        b_from_lines.clicked.connect(self.range_from_lines)
        b_ref_cur = QPushButton("Geçerli kare")
        b_ref_cur.setToolTip("Referans kareyi videodaki geçerli kare yap")
        b_ref_cur.clicked.connect(lambda: self.sp_ref.setValue(self.media.frame))
        row1 = QHBoxLayout()
        for lab, w in (("Baş", self.sp_start), ("Son", self.sp_end), ("Ref", self.sp_ref)):
            row1.addWidget(QLabel(lab))
            row1.addWidget(w)
        row1b = QHBoxLayout()
        row1b.addWidget(b_from_lines)
        row1b.addWidget(b_ref_cur)
        self.roi_lbl = QLabel("Alan: —")
        b_draw = QPushButton("Videoda çiz")
        b_draw.setToolTip("Video bölmesinde İzleme aracını açar; izlenecek alanı sürükleyerek çizin")
        b_draw.clicked.connect(self.draw_roi)
        b_clip = QPushButton("Satırın \\clip'inden")
        b_clip.clicked.connect(self.roi_from_clip)
        row1c = QHBoxLayout()
        row1c.addWidget(b_draw)
        row1c.addWidget(b_clip)
        self.method = QComboBox()
        for _m, _md, label in METHODS:
            self.method.addItem(label)
        self.method.setCurrentIndex(int(cfg.get("method", 0)) % len(METHODS))
        self.b_track = QPushButton("İzle")
        self.b_track.setDefault(True)
        self.b_track.clicked.connect(self.start_tracking)
        self.b_cancel = QPushButton("İptal")
        self.b_cancel.setEnabled(False)
        self.b_cancel.clicked.connect(lambda: setattr(self, "_cancel", True))
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.bar.setTextVisible(False)
        self.bar.setMaximumHeight(8)
        row1d = QHBoxLayout()
        row1d.addWidget(self.method, 1)
        row1d.addWidget(self.b_track)
        row1d.addWidget(self.b_cancel)
        l1 = QVBoxLayout(g1)
        l1.addLayout(row1)
        l1.addLayout(row1b)
        l1.addWidget(self.roi_lbl)
        l1.addLayout(row1c)
        l1.addLayout(row1d)
        l1.addWidget(self.bar)

        # ---- 2. iz
        g2 = QGroupBox("2 · İz")
        self.summary = QLabel("Henüz iz yok")
        self.summary.setWordWrap(True)
        self.plot = CurvePlot()
        self.plot.frame_clicked.connect(lambda f: self.video.seek_frame(f))
        b_open = QPushButton("Aç…")
        b_open.setToolTip(".assmotiontrack ya da AE Keyframe Data (.txt) dosyası")
        b_open.clicked.connect(self.open_dialog)
        b_save = QPushButton("Kaydet…")
        b_save.clicked.connect(self.save_dialog)
        b_paste = QPushButton("AE yapıştır")
        b_paste.setToolTip("Panodaki Mocha / After Effects Keyframe Data'yı içe al "
                           "(veri seçili satırın ilk karesinden başlar)")
        b_paste.clicked.connect(self.paste_ae)
        b_copy = QPushButton("AE kopyala")
        b_copy.setToolTip("İzi AE Keyframe Data olarak panoya kopyala (Aegisub-Motion'a yapıştırılabilir)")
        b_copy.clicked.connect(self.copy_ae)
        self.b_undo_track = QPushButton("İzi geri al")
        self.b_undo_track.setToolTip("Tuvaldeki elle düzeltmeyi geri al (İzleme aracında Ctrl+sürükle: bu kare; "
                                     "Ctrl+Shift+sürükle: bu kare ve referanstan uzaktaki tüm kareler)")
        self.b_undo_track.clicked.connect(self.undo_track)
        self.b_undo_track.setEnabled(False)
        row2 = QHBoxLayout()
        for b in (b_open, b_save, b_paste, b_copy, self.b_undo_track):
            row2.addWidget(b)
        l2 = QVBoxLayout(g2)
        l2.addWidget(self.summary)
        l2.addWidget(self.plot, 1)
        l2.addLayout(row2)

        # ---- örtme
        g_occ = QGroupBox("Örtme · önden geçen nesneler")
        self.occ_thr = QSpinBox()
        self.occ_thr.setRange(6, 80)
        self.occ_thr.setValue(int(cfg.get("occ_threshold", 22)))
        self.occ_thr.setPrefix("ΔE ")
        self.occ_thr.setToolTip("Renk farkı eşiği (CIELAB). Işık değişen sahnede yükseltin")
        self.occ_grow = QSpinBox()
        self.occ_grow.setRange(0, 12)
        self.occ_grow.setValue(int(cfg.get("occ_grow", 2)))
        self.occ_grow.setSuffix(" px")
        self.occ_grow.setToolTip("Maskeyi bu kadar genişlet (kenarda tabela sızmasın)")
        self.occ_model = QComboBox()
        self.occ_model.addItems(["Ortanca model (önerilen)", "Yalnız referans kare"])
        self.occ_model.setToolTip("Ortanca: karelerden temiz görünüm çıkarılır; referans karede örtme olsa da çalışır")
        self.occ_model.setCurrentIndex(int(cfg.get("occ_model", 0)) % 2)
        self.occ_method = QComboBox()
        self.occ_method.addItems(["Hareket farkı (iz gerekir)", "Renk modeli — tohum çiz (saç, el, nesne)"])
        self.occ_method.setToolTip("Hareket farkı: iz ile çarpıtılmış temiz görünümden sapan pikseller.\n"
                                   "Renk modeli (hairtrack): İzleme aracında 'Tohum' ile nesnenin üstüne sol tık, "
                                   "çevresine sağ tıkla çiz; tabela duruyorsa da çalışır, iz gerekmez.")
        self.occ_method.setCurrentIndex(int(cfg.get("occ_method", 0)) % 2)
        self.b_seed = QPushButton("Tohum çiz")
        self.b_seed.setToolTip("Video bölmesinde Tohum aracını açar: sol sürükle = nesne (yeşil), "
                               "sağ sürükle = nesne değil (kırmızı). Tohumlar çizildiği karede geçerlidir.")
        self.b_seed.clicked.connect(self.draw_seeds)
        self.b_seed_clear = QPushButton("Tohumları temizle")
        self.b_seed_clear.clicked.connect(self.clear_seeds)
        self.b_occ = QPushButton("Örtenleri bul")
        self.b_occ.clicked.connect(self.start_occlusion)
        self.occ_lbl = QLabel("—")
        self.occ_lbl.setWordWrap(True)
        ro = QHBoxLayout()
        ro.addWidget(self.occ_thr)
        ro.addWidget(self.occ_grow)
        ro.addWidget(self.occ_model, 1)
        ro.addWidget(self.b_occ)
        rs = QHBoxLayout()
        rs.addWidget(self.occ_method, 1)
        rs.addWidget(self.b_seed)
        rs.addWidget(self.b_seed_clear)
        lo = QVBoxLayout(g_occ)
        lo.addLayout(rs)
        lo.addLayout(ro)
        lo.addWidget(self.occ_lbl)

        # ---- 3. uygula
        g3 = QGroupBox("3 · Seçili satırlara uygula")
        self.c_pos = QCheckBox("Konum")
        self.c_scale = QCheckBox("Ölçek")
        self.c_rot = QCheckBox("Dönüş")
        self.c_border = QCheckBox("Kenarlık/gölge/bulanıklık")
        self.c_clip = QCheckBox("Clip")
        for cb, key, dv in ((self.c_pos, "position", True), (self.c_scale, "scale", True),
                            (self.c_rot, "rotation", True), (self.c_border, "border", True),
                            (self.c_clip, "clip", True)):
            cb.setChecked(bool(cfg.get(key, dv)))
        self.mode = QComboBox()
        self.mode.addItems(["Kare kare (her kareye bir satır)", "Sıkıştırılmış (\\move + \\t)"])
        self.mode.setCurrentIndex(1 if cfg.get("mode") == "compact" else 0)
        self.mode.setToolTip("Sıkıştırılmış: doğrusal giden parçalar tek satır; ±0.5 px / ±0.5% / ±0.3° içinde kalır")
        self.smooth = QSpinBox()
        self.smooth.setRange(0, 12)
        self.smooth.setValue(int(cfg.get("smooth", 0)))
        self.smooth.setSuffix(" kare")
        self.smooth.setToolTip("Kayan ortalama yarıçapı (titremeyi azaltır; 0 = kapalı)")
        self.refmode = QComboBox()
        self.refmode.addItems(["İzin referans karesi", "Satırın ilk karesi"])
        self.refmode.setToolTip("Satırın doğru yerleştirildiği kare (Aegisub-Motion: ilk kare)")
        self.refmode.setCurrentIndex(int(cfg.get("refmode", 0)) % 2)
        self.c_occ = QCheckBox("Örtme \\iclip'i")
        self.c_occ.setChecked(bool(cfg.get("use_occlusion", True)))
        self.c_occ.setToolTip("Bulunduysa önden geçen nesnenin olduğu karelerde satıra \\iclip yazılır")
        self.c_hide = QCheckBox("Görünürlük şunun altındaysa satır üretme:")
        self.c_hide.setChecked(bool(cfg.get("hide", False)))
        self.hide_pct = QSpinBox()
        self.hide_pct.setRange(1, 95)
        self.hide_pct.setValue(int(cfg.get("hide_pct", 15)))
        self.hide_pct.setSuffix(" %")
        self.b_apply = QPushButton("Uygula")
        self.b_apply.clicked.connect(self.apply)
        self.b_batch = QPushButton("Toplu: her satırı ayrı izle ve uygula")
        self.b_batch.setToolTip("Seçili her satır kendi kare aralığında ve kendi alanıyla (dikdörtgen \\clip ya da "
                                "metin kutusu) izlenir; referans = satırın ilk karesi; sahne kesmesinde durur")
        self.b_batch.clicked.connect(self.start_batch)
        self.c_batch_occ = QCheckBox("toplu izlemede örtmeyi de bul")
        self.c_batch_occ.setChecked(bool(cfg.get("batch_occ", False)))
        self.b_render_qc = QPushButton("Yakarak denetle (libass)")
        self.b_render_qc.setToolTip("Seçili satırları libass ile yakar; \\clip/\\iclip sonrası satırın gerçekte ne "
                                    "kadarı göründüğünü ölçer ve eşiğin altındakileri seçer")
        self.b_render_qc.clicked.connect(self.start_render_qc)
        self.qc_pct = QSpinBox()
        self.qc_pct.setRange(1, 99)
        self.qc_pct.setValue(int(cfg.get("qc_pct", 15)))
        self.qc_pct.setSuffix(" %")
        self.qc_pct.setToolTip("Görünürlüğü bunun altındaki satırlar seçilir")
        self.warn_lbl = QLabel("")
        self.warn_lbl.setWordWrap(True)
        self.warn_lbl.setStyleSheet("color:#e0a030")
        checks = QHBoxLayout()
        for cb in (self.c_pos, self.c_scale, self.c_rot, self.c_border, self.c_clip):
            checks.addWidget(cb)
        checks.addStretch(1)
        form = QFormLayout()
        form.addRow("Kip", self.mode)
        form.addRow("Satır hangi karede doğru", self.refmode)
        form.addRow("Yumuşatma", self.smooth)
        rh = QHBoxLayout()
        rh.addWidget(self.c_occ)
        rh.addWidget(self.c_hide)
        rh.addWidget(self.hide_pct)
        rh.addStretch(1)
        rb = QHBoxLayout()
        rb.addWidget(self.b_batch, 1)
        rb.addWidget(self.c_batch_occ)
        l3 = QVBoxLayout(g3)
        l3.addLayout(checks)
        l3.addLayout(rh)
        l3.addLayout(form)
        l3.addWidget(self.b_apply)
        l3.addLayout(rb)
        rq = QHBoxLayout()
        rq.addWidget(self.b_render_qc, 1)
        rq.addWidget(QLabel("eşik"))
        rq.addWidget(self.qc_pct)
        l3.addLayout(rq)
        l3.addWidget(self.warn_lbl)

        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addWidget(g1)
        lay.addWidget(g2, 1)
        lay.addWidget(g_occ)
        lay.addWidget(g3)

        self._timer = QTimer(self)
        self._timer.setInterval(50)
        self._timer.timeout.connect(self._poll)
        media.position_changed.connect(self.plot.set_current)
        media.index_ready.connect(self._update_enabled)
        media.video_closed.connect(self._update_enabled)
        self._update_enabled()

    # ------------------------------------------------------------ durum
    def _update_enabled(self, *_):
        ok = self.media.index is not None
        self.b_track.setEnabled(ok and not self.busy)
        self.b_occ.setEnabled(ok and not self.busy and self.track is not None)
        self.b_batch.setEnabled(ok and not self.busy)
        self.b_render_qc.setEnabled(not self.busy)
        self.b_undo_track.setEnabled(bool(self.track_undo) and not self.busy)

    def set_roi(self, roi):
        self.roi = tuple(float(v) for v in roi) if roi else None
        if self.roi:
            x, y, w, h = self.roi
            self.roi_lbl.setText(f"Alan: {x:.0f},{y:.0f}  {w:.0f}×{h:.0f} (video px)")
        else:
            self.roi_lbl.setText("Alan: —")
        self.video.canvas.update()

    def set_track(self, tr, keep_undo: bool = False):
        if not keep_undo:
            self.track_undo = []
        self.track = tr
        if tr is not None and tr.roi and not self.roi:
            self.set_roi(tr.roi)
        self.summary.setText(tr.summary() + ("\n" + "; ".join(tr.notes) if tr.notes else "") if tr else "Henüz iz yok")
        self.plot.set_track(tr, self.roi or (tr.roi if tr else None))
        self.plot.set_current(self.media.frame)
        occ = None
        if tr is not None and tr.occlusion:
            from ..media.occlude import OcclusionResult
            try:
                occ = OcclusionResult.from_json(tr.occlusion)
            except Exception:  # noqa: BLE001
                occ = None
        self.set_occlusion(occ)
        self._update_enabled()
        self.video.canvas.update()

    def set_occlusion(self, occ):
        self.occlusion = occ
        if self.track is not None:
            self.track.occlusion = occ.to_json() if occ is not None else None
        self.plot.set_visibility({k: f.visibility for k, f in occ.frames.items()} if occ else None)
        self.occ_lbl.setText(occ.summary() if occ else "—")
        self.video.canvas.update()

    def _video_size(self):
        idx = self.media.index
        if idx is not None and idx.width:
            return idx.width, idx.height
        return self.ctx.doc.play_res()

    # ------------------------------------------------------------ 1. aralık/alan
    def range_from_lines(self):
        fm = self.ctx.frame_map
        rows = self.ctx.selected_rows()
        if fm is None or not rows:
            return False
        evs = self.ctx.doc.events
        spans = [M.line_frames(evs[r], fm) for r in rows]
        a, b = min(s[0] for s in spans), max(s[1] for s in spans)
        n = self.media.frame_count
        if n:
            b = min(b, n - 1)
            a = min(a, b)
        self.sp_start.setValue(a)
        self.sp_end.setValue(b)
        cur = self.media.frame
        self.sp_ref.setValue(cur if a <= cur <= b else a)
        return True

    def draw_roi(self):
        if self.win is not None:
            self.win.dock_video.show()
        self.video.set_tool("track")

    def roi_from_clip(self):
        ev = self.ctx.active_event()
        c = T.get_clip(ev.text) if ev is not None else None
        if not c or c[0] != "rect":
            self.ctx.status.emit("Etkin satırda dikdörtgen \\clip yok")
            return False
        x1, y1, x2, y2 = c[2]
        prx, pry = self.ctx.doc.play_res()
        vw, vh = self._video_size()
        kx, ky = vw / prx, vh / pry
        self.set_roi((x1 * kx, y1 * ky, (x2 - x1) * kx, (y2 - y1) * ky))
        return True

    # ------------------------------------------------------------ izleme
    def start_tracking(self):
        if self.busy:
            return False
        from ..media import tracker
        from ..media.frames import FrameGrabber
        ok, why = tracker.available()
        if not ok:
            self._fail(why)
            return False
        if self.media.index is None or not self.media.video_path:
            self._fail("Önce video açın")
            return False
        if not self.roi:
            self._fail("Önce izlenecek alanı seçin (Videoda çiz)")
            return False
        a, b, r = self.sp_start.value(), self.sp_end.value(), self.sp_ref.value()
        n = self.media.frame_count
        b = min(b, n - 1)
        if not (a <= r <= b):
            self._fail("Referans kare başlangıç ile bitiş arasında olmalı")
            return False
        method, model, _ = METHODS[self.method.currentIndex()]
        self._save_cfg()
        grab = FrameGrabber(self.media.video_path, self.media.index)
        fm = self.ctx.frame_map
        fps = getattr(fm, "fps", None) or getattr(self.media.index, "fps", None)
        vw, vh = self._video_size()
        roi = self.roi
        video_name = os.path.basename(self.media.video_path)

        def work():
            try:
                return tracker.track(
                    grab.get, a, b, r, roi, method=method, model=model, width=vw, height=vh,
                    fps=float(fps) if fps else None, video=video_name,
                    progress=lambda p, k: setattr(self, "_prog", (p, f"İzleniyor… kare {k}")),
                    cancel=lambda: self._cancel)
            finally:
                grab.close()

        def done(tr):
            self.set_track(tr)
            self.ctx.status.emit("Hareket izi: " + tr.summary())

        return self._run_bg(work, done, self.summary)

    def _run_bg(self, work, done, label):
        """work() arka planda; bitince done(sonuç) ana iş parçacığında. İptal/hata label'a yazılır."""
        from ..media.tracker import Cancelled
        self.busy = True
        self.error = None
        self._result = None
        self._cancel = False
        self._prog = (0.0, "")
        self._done = done
        self._label = label
        self.b_cancel.setEnabled(True)
        self._update_enabled()

        def runner():
            try:
                self._result = work()
            except Cancelled:
                self.error = "İptal edildi"
            except Exception as e:  # noqa: BLE001
                self.error = str(e)

        self._thread = threading.Thread(target=runner, name="osisub-motion", daemon=True)
        self._thread.start()
        self._timer.start()
        return True

    def _poll(self):
        p, msg = self._prog
        self.bar.setValue(int(p * 1000))
        if self._thread.is_alive():
            if msg:
                self._label.setText(msg)
            return
        self._timer.stop()
        self.busy = False
        self.b_cancel.setEnabled(False)
        self._update_enabled()
        if self.error:
            self._fail(self.error, self._label)
            return
        self._done(self._result)

    # ------------------------------------------------------------ örtme
    def occlusion_options(self):
        from ..media.occlude import OcclusionOptions
        return OcclusionOptions(threshold=float(self.occ_thr.value()), grow=self.occ_grow.value(),
                                model="median" if self.occ_model.currentIndex() == 0 else "ref")

    # ------------------------------------------------------------ tohum (renk modeli)
    def draw_seeds(self):
        if self.win is not None:
            self.win.dock_video.show()
        self.occ_method.setCurrentIndex(1)
        self.video.set_tool("seed")

    def clear_seeds(self):
        self.seed_pos = self.seed_neg = None
        self.seed_frame = None
        self.video.canvas.update()

    def paint_seed(self, x0, y0, x1, y1, positive: bool, radius: int = 5):
        """Tuval çağırır: video pikselinde fırça darbesi. Başka karede çizilirse eski tohumlar silinir."""
        import cv2
        import numpy as np
        k = self.media.frame
        vw, vh = self._video_size()
        if self.seed_frame is not None and self.seed_frame != k:
            self.clear_seeds()
            self.ctx.status.emit("Tohumlar yeni karede baştan çiziliyor (tohumlar tek kareye aittir)")
        if self.seed_pos is None:
            self.seed_pos = np.zeros((vh, vw), np.uint8)
            self.seed_neg = np.zeros((vh, vw), np.uint8)
            self.seed_frame = k
        target = self.seed_pos if positive else self.seed_neg
        cv2.line(target, (int(round(x0)), int(round(y0))), (int(round(x1)), int(round(y1))), 1, 2 * radius + 1)
        other = self.seed_neg if positive else self.seed_pos
        other[target > 0] = 0                       # aynı piksel iki tohumda olmasın: son çizilen kazanır

    def _start_colormask(self):
        from ..media import colormask
        from ..media.frames import FrameGrabber
        if self.seed_pos is None or not self.seed_pos.any() or not self.seed_neg.any():
            self._fail("Önce tohum çiz: nesnenin üstüne sol, çevresine sağ tıkla sürükle", self.occ_lbl)
            return False
        a, b = self.sp_start.value(), self.sp_end.value()
        if b <= a:
            self.range_from_lines()
            a, b = self.sp_start.value(), self.sp_end.value()
        b = min(b, self.media.frame_count - 1)
        k = self.seed_frame
        if not (a <= k <= b):
            self._fail(f"Tohum karesi ({k}) aralığın ({a}–{b}) dışında", self.occ_lbl)
            return False
        self._save_cfg()
        grab = FrameGrabber(self.media.video_path, self.media.index)
        pos, neg, crop = self.seed_pos.copy(), self.seed_neg.copy(), self.roi
        vw, vh = self._video_size()
        opts = colormask.ColorMaskOptions(grow=self.occ_grow.value(), scale=0.5 if max(vw, vh) > 1400 else 1.0)

        def work():
            try:
                res, _model = colormask.track_masks(
                    grab.get, a, b, k, pos, neg, crop, opts,
                    progress=lambda p, f: setattr(self, "_prog", (p, f"Maske çıkarılıyor… kare {f}")),
                    cancel=lambda: self._cancel)
                return res
            finally:
                grab.close()

        def done(res):
            self.set_occlusion(res)
            self.ctx.status.emit(res.summary())

        return self._run_bg(work, done, self.occ_lbl)

    def start_occlusion(self):
        if self.busy:
            return False
        if self.media.index is None or not self.media.video_path:
            self._fail("Önce video açın", self.occ_lbl)
            return False
        if self.occ_method.currentIndex() == 1:
            return self._start_colormask()
        if self.track is None or not (self.roi or self.track.roi):
            self._fail("Önce izleyin", self.occ_lbl)
            return False
        if self.media.index is None or not self.media.video_path:
            self._fail("Önce video açın", self.occ_lbl)
            return False
        from ..media import occlude
        from ..media.frames import FrameGrabber
        self._save_cfg()
        grab = FrameGrabber(self.media.video_path, self.media.index)
        tr, roi, opts = self.track, self.roi or self.track.roi, self.occlusion_options()

        def work():
            try:
                return occlude.detect(grab.get, tr, roi, opts,
                                      progress=lambda p, k: setattr(self, "_prog", (p, f"Örtme aranıyor… kare {k}")),
                                      cancel=lambda: self._cancel)
            finally:
                grab.close()

        def done(res):
            self.set_occlusion(res)
            self.ctx.status.emit(res.summary())

        return self._run_bg(work, done, self.occ_lbl)

    # ------------------------------------------------------------ toplu
    def start_batch(self):
        if self.busy:
            return False
        from ..media import batch, occlude, tracker
        from ..media.frames import FrameGrabber
        ok, why = tracker.available()
        fm = self.ctx.frame_map
        if not ok or self.media.index is None or fm is None:
            self._fail(why or "Önce video açın")
            return False
        rows = [r for r in self.ctx.selected_rows() if not self.ctx.doc.events[r].comment]
        if not rows:
            self._fail("Toplu izleme için satır seçin")
            return False
        self._save_cfg()
        vs = self._video_size()
        jobs = batch.plan(self.ctx.doc, rows, fm, self.media.frame_count, vs, scenes=self.media.scenes)
        method, model, _ = METHODS[self.method.currentIndex()]
        occ_opts = self.occlusion_options() if self.c_batch_occ.isChecked() else None
        grab = FrameGrabber(self.media.video_path, self.media.index)
        opts = self.options(with_occlusion=False)
        snap = {j.row: self.ctx.doc.events[j.row] for j in jobs}     # izleme sırasında belge değişirse fark et
        n_jobs = len(jobs)

        def work():
            try:
                return batch.run(jobs, grab.get, method=method, model=model, occlusion_opts=occ_opts,
                                 width=vs[0], height=vs[1],
                                 progress=lambda p, row, k: setattr(
                                     self, "_prog", (p, f"Toplu izleme: satır {row + 1} · kare {k} ({n_jobs} iş)")),
                                 cancel=lambda: self._cancel)
            finally:
                grab.close()

        def done(res_jobs):
            self.batch_jobs = res_jobs
            evs = self.ctx.doc.events
            if any(r >= len(evs) or evs[r] is not e for r, e in snap.items()):
                self._fail("İzleme sırasında satırlar değişti; toplu sonuç uygulanmadı (yeniden çalıştırın)")
                return
            events, new_rows, warns = batch.apply_jobs(self.ctx.doc, res_jobs, fm, vs, opts)
            ok_n = sum(1 for j in res_jobs if j.error is None)
            self._show_warnings(warns)
            if new_rows:
                self.ctx.replace_all_events(events, "Toplu hareket takibi", selection=set(new_rows),
                                            active=new_rows[0])
            last = next((j for j in reversed(res_jobs) if j.track is not None), None)
            if last is not None:        # son izi bölmede göster (incelemek / kaydetmek için)
                self.set_roi(last.roi)
                last.track.occlusion = last.occlusion.to_json() if last.occlusion else None
                self.set_track(last.track)
            self.summary.setText(f"Toplu izleme: {ok_n}/{len(res_jobs)} satır izlendi → {len(new_rows)} satır")
            self.ctx.status.emit(self.summary.text())
        return self._run_bg(work, done, self.summary)

    def wait(self, timeout: float = 60.0) -> bool:
        """Testler için: izleme bitene kadar olay döngüsünü döndür."""
        import time
        t = time.time()
        while self.busy and time.time() - t < timeout:
            QApplication.processEvents()
            time.sleep(0.01)
        return not self.busy

    def _fail(self, msg, label=None):
        self.error = msg
        (label or self.summary).setText(f"⚠ {msg}")
        self.ctx.status.emit(msg)

    def _show_warnings(self, warns):
        self.last_warnings = warns
        self.warn_lbl.setText("\n".join(warns[:6]) + (f"\n… (+{len(warns) - 6})" if len(warns) > 6 else ""))

    # ------------------------------------------------------------ dosya / pano
    def _default_path(self):
        d = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else os.path.expanduser("~")
        base = os.path.splitext(os.path.basename(self.ctx.doc.path or "iz"))[0]
        if self.track:
            base += f"_{self.track.start_frame}-{self.track.end_frame}"
        return os.path.join(d, base + ".assmotiontrack")

    def save_dialog(self):
        if self.track is None:
            return
        p, _ = QFileDialog.getSaveFileName(self, "İzi kaydet", self._default_path(),
                                           "OsiSub hareket izi (*.assmotiontrack);;AE Keyframe Data (*.txt)")
        if p:
            self.save(p)

    def save(self, path: str):
        if self.occlusion is not None:
            self.track.occlusion = self.occlusion.to_json()
        if path.lower().endswith(".txt"):
            with open(path, "w", encoding="utf-8", newline="") as fh:
                fh.write(self.track.to_ae())
        else:
            self.track.save(path)
        self.ctx.status.emit(f"İz kaydedildi: {os.path.basename(path)}")

    def open_dialog(self):
        start = os.path.dirname(self._default_path())
        p, _ = QFileDialog.getOpenFileName(self, "İz aç", start,
                                           "Hareket izi (*.assmotiontrack *.txt);;Tümü (*)")
        if p:
            self.load(p)

    def load(self, path: str):
        try:
            tr = M.MotionTrack.load(path)
        except Exception as e:  # noqa: BLE001
            self._fail(f"İz okunamadı: {e}")
            return None
        if tr.method == "ae":
            tr = self._place_ae(tr)
        self.set_track(tr)
        return tr

    def _place_ae(self, tr):
        """AE verisini seçili satırın ilk karesine yerleştir."""
        fm = self.ctx.frame_map
        rows = self.ctx.selected_rows()
        start = 0
        if fm is not None and rows:
            start = min(M.line_frames(self.ctx.doc.events[r], fm)[0] for r in rows)
            span = max(M.line_frames(self.ctx.doc.events[r], fm)[1] for r in rows) - start + 1
            if span != len(tr):
                tr.notes.append(f"veri {len(tr)} kare, seçili satırlar {span} kare")
        shift = start - tr.start_frame
        for f in tr.frames:
            f.frame += shift
        tr.ref_frame += shift
        if not tr.width:
            tr.width, tr.height = self._video_size()
        return tr

    def paste_ae(self):
        text = QApplication.clipboard().text()
        return self.load_ae_text(text)

    def load_ae_text(self, text: str):
        try:
            tr = M.from_ae(text)
        except Exception as e:  # noqa: BLE001
            self._fail(f"AE verisi okunamadı: {e}")
            return None
        tr = self._place_ae(tr)
        self.set_track(tr)
        return tr

    def copy_ae(self):
        if self.track is not None:
            QApplication.clipboard().setText(self.track.to_ae())
            self.ctx.status.emit("AE Keyframe Data panoya kopyalandı")

    # ------------------------------------------------------------ uygula
    def options(self, with_occlusion: bool = True) -> M.ApplyOptions:
        occ = self.occlusion if (with_occlusion and self.c_occ.isChecked()) else None
        return M.ApplyOptions(position=self.c_pos.isChecked(), scale=self.c_scale.isChecked(),
                              rotation=self.c_rot.isChecked(), border=self.c_border.isChecked(),
                              clip=self.c_clip.isChecked(),
                              mode="compact" if self.mode.currentIndex() == 1 else "frame",
                              occlusion=occ,
                              hide_below=self.hide_pct.value() / 100.0 if self.c_hide.isChecked() else 0.0)

    def _save_cfg(self):
        self.ctx.config["motion"] = {
            "method": self.method.currentIndex(), "position": self.c_pos.isChecked(),
            "scale": self.c_scale.isChecked(), "rotation": self.c_rot.isChecked(),
            "border": self.c_border.isChecked(), "clip": self.c_clip.isChecked(),
            "mode": "compact" if self.mode.currentIndex() == 1 else "frame",
            "smooth": self.smooth.value(), "refmode": self.refmode.currentIndex(),
            "occ_threshold": self.occ_thr.value(), "occ_grow": self.occ_grow.value(),
            "occ_model": self.occ_model.currentIndex(), "use_occlusion": self.c_occ.isChecked(),
            "occ_method": self.occ_method.currentIndex(),
            "hide": self.c_hide.isChecked(), "hide_pct": self.hide_pct.value(),
            "batch_occ": self.c_batch_occ.isChecked(), "qc_pct": self.qc_pct.value()}
        self.ctx.config.save()

    def apply(self):
        if self.track is None and self.occlusion is not None and self.occlusion.frames:
            # iz yok ama maske var (duran tabela + önünden geçen nesne): kimlik iziyle yalnız \iclip
            ks = sorted(self.occlusion.frames)
            vw, vh = self._video_size()
            ident = M.MotionTrack([M.TrackFrame(k) for k in range(ks[0], ks[-1] + 1)], ks[0], vw, vh,
                                  method="kimlik (yalnız maske)")
            ident.occlusion = self.occlusion.to_json()
            self.track = ident
        if self.track is None:
            self._fail("Önce izleyin ya da bir iz açın")
            return None
        fm = self.ctx.frame_map
        if fm is None:
            self._fail("Kare eşlemesi yok (video açın)")
            return None
        rows = [r for r in self.ctx.selected_rows() if not self.ctx.doc.events[r].comment]
        if not rows:
            self._fail("Uygulanacak satır seçili değil")
            return None
        self._save_cfg()
        tr = self.track.smoothed(self.smooth.value()) if self.smooth.value() else self.track
        opts = self.options()
        if self.refmode.currentIndex() == 1:
            # her satır kendi ilk karesine göre
            out_events = list(self.ctx.doc.events)
            new_rows_total = []
            warns = []
            for r in sorted(rows, reverse=True):
                ev = out_events[r]
                f0 = M.line_frames(ev, fm)[0]
                o = M.ApplyOptions(**{**opts.__dict__, "ref_frame": f0})
                try:
                    new, w = M.apply_track(ev, self.ctx.doc.style_by_name(ev.style), tr, fm,
                                           self.ctx.doc.play_res(), self._video_size(), o)
                except M.MotionError as e:
                    warns.append(f"Satır {r + 1}: {e}")
                    continue
                warns += [f"Satır {r + 1}: {x}" for x in w]
                out_events[r:r + 1] = new
                new_rows_total = [x + len(new) - 1 for x in new_rows_total] + list(range(r, r + len(new)))
            events, new_rows = out_events, sorted(new_rows_total)
        else:
            events, new_rows, warns = M.apply_to_lines(self.ctx.doc, rows, tr, fm, self._video_size(), opts)
        self._show_warnings(warns)
        if not new_rows:
            self.ctx.status.emit("Hiçbir satıra uygulanamadı")
            return None
        self.ctx.replace_all_events(events, "Hareket takibi uygula", selection=set(new_rows), active=new_rows[0])
        self.ctx.status.emit(f"Hareket izi uygulandı: {len(rows)} satır → {len(new_rows)} satır")
        return new_rows

    # ------------------------------------------------------------ elle düzeltme (tuval)
    def begin_track_edit(self):
        """Tuval sürüklemeye başlamadan önce çağırır: geri al için kopya."""
        if self.track is not None:
            self.track_undo.append(self.track._copy([M.TrackFrame(f.frame, f.c, f.t, f.conf, f.ok)
                                                     for f in self.track.frames]))
            self.track_undo = self.track_undo[-20:]

    def shift_track(self, frame: int, delta: complex, following: bool, base=None):
        """Karenin (ve istenirse referanstan uzaktaki tüm karelerin) konumunu delta kadar kaydır.
        `base`: sürükleme başındaki iz (her harekette ondan yeniden hesaplanır)."""
        tr = base or self.track
        if tr is None or not tr.covers(frame):
            return
        out = []
        for f in tr.frames:
            hit = f.frame == frame or (following and (
                (frame > tr.ref_frame and f.frame >= frame) or (frame < tr.ref_frame and f.frame <= frame)))
            if hit and f.frame != tr.ref_frame:
                out.append(M.TrackFrame(f.frame, f.c, f.t + delta, 1.0, True))
            else:
                out.append(M.TrackFrame(f.frame, f.c, f.t, f.conf, f.ok))
        new = tr._copy(out)
        if "elle düzeltildi" not in new.notes:
            new.notes.append("elle düzeltildi")
        self.track = new
        self.plot.set_track(new, self.roi or new.roi)
        self.summary.setText(new.summary() + "\n" + "; ".join(new.notes))
        self._update_enabled()

    def undo_track(self):
        if not self.track_undo:
            return
        tr = self.track_undo.pop()
        self.set_track(tr, keep_undo=True)

    # ------------------------------------------------------------ libass denetimi
    def start_render_qc(self):
        if self.busy:
            return False
        rows = [r for r in self.ctx.selected_rows() if not self.ctx.doc.events[r].comment]
        if not rows:
            self._fail("Denetlenecek satırları seçin", self.warn_lbl)
            return False
        from ..media import render_qc
        self._save_cfg()
        doc = self.ctx.doc.clone()
        vs = self._video_size()
        fontsdir = None
        if self.ctx.doc.path:
            fd = os.path.join(os.path.dirname(self.ctx.doc.path), "fonts")
            fontsdir = fd if os.path.isdir(fd) else None
        thr = self.qc_pct.value() / 100.0
        snap = {r: self.ctx.doc.events[r] for r in rows}

        def work():
            return render_qc.measure(doc, rows, vs, fontsdir=fontsdir,
                                     progress=lambda p, r: setattr(self, "_prog", (p, f"Yakılıyor… satır {r + 1}")),
                                     cancel=lambda: self._cancel)

        def done(res):
            self.qc_result = res
            low = [v for v in res if v.ratio < thr]
            empty = [v for v in res if v.full <= 0]
            evs = self.ctx.doc.events
            if any(r >= len(evs) or evs[r] is not e for r, e in snap.items()):
                self._fail("Denetim sırasında satırlar değişti; sonuç seçilmedi", self.warn_lbl)
                return
            msg = (f"libass denetimi: {len(res)} satır; {len(low)} satırın görünürlüğü %{thr * 100:.0f} altında"
                   + (f" ({len(empty)} satır hiç çizilmiyor)" if empty else ""))
            if low:
                rows_low = [v.row for v in low]
                self.ctx.set_selection(set(rows_low), rows_low[0])
                msg += " — seçildi (silmek için Ctrl+Delete, gizlemek için Ctrl+K)"
            self.warn_lbl.setText(msg)
            self.ctx.status.emit(msg)

        return self._run_bg(work, done, self.warn_lbl)
