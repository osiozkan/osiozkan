"""Zamanlama son işlemcisi penceresi. Ayarlar config.json'da hatırlanır."""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox,
                               QFormLayout, QGridLayout, QGroupBox, QHBoxLayout, QLabel,
                               QListWidget, QListWidgetItem, QPushButton, QSlider, QSpinBox,
                               QVBoxLayout)

from ..core.timing import TimingOptions, postprocess


class TimingDialog(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Zamanlama son işlemcisi")
        saved = dict(ctx.config["timing_pp"] or {})

        def spin(lo, hi, val, suffix):
            s = QSpinBox()
            s.setRange(lo, hi)
            s.setValue(int(val))
            s.setSuffix(suffix)
            return s

        # --- payı
        self.g_lead = QGroupBox("Giriş / çıkış payı")
        self.g_lead.setCheckable(True)
        self.g_lead.setChecked(bool(saved.get("lead", False)))
        self.lead_in = spin(0, 10000, saved.get("lead_in", 0), " ms")
        self.lead_out = spin(0, 10000, saved.get("lead_out", 0), " ms")
        f = QFormLayout(self.g_lead)
        f.addRow("Giriş (başlangıç öne)", self.lead_in)
        f.addRow("Çıkış (bitiş arkaya)", self.lead_out)

        # --- bitişik
        self.g_adj = QGroupBox("Bitişik satırları birleştir")
        self.g_adj.setCheckable(True)
        self.g_adj.setChecked(bool(saved.get("adjacent", False)))
        self.adj_thr = spin(0, 10000, saved.get("adjacent_threshold", 300), " ms")
        self.adj_bias = QSlider(Qt.Horizontal)
        self.adj_bias.setRange(0, 100)
        self.adj_bias.setValue(int(saved.get("adjacent_bias", 0.5) * 100))
        bh = QHBoxLayout()
        bh.addWidget(QLabel("Öncekinin bitişi"))
        bh.addWidget(self.adj_bias, 1)
        bh.addWidget(QLabel("Sonrakinin başı"))
        f = QFormLayout(self.g_adj)
        f.addRow("En fazla boşluk/çakışma", self.adj_thr)
        f.addRow("Birleşme noktası", bh)

        # --- anahtar kare
        self.g_kf = QGroupBox("Anahtar karelere yasla")
        self.g_kf.setCheckable(True)
        has_kf = ctx.frame_map is not None and (bool(ctx.frame_map.keyframes) or bool(
            getattr(getattr(ctx, "media", None), "scenes", None)))
        self.g_kf.setChecked(bool(saved.get("keyframes", False)) and has_kf)
        self.g_kf.setEnabled(has_kf)
        if not has_kf:
            self.g_kf.setToolTip("Önce video açın (anahtar kareler videodan okunur)")
        self.sb = spin(0, 500, saved.get("start_before", 5), " kare")
        self.sa = spin(0, 500, saved.get("start_after", 4), " kare")
        self.eb = spin(0, 500, saved.get("end_before", 5), " kare")
        self.ea = spin(0, 500, saved.get("end_after", 11), " kare")
        scenes = getattr(getattr(ctx, "media", None), "scenes", None) or []
        self.use_scenes = QCheckBox(f"Sahne değişimlerini de yaslama noktası say ({len(scenes)})")
        self.use_scenes.setEnabled(bool(scenes))
        self.use_scenes.setChecked(bool(scenes) and bool(saved.get("use_scenes", True)))
        if not scenes:
            self.use_scenes.setToolTip("Video > Sahne değişimlerini bul")
        g = QGridLayout(self.g_kf)
        g.addWidget(QLabel("Başlangıç: anahtar kare önce ise en çok"), 0, 0)
        g.addWidget(self.sb, 0, 1)
        g.addWidget(QLabel("Başlangıç: anahtar kare sonra ise en çok"), 1, 0)
        g.addWidget(self.sa, 1, 1)
        g.addWidget(QLabel("Bitiş: anahtar kare önce ise en çok"), 2, 0)
        g.addWidget(self.eb, 2, 1)
        g.addWidget(QLabel("Bitiş: anahtar kare sonra ise en çok"), 3, 0)
        g.addWidget(self.ea, 3, 1)
        g.addWidget(self.use_scenes, 4, 0, 1, 2)

        # --- konuşmaya yasla (VAD)
        speech = getattr(getattr(ctx, "media", None), "speech", None) or []
        self.g_speech = QGroupBox(f"Konuşmaya yasla (önce uygulanır; {len(speech)} konuşma bölümü)")
        self.g_speech.setCheckable(True)
        self.g_speech.setChecked(bool(saved.get("speech", False)) and bool(speech))
        self.g_speech.setEnabled(bool(speech))
        if not speech:
            self.g_speech.setToolTip("Ses yüklenince konuşma bölümleri kendiliğinden bulunur")
        self.sp_start = spin(0, 3000, saved.get("speech_start", 400), " ms")
        self.sp_end = spin(0, 3000, saved.get("speech_end", 400), " ms")
        f = QFormLayout(self.g_speech)
        f.addRow("Başlangıç en çok kayabilir", self.sp_start)
        f.addRow("Bitiş en çok kayabilir", self.sp_end)

        # --- kapsam
        self.scope = QComboBox()
        self.scope.addItems(["Seçili satırlar", "Tüm satırlar"])
        self.scope.setCurrentIndex(0 if len(ctx.selected_rows()) > 1 else 1)
        self.styles = QListWidget()
        self.styles.setMaximumHeight(120)
        used = {e.style for e in ctx.doc.events}
        for st in ctx.doc.styles:
            it = QListWidgetItem(st.name)
            it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
            on = st.name in used and st.name in set(saved.get("styles", [st.name]))
            it.setCheckState(Qt.Checked if on else Qt.Unchecked)
            self.styles.addItem(it)
        self.result = QLabel("")
        b_prev = QPushButton("Önizle")
        b_prev.clicked.connect(self.preview)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.button(QDialogButtonBox.Ok).setText("Uygula")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        for w in (self.g_speech, self.g_lead, self.g_adj, self.g_kf):
            lay.addWidget(w)
        fs = QFormLayout()
        fs.addRow("Satırlar", self.scope)
        fs.addRow("Stiller", self.styles)
        lay.addLayout(fs)
        h = QHBoxLayout()
        h.addWidget(b_prev)
        h.addWidget(self.result, 1)
        lay.addLayout(h)
        lay.addWidget(bb)

    def options(self) -> TimingOptions:
        styles = {self.styles.item(i).text() for i in range(self.styles.count())
                  if self.styles.item(i).checkState() == Qt.Checked}
        lead = self.g_lead.isChecked()
        return TimingOptions(
            lead_in=self.lead_in.value() if lead else 0, lead_out=self.lead_out.value() if lead else 0,
            adjacent=self.g_adj.isChecked(), adjacent_threshold=self.adj_thr.value(),
            adjacent_bias=self.adj_bias.value() / 100.0, keyframes=self.g_kf.isChecked(),
            start_before=self.sb.value(), start_after=self.sa.value(), end_before=self.eb.value(),
            end_after=self.ea.value(), styles=styles)

    def _rows(self):
        return self.ctx.selected_rows() if self.scope.currentIndex() == 0 else list(range(len(self.ctx.doc.events)))

    def _frame_map(self):
        fm = self.ctx.frame_map
        scenes = getattr(getattr(self.ctx, "media", None), "scenes", None) or []
        if fm is None or not (self.use_scenes.isChecked() and scenes):
            return fm
        import copy
        fm2 = copy.copy(fm)
        fm2.keyframes = sorted(set(fm.keyframes) | set(scenes))
        return fm2

    def run(self):
        evs = self.ctx.doc.events
        rows = self._rows()
        speech = getattr(getattr(self.ctx, "media", None), "speech", None) or []
        self.speech_count = 0
        if self.g_speech.isChecked() and speech:
            from ..media.vad import snap_to_speech
            opt = self.options()
            rows_f = [r for r in rows if not evs[r].comment and (opt.styles is None or evs[r].style in opt.styles)]
            sch = snap_to_speech(evs, rows_f, speech, self.sp_start.value(), self.sp_end.value())
            self.speech_count = len(sch)
            work = list(evs)
            for r, c in sch.items():
                work[r] = evs[r].replace(**c)
            res = postprocess(work, rows, opt, self._frame_map())
            merged = {}
            for r in set(sch) | set(res.changes):
                c = dict(sch.get(r, {}))
                c.update(res.changes.get(r, {}))
                s_, e_ = c.get("start", evs[r].start), c.get("end", evs[r].end)
                if (s_, e_) != (evs[r].start, evs[r].end):
                    merged[r] = {"start": s_, "end": e_}
            res.changes = merged
            return res
        return postprocess(evs, rows, self.options(), self._frame_map())

    def preview(self):
        r = self.run()
        self.result.setText(f"{len(r.changes)} satır değişecek · konuşma {getattr(self, 'speech_count', 0)} · "
                            f"pay {r.lead} · bitişik {r.adjacent} · anahtar kare {r.keyframe}")
        return r

    def apply(self):
        r = self.run()
        o = self.options()
        self.ctx.config["timing_pp"] = {
            "lead": self.g_lead.isChecked(), "lead_in": self.lead_in.value(), "lead_out": self.lead_out.value(),
            "adjacent": o.adjacent, "adjacent_threshold": o.adjacent_threshold, "adjacent_bias": o.adjacent_bias,
            "keyframes": o.keyframes, "start_before": o.start_before, "start_after": o.start_after,
            "end_before": o.end_before, "end_after": o.end_after, "styles": sorted(o.styles or []),
            "use_scenes": self.use_scenes.isChecked(), "speech": self.g_speech.isChecked(),
            "speech_start": self.sp_start.value(), "speech_end": self.sp_end.value()}
        self.ctx.config.save()
        if r.changes:
            self.ctx.update_events(r.changes, "Zamanlama son işlemcisi")
        self.ctx.status.emit(f"Zamanlama son işlemcisi: {len(r.changes)} satır")
        self.last_result = r
        self.accept()
