"""Videoyla dışa aktarma pencereleri: gömülü altyazıyı aç/çıkar, videoya yak (hardsub), MKV'ye göm (softsub).

Yakma ve gömme açık belgenin ŞU ANKİ halini kullanır (kaydedilmemiş değişiklikler dahil): belge geçici
bir .ass'a yazılır; isteğe bağlı olarak kullanılan yazı tipleri toplanıp (Yazı tipi toplayıcı) yakmada
fontsdir'e kopyalanır, gömmede MKV'ye ek olarak konur.
"""

from __future__ import annotations

import os
import shutil
import tempfile

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QDialogButtonBox, QFileDialog,
                               QFormLayout, QHBoxLayout, QLabel, QLineEdit, QListWidget, QListWidgetItem,
                               QMessageBox, QProgressBar, QPushButton, QSpinBox, QVBoxLayout)

from ..media import export as X


def collect_font_paths(ctx) -> tuple[list[str], list[str]]:
    """Belgede kullanılan yazı tiplerinin dosyaları + bulunamayanların adları."""
    from ..core import fonts as F
    from ..media.cache import cache_root
    deep, shallow = F.doc_font_dirs(ctx.doc)
    idx = F.FontIndex(F.system_font_dirs() + deep, os.path.join(cache_root(), "fonts.json"), shallow=shallow)
    reps = F.collect(ctx.doc, idx)
    paths = sorted({r.face.path for r in reps if r.face is not None})
    missing = sorted({r.use.name for r in reps if r.face is None})
    return paths, missing


def _write_temp_ass(ctx) -> tuple[str, str]:
    d = tempfile.mkdtemp(prefix="osisub-export-")
    p = os.path.join(d, "altyazi.ass")
    with open(p, "wb") as f:
        f.write(ctx.doc.to_bytes())
    return d, p


class _JobDialog(QDialog):
    """Ortak: video seçimi, çıktı yolu, ilerleme, başlat/iptal."""
    title = ""
    out_suffix = ""

    def __init__(self, win):
        super().__init__(win)
        self.win = win
        self.ctx = win.ctx
        self.setWindowTitle(self.title)
        self.resize(640, 0)
        self.video = QLineEdit(win.media.video_path or "")
        b_v = QPushButton("…")
        b_v.clicked.connect(self._pick_video)
        vh = QHBoxLayout()
        vh.addWidget(self.video, 1)
        vh.addWidget(b_v)
        self.out = QLineEdit(self._default_out())
        b_o = QPushButton("…")
        b_o.clicked.connect(self._pick_out)
        oh = QHBoxLayout()
        oh.addWidget(self.out, 1)
        oh.addWidget(b_o)
        self.form = QFormLayout()
        self.form.addRow("Video", vh)
        self.form.addRow("Çıktı", oh)
        self.build(self.form)
        self.fonts = QCheckBox("Kullanılan yazı tiplerini topla (Yazı tipi toplayıcı)")
        self.fonts.setChecked(True)
        self.form.addRow("", self.fonts)
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.info = QLabel("")
        self.info.setWordWrap(True)
        self.b_start = QPushButton("Başlat")
        self.b_start.clicked.connect(self.start)
        self.b_cancel = QPushButton("İptal")
        self.b_cancel.setEnabled(False)
        self.b_cancel.clicked.connect(self.cancel)
        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.rejected.connect(self.reject)
        bh = QHBoxLayout()
        bh.addWidget(self.b_start)
        bh.addWidget(self.b_cancel)
        bh.addStretch(1)
        bh.addWidget(bb)
        lay = QVBoxLayout(self)
        lay.addLayout(self.form)
        lay.addWidget(self.bar)
        lay.addWidget(self.info)
        lay.addLayout(bh)
        self.job = None
        self._tmp = None

    def build(self, form):
        pass

    def command(self, video, ass, out, fonts, fontsdir) -> tuple[list, float]:
        raise NotImplementedError

    def _default_out(self):
        v = self.win.media.video_path
        if not v:
            return ""
        return os.path.splitext(v)[0] + self.out_suffix

    def _pick_video(self):
        p, _ = QFileDialog.getOpenFileName(self, "Video", os.path.dirname(self.video.text()),
                                           "Video (*.mkv *.mp4 *.m2ts *.ts *.avi *.webm *.mov);;Tüm dosyalar (*)")
        if p:
            self.video.setText(p)
            if not self.out.text():
                self.out.setText(os.path.splitext(p)[0] + self.out_suffix)

    def _pick_out(self):
        ext = os.path.splitext(self.out_suffix)[1]
        p, _ = QFileDialog.getSaveFileName(self, "Çıktı", self.out.text(), f"Video (*{ext});;Tüm dosyalar (*)")
        if p:
            self.out.setText(p)

    def start(self):
        video, out = self.video.text().strip(), self.out.text().strip()
        if not video or not os.path.exists(video):
            self.info.setText("Video bulunamadı")
            return False
        if not out or os.path.abspath(out) == os.path.abspath(video):
            self.info.setText("Çıktı dosyası videonun kendisi olamaz")
            return False
        self._tmp, ass = _write_temp_ass(self.ctx)
        fonts, missing = ([], [])
        fontsdir = None
        if self.fonts.isChecked():
            QApplication.setOverrideCursor(Qt.WaitCursor)
            try:
                fonts, missing = collect_font_paths(self.ctx)
            finally:
                QApplication.restoreOverrideCursor()
            if fonts:
                fontsdir = os.path.join(self._tmp, "fonts")
                os.makedirs(fontsdir, exist_ok=True)
                for p in fonts:
                    shutil.copy2(p, os.path.join(fontsdir, os.path.basename(p)))
        cmd, dur = self.command(video, ass, out, fonts, fontsdir)
        self.job = X.Job(cmd, dur)
        self.job.start()
        self.b_start.setEnabled(False)
        self.b_cancel.setEnabled(True)
        msg = f"Çalışıyor… ({len(fonts)} yazı tipi)"
        if missing:
            msg += " · BULUNAMAYAN: " + ", ".join(missing)
        self.info.setText(msg)
        QTimer.singleShot(100, self._poll)
        return True

    def _poll(self):
        if self.job is None:
            return
        self.bar.setValue(int(self.job.progress * 1000))
        if self.job.running:
            QTimer.singleShot(150, self._poll)
            return
        self._finished()

    def _finished(self):
        job = self.job
        self.b_start.setEnabled(True)
        self.b_cancel.setEnabled(False)
        if job.returncode == 0:
            self.bar.setValue(1000)
            self.info.setText(f"Bitti: {self.out.text()}")
            self.ctx.status.emit(f"{self.title}: {os.path.basename(self.out.text())} hazır")
        else:
            self.info.setText(f"Hata (kod {job.returncode}): {job.error[-600:]}")
        if self._tmp:
            shutil.rmtree(self._tmp, ignore_errors=True)
            self._tmp = None

    def cancel(self):
        if self.job is not None:
            self.job.cancel()

    def wait(self, timeout=300.0) -> bool:
        """Testler için: iş bitene kadar bekle ve sonucu işle."""
        if self.job is None:
            return False
        ok = self.job.wait(timeout)
        self._finished()
        return ok and self.job.returncode == 0

    def reject(self):
        if self.job is not None and self.job.running:
            if not self.win.headless and QMessageBox.question(self, self.title, "İş sürüyor. Durdurulsun mu?") \
                    != QMessageBox.Yes:
                return
            self.job.cancel()
        super().reject()


class BurnDialog(_JobDialog):
    title = "Videoya yak (hardsub)"
    out_suffix = "_hardsub.mp4"

    def build(self, form):
        cfg = dict(self.ctx.config["burn"] or {})
        self.codec = QComboBox()
        self.codec.addItem("H.264 (x264) — her yerde oynar", "libx264")
        self.codec.addItem("H.265 (x265) — daha küçük dosya", "libx265")
        self.codec.setCurrentIndex(max(0, self.codec.findData(cfg.get("codec", "libx264"))))
        self.crf = QSpinBox()
        self.crf.setRange(0, 51)
        self.crf.setValue(int(cfg.get("crf", 18)))
        self.crf.setToolTip("Kalite: küçük = daha iyi ve daha büyük dosya (x264 için 16–20 iyi)")
        self.preset = QComboBox()
        for p in ("ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"):
            self.preset.addItem(p)
        self.preset.setCurrentText(cfg.get("preset", "medium"))
        self.tune = QCheckBox("Anime ayarı (tune=animation, yalnız x264)")
        self.tune.setChecked(cfg.get("tune", True))
        self.audio = QComboBox()
        self.audio.addItem("Sesi kopyala", "copy")
        self.audio.addItem("AAC'ye çevir (MP4 uyumu)", "aac")
        self.audio.addItem("Sessiz", "none")
        self.audio.setCurrentIndex(max(0, self.audio.findData(cfg.get("audio", "copy"))))
        self.range = QComboBox()
        self.range.addItem("Tüm video", "all")
        self.range.addItem("Seçili satırların aralığı (önizleme)", "sel")
        q = QHBoxLayout()
        q.addWidget(self.crf)
        q.addWidget(QLabel("hazır ayar"))
        q.addWidget(self.preset)
        form.addRow("Kodek", self.codec)
        form.addRow("CRF", q)
        form.addRow("", self.tune)
        form.addRow("Ses", self.audio)
        form.addRow("Aralık", self.range)

    def command(self, video, ass, out, fonts, fontsdir):
        o = X.BurnOptions(codec=self.codec.currentData(), crf=self.crf.value(), preset=self.preset.currentText(),
                          audio=self.audio.currentData(),
                          tune="animation" if (self.tune.isChecked() and self.codec.currentData() == "libx264") else "")
        dur = X.duration_s(video)
        if self.range.currentData() == "sel":
            evs = [self.ctx.doc.events[r] for r in self.ctx.selected_rows()]
            if evs:
                o.start_ms = max(0, min(e.start for e in evs) - 1000)
                o.end_ms = max(e.end for e in evs) + 1000
                dur = (o.end_ms - o.start_ms) / 1000.0
        self.ctx.config["burn"] = {"codec": o.codec, "crf": o.crf, "preset": o.preset, "audio": o.audio,
                                   "tune": self.tune.isChecked()}
        self.ctx.config.save()
        return X.burn_cmd(video, ass, out, o, fontsdir), dur


class MuxDialog(_JobDialog):
    title = "MKV'ye göm (softsub)"
    out_suffix = "_tr.mkv"

    def build(self, form):
        cfg = dict(self.ctx.config["mux"] or {})
        self.lang = QLineEdit(cfg.get("lang", "tur"))
        self.lang.setToolTip("ISO 639-2 dil kodu (tur, eng, jpn…)")
        self.track_title = QLineEdit(cfg.get("title", "Türkçe"))
        self.default = QCheckBox("Varsayılan altyazı izi yap")
        self.default.setChecked(cfg.get("default", True))
        self.keep_subs = QCheckBox("Videodaki mevcut altyazı izlerini koru")
        self.keep_subs.setChecked(cfg.get("keep_subs", False))
        self.keep_att = QCheckBox("Videodaki mevcut ekleri (yazı tiplerini) koru")
        self.keep_att.setChecked(cfg.get("keep_att", True))
        form.addRow("Dil", self.lang)
        form.addRow("İz adı", self.track_title)
        form.addRow("", self.default)
        form.addRow("", self.keep_subs)
        form.addRow("", self.keep_att)

    def command(self, video, ass, out, fonts, fontsdir):
        o = X.MuxOptions(language=self.lang.text().strip() or "und", title=self.track_title.text().strip(),
                         default=self.default.isChecked(), keep_subs=self.keep_subs.isChecked(),
                         keep_attachments=self.keep_att.isChecked())
        self.ctx.config["mux"] = {"lang": o.language, "title": o.title, "default": o.default,
                                  "keep_subs": o.keep_subs, "keep_att": o.keep_attachments}
        self.ctx.config.save()
        return X.mux_cmd_full(video, ass, out, o, fonts), X.duration_s(video)


class ExtractDialog(QDialog):
    """Videodaki (MKV/MP4) altyazı izlerini listeler: belge ya da referans olarak aç, dosyaya kaydet,
    ekleri (yazı tiplerini) çıkar. Resim tabanlı izler (PGS/VobSub) OCR gerektirir, açılamaz."""

    def __init__(self, win, video: str | None = None):
        super().__init__(win)
        self.win = win
        self.ctx = win.ctx
        self.setWindowTitle("Videodaki altyazılar")
        self.resize(620, 380)
        self.video = video or win.media.video_path or ""
        self.label = QLabel("")
        b_v = QPushButton("Başka video…")
        b_v.clicked.connect(self._pick)
        top = QHBoxLayout()
        top.addWidget(self.label, 1)
        top.addWidget(b_v)
        self.list = QListWidget()
        self.list.itemDoubleClicked.connect(lambda *_: self.open_track("doc"))
        b_doc = QPushButton("Belge olarak aç")
        b_doc.clicked.connect(lambda: self.open_track("doc"))
        b_ref = QPushButton("Referans olarak aç")
        b_ref.clicked.connect(lambda: self.open_track("ref"))
        b_save = QPushButton("Kaydet…")
        b_save.clicked.connect(lambda: self.save_track())
        b_att = QPushButton("Yazı tiplerini çıkar…")
        b_att.clicked.connect(lambda: self.extract_fonts())
        bh = QHBoxLayout()
        for b in (b_doc, b_ref, b_save, b_att):
            bh.addWidget(b)
        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.rejected.connect(self.reject)
        self.info = QLabel("")
        self.info.setWordWrap(True)
        lay = QVBoxLayout(self)
        lay.addLayout(top)
        lay.addWidget(self.list, 1)
        lay.addLayout(bh)
        lay.addWidget(self.info)
        lay.addWidget(bb)
        self.tracks: list[X.SubTrack] = []
        self.refresh()

    def _pick(self):
        p, _ = QFileDialog.getOpenFileName(self, "Video", os.path.dirname(self.video),
                                           "Video (*.mkv *.mp4 *.m2ts *.ts *.webm *.mov);;Tüm dosyalar (*)")
        if p:
            self.video = p
            self.refresh()

    def refresh(self):
        self.list.clear()
        self.tracks = []
        if not self.video:
            self.label.setText("Video yok — önce video açın ya da seçin")
            return
        self.label.setText(os.path.basename(self.video))
        try:
            self.tracks = X.subtitle_tracks(self.video)
        except RuntimeError as e:
            self.info.setText(str(e))
            return
        for t in self.tracks:
            it = QListWidgetItem(t.label())
            if not t.is_text:
                it.setFlags(it.flags() & ~Qt.ItemIsEnabled)
                it.setToolTip("Resim tabanlı altyazı: metne çevirmek için OCR gerekir (henüz yok)")
            self.list.addItem(it)
        first = next((i for i, t in enumerate(self.tracks) if t.is_text), None)
        if first is not None:
            self.list.setCurrentRow(first)
        if not self.tracks:
            self.info.setText("Bu videoda altyazı izi yok")

    def current(self) -> X.SubTrack | None:
        i = self.list.currentRow()
        if 0 <= i < len(self.tracks) and self.tracks[i].is_text:
            return self.tracks[i]
        return None

    def _extract_to(self, t: X.SubTrack, path: str) -> bool:
        rc, err = X.run_sync(X.extract_cmd(self.video, t, path))
        if rc != 0:
            self.info.setText(f"Çıkarılamadı: {err[-400:]}")
            return False
        return True

    def suggested_name(self, t: X.SubTrack) -> str:
        tag = t.language or f"iz{t.sub_index + 1}"
        return os.path.splitext(self.video)[0] + f".{tag}{t.ext}"

    def open_track(self, mode: str):
        t = self.current()
        if t is None:
            return False
        d = tempfile.mkdtemp(prefix="osisub-extract-")
        p = os.path.join(d, os.path.basename(self.suggested_name(t)))
        if not self._extract_to(t, p):
            return False
        if mode == "ref":
            self.win.open_reference(p)
        else:
            if not self.win.maybe_save():
                return False
            self.ctx.open_document(p, remember=False)
            doc = self.ctx.doc
            doc.path = None                          # geçici dosyaya kaydedilmesin: Farklı kaydet sorulur
            doc.source_path = self.suggested_name(t)
            self.win.gui_host.doc = doc
            self.ctx.path_changed.emit()
        self.ctx.status.emit(f"Videodan açıldı: {t.label()}")
        self.accept()
        return True

    def save_track(self, path: str | None = None):
        t = self.current()
        if t is None:
            return None
        if path is None:
            path, _ = QFileDialog.getSaveFileName(self, "Altyazıyı kaydet", self.suggested_name(t),
                                                  f"Altyazı (*{t.ext})")
            if not path:
                return None
        if self._extract_to(t, path):
            self.info.setText(f"Kaydedildi: {path}")
            return path
        return None

    def extract_fonts(self, dest: str | None = None):
        if not self.video:
            return []
        if dest is None:
            dest = QFileDialog.getExistingDirectory(self, "Ekleri çıkar", os.path.dirname(self.video))
            if not dest:
                return []
        try:
            files = X.dump_attachments(self.video, dest)
        except RuntimeError as e:
            self.info.setText(str(e))
            return []
        self.info.setText(f"{len(files)} ek çıkarıldı: {dest}")
        return files
