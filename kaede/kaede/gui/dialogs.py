from __future__ import annotations
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QColor, QFont
from PySide6.QtWidgets import (QDialog, QVBoxLayout, QHBoxLayout, QFormLayout, QLabel, QLineEdit, QSpinBox,
                               QDoubleSpinBox, QCheckBox, QComboBox, QPushButton, QDialogButtonBox, QListWidget,
                               QListWidgetItem, QRadioButton, QGroupBox, QTableWidget, QTableWidgetItem,
                               QColorDialog, QFontComboBox, QPlainTextEdit, QWidget, QGridLayout, QTabWidget,
                               QFileDialog, QMessageBox, QHeaderView, QAbstractItemView)

from ..core.ass import Style, Document, parse_color, format_color
from ..core import tools


def _buttons(dlg, ok="Tamam", cancel="İptal"):
    bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
    bb.button(QDialogButtonBox.Ok).setText(ok); bb.button(QDialogButtonBox.Cancel).setText(cancel)
    bb.accepted.connect(dlg.accept); bb.rejected.connect(dlg.reject)
    return bb


class ShiftDialog(QDialog):
    def __init__(self, parent, has_video: bool):
        super().__init__(parent); self.setWindowTitle("Zamanları Kaydır")
        f = QFormLayout(self)
        self.amount = QDoubleSpinBox(); self.amount.setRange(-36000, 36000); self.amount.setDecimals(3)
        self.unit = QComboBox(); self.unit.addItems(["saniye", "milisaniye", "kare"])
        if not has_video: self.unit.model().item(2).setEnabled(False)
        row = QHBoxLayout(); row.addWidget(self.amount); row.addWidget(self.unit)
        f.addRow("Miktar (− geri):", row)
        self.scope = QComboBox(); self.scope.addItems(["Seçili satırlar", "Tüm satırlar", "Seçimden sona kadar"])
        f.addRow("Kapsam:", self.scope)
        self.what = QComboBox(); self.what.addItems(["Başlangıç + Bitiş", "Yalnız başlangıç", "Yalnız bitiş"])
        f.addRow("Zaman:", self.what)
        f.addRow(_buttons(self))

    def values(self):
        a = self.amount.value(); u = self.unit.currentIndex()
        ms = int(round(a * 1000)) if u == 0 else int(round(a)) if u == 1 else 0
        frames = int(a) if u == 2 else 0
        return ms, frames, self.scope.currentIndex(), self.what.currentIndex()


class FindReplaceDialog(QDialog):
    findNext = Signal(dict); replaceOne = Signal(dict); replaceAll = Signal(dict)

    def __init__(self, parent):
        super().__init__(parent); self.setWindowTitle("Bul / Değiştir")
        self.setWindowFlag(Qt.Tool, True)
        f = QFormLayout(self)
        self.find = QLineEdit(); self.repl = QLineEdit()
        f.addRow("Bul:", self.find); f.addRow("Değiştir:", self.repl)
        self.field = QComboBox(); self.field.addItems(["Metin", "Aktör", "Stil", "Efekt"])
        f.addRow("Alan:", self.field)
        self.regex = QCheckBox("Düzenli ifade"); self.case = QCheckBox("Büyük/küçük harf duyarlı")
        self.word = QCheckBox("Tam sözcük"); self.sel = QCheckBox("Yalnız seçili satırlar")
        self.tr = QCheckBox("Türkçe i/İ ı/I eşleştirme"); self.tr.setChecked(True)
        for w in (self.regex, self.case, self.word, self.sel, self.tr): f.addRow(w)
        row = QHBoxLayout()
        for label, sig in (("Sonrakini Bul", self.findNext), ("Değiştir", self.replaceOne), ("Tümünü Değiştir", self.replaceAll)):
            b = QPushButton(label); b.clicked.connect(lambda _=False, s=sig: s.emit(self.opts())); row.addWidget(b)
        f.addRow(row)
        self.status = QLabel(""); f.addRow(self.status)

    def opts(self):
        return dict(find=self.find.text(), repl=self.repl.text(), regex=self.regex.isChecked(),
                    case=self.case.isChecked(), word=self.word.isChecked(), sel=self.sel.isChecked(),
                    tr=self.tr.isChecked(), field=["text", "actor", "style", "effect"][self.field.currentIndex()])


class StyleEditor(QDialog):
    def __init__(self, parent, style: Style, fonts=True):
        super().__init__(parent); self.setWindowTitle(f"Stil: {style.name}")
        self.s = style.copy()
        tabs = QGridLayout(self)
        f = QFormLayout()
        self.name = QLineEdit(style.name)
        self.font = QFontComboBox(); self.font.setCurrentFont(QFont(style.fontname))
        self.font.setEditable(True); self.font.setEditText(style.fontname)
        self.size = QDoubleSpinBox(); self.size.setRange(1, 1000); self.size.setValue(style.fontsize)
        self.bold = QCheckBox("Kalın"); self.bold.setChecked(style.bold)
        self.italic = QCheckBox("İtalik"); self.italic.setChecked(style.italic)
        self.under = QCheckBox("Altı çizili"); self.under.setChecked(style.underline)
        self.strike = QCheckBox("Üstü çizili"); self.strike.setChecked(style.strikeout)
        f.addRow("Ad:", self.name); f.addRow("Yazı tipi:", self.font); f.addRow("Boyut:", self.size)
        r = QHBoxLayout()
        for c in (self.bold, self.italic, self.under, self.strike): r.addWidget(c)
        f.addRow(r)
        self.colors = {}
        for key, label in (("primary", "Ana renk"), ("secondary", "İkincil"), ("outline", "Kontur"), ("back", "Gölge")):
            b = QPushButton(); b.clicked.connect(lambda _=False, k=key: self.pick(k)); self.colors[key] = b
            self._paint(key); f.addRow(label + ":", b)
        g = QFormLayout()
        def dsb(v, lo=-1000, hi=1000, dec=2):
            w = QDoubleSpinBox(); w.setRange(lo, hi); w.setDecimals(dec); w.setValue(v); return w
        self.outw = dsb(style.outline_w, 0, 100); self.shadow = dsb(style.shadow, 0, 100)
        self.sx = dsb(style.scale_x, 1, 1000); self.sy = dsb(style.scale_y, 1, 1000)
        self.spacing = dsb(style.spacing); self.angle = dsb(style.angle, -360, 360)
        self.align = QComboBox()
        for n in (7, 8, 9, 4, 5, 6, 1, 2, 3): self.align.addItem(f"{n}", n)
        self.align.setCurrentIndex(self.align.findData(style.align))
        self.ml = QSpinBox(); self.mr = QSpinBox(); self.mv = QSpinBox()
        for w, v in ((self.ml, style.margin_l), (self.mr, style.margin_r), (self.mv, style.margin_t)):
            w.setRange(0, 9999); w.setValue(v)
        self.bs = QComboBox(); self.bs.addItems(["Kontur+Gölge", "Opak kutu"]); self.bs.setCurrentIndex(1 if style.border_style == 3 else 0)
        self.enc = QSpinBox(); self.enc.setRange(0, 255); self.enc.setValue(style.encoding)
        for lab, w in (("Kontur:", self.outw), ("Gölge:", self.shadow), ("Ölçek X%:", self.sx), ("Ölçek Y%:", self.sy),
                       ("Harf aralığı:", self.spacing), ("Açı:", self.angle), ("Hizalama (numpad):", self.align),
                       ("Kenar boşluğu sol:", self.ml), ("sağ:", self.mr), ("dikey:", self.mv),
                       ("Çerçeve stili:", self.bs), ("Kodlama:", self.enc)):
            g.addRow(lab, w)
        tabs.addLayout(f, 0, 0); tabs.addLayout(g, 0, 1); tabs.addWidget(_buttons(self), 1, 0, 1, 2)

    def _paint(self, key):
        r, g, b, a = parse_color(getattr(self.s, key))
        self.colors[key].setText(getattr(self.s, key))
        self.colors[key].setStyleSheet(f"background: rgb({r},{g},{b}); color: {'#000' if r+g+b > 380 else '#fff'}")

    def pick(self, key):
        r, g, b, a = parse_color(getattr(self.s, key))
        c = QColorDialog.getColor(QColor(r, g, b, 255 - a), self, "Renk", QColorDialog.ShowAlphaChannel)
        if c.isValid():
            setattr(self.s, key, format_color(c.red(), c.green(), c.blue(), 255 - c.alpha())); self._paint(key)

    def result_style(self) -> Style:
        s = self.s
        s.name = self.name.text().strip() or "Default"; s.fontname = self.font.currentText()
        s.fontsize = self.size.value(); s.bold = self.bold.isChecked(); s.italic = self.italic.isChecked()
        s.underline = self.under.isChecked(); s.strikeout = self.strike.isChecked()
        s.outline_w = self.outw.value(); s.shadow = self.shadow.value(); s.scale_x = self.sx.value()
        s.scale_y = self.sy.value(); s.spacing = self.spacing.value(); s.angle = self.angle.value()
        s.align = self.align.currentData(); s.margin_l = self.ml.value(); s.margin_r = self.mr.value()
        s.margin_t = self.mv.value(); s.border_style = 3 if self.bs.currentIndex() else 1
        s.encoding = self.enc.value()
        return s


class StylesManager(QDialog):
    """Edit script styles; the 'library' on the right persists in app settings (Aegisub storage)."""
    def __init__(self, parent, doc: Document, library: list[Style]):
        super().__init__(parent); self.setWindowTitle("Stil Yöneticisi"); self.resize(640, 420)
        self.doc = doc; self.lib = library
        lay = QHBoxLayout(self)
        left = QVBoxLayout(); right = QVBoxLayout()
        self.lw = QListWidget(); self.rw = QListWidget()
        left.addWidget(QLabel("Betik stilleri")); left.addWidget(self.lw)
        right.addWidget(QLabel("Depo (kalıcı)")); right.addWidget(self.rw)
        mid = QVBoxLayout()
        self.btns = {}
        for key, label, fn in (("new", "Yeni", self.new), ("edit", "Düzenle", self.edit), ("copy", "Kopyala", self.copy),
                               ("del", "Sil", self.delete), ("up", "↑", lambda: self.move(-1)), ("dn", "↓", lambda: self.move(1)),
                               ("to_lib", "→ Depoya", self.to_lib), ("from_lib", "← Betiğe", self.from_lib)):
            b = QPushButton(label); b.clicked.connect(fn); mid.addWidget(b)
        mid.addStretch(1)
        lay.addLayout(left, 2); lay.addLayout(mid); lay.addLayout(right, 2)
        close = QPushButton("Kapat"); close.clicked.connect(self.accept); mid.addWidget(close)
        self.reload()

    def reload(self):
        self.lw.clear(); self.rw.clear()
        for s in self.doc.styles: self.lw.addItem(s.name)
        for s in self.lib: self.rw.addItem(s.name)

    def cur(self):
        i = self.lw.currentRow()
        return i if 0 <= i < len(self.doc.styles) else None

    def new(self):
        s = Style(name=self._uniq("Yeni")); dlg = StyleEditor(self, s)
        if dlg.exec(): self.doc.styles.append(dlg.result_style()); self.reload()

    def edit(self):
        i = self.cur()
        if i is None: return
        old = self.doc.styles[i]; dlg = StyleEditor(self, old)
        if dlg.exec():
            new = dlg.result_style()
            if new.name != old.name: tools.rename_style(self.doc, old.name, new.name)
            self.doc.styles[i] = new; self.reload()

    def copy(self):
        i = self.cur()
        if i is None: return
        s = self.doc.styles[i].copy(); s.name = self._uniq(s.name + " copy"); self.doc.styles.insert(i + 1, s); self.reload()

    def delete(self):
        i = self.cur()
        if i is None or len(self.doc.styles) <= 1: return
        del self.doc.styles[i]; self.reload()

    def move(self, d):
        i = self.cur()
        if i is None or not 0 <= i + d < len(self.doc.styles): return
        st = self.doc.styles; st[i], st[i + d] = st[i + d], st[i]; self.reload(); self.lw.setCurrentRow(i + d)

    def to_lib(self):
        i = self.cur()
        if i is not None: self.lib.append(self.doc.styles[i].copy()); self.reload()

    def from_lib(self):
        i = self.rw.currentRow()
        if 0 <= i < len(self.lib):
            s = self.lib[i].copy(); s.name = self._uniq(s.name); self.doc.styles.append(s); self.reload()

    def _uniq(self, n):
        names = {s.name for s in self.doc.styles}; k = n; c = 2
        while k in names: k = f"{n} {c}"; c += 1
        return k


class PropertiesDialog(QDialog):
    def __init__(self, parent, doc: Document, video_size=None):
        super().__init__(parent); self.setWindowTitle("Betik Özellikleri"); self.doc = doc
        f = QFormLayout(self)
        self.title = QLineEdit(doc.get_info("Title")); self.orig = QLineEdit(doc.get_info("Original Script"))
        self.trans = QLineEdit(doc.get_info("Original Translation")); self.edit = QLineEdit(doc.get_info("Original Editing"))
        self.timing = QLineEdit(doc.get_info("Original Timing")); self.update = QLineEdit(doc.get_info("Update Details"))
        rx, ry = doc.res
        self.rx = QSpinBox(); self.ry = QSpinBox()
        for w, v in ((self.rx, rx), (self.ry, ry)): w.setRange(1, 20000); w.setValue(v)
        self.wrap = QComboBox(); self.wrap.addItems(["0: akıllı (üst geniş)", "1: satır sonu", "2: sarma yok", "3: akıllı (alt geniş)"])
        self.wrap.setCurrentIndex(int(doc.get_info("WrapStyle", "0") or 0))
        self.scaled = QCheckBox("Kenar/gölge video ile ölçeklensin"); self.scaled.setChecked(doc.get_info("ScaledBorderAndShadow", "yes").lower() == "yes")
        self.matrix = QComboBox(); self.matrix.addItems(["None", "TV.601", "TV.709", "PC.601", "PC.709"])
        self.matrix.setCurrentText(doc.get_info("YCbCr Matrix", "None"))
        for lab, w in (("Başlık:", self.title), ("Orijinal betik:", self.orig), ("Çeviri:", self.trans), ("Düzenleme:", self.edit),
                       ("Zamanlama:", self.timing), ("Güncelleme:", self.update), ("Çözünürlük X:", self.rx),
                       ("Çözünürlük Y:", self.ry), ("Satır sarma:", self.wrap), ("", self.scaled), ("YCbCr:", self.matrix)):
            f.addRow(lab, w)
        row = QHBoxLayout()
        if video_size:
            b = QPushButton("Videodan al"); b.clicked.connect(lambda: (self.rx.setValue(video_size[0]), self.ry.setValue(video_size[1]))); row.addWidget(b)
        self.resample = QCheckBox("Değeri değiştirince içeriği yeniden ölçekle"); row.addWidget(self.resample)
        f.addRow(row); f.addRow(_buttons(self))

    def apply(self):
        d = self.doc
        old = d.res
        for k, w in (("Title", self.title), ("Original Script", self.orig), ("Original Translation", self.trans),
                     ("Original Editing", self.edit), ("Original Timing", self.timing), ("Update Details", self.update)):
            if w.text() or d.get_info(k): d.set_info(k, w.text())
        new = (self.rx.value(), self.ry.value())
        if new != old and self.resample.isChecked():
            tools.resample(d, *new)
        else:
            d.set_info("PlayResX", str(new[0])); d.set_info("PlayResY", str(new[1]))
        d.set_info("WrapStyle", str(self.wrap.currentIndex()))
        d.set_info("ScaledBorderAndShadow", "yes" if self.scaled.isChecked() else "no")
        d.set_info("YCbCr Matrix", self.matrix.currentText())


class TimingDialog(QDialog):
    """Fix-up pass: chain, overlaps, min/max durations, CPS."""
    def __init__(self, parent):
        super().__init__(parent); self.setWindowTitle("Zamanlama Düzeltmeleri")
        f = QFormLayout(self)
        self.overlap = QCheckBox("Çakışmaları düzelt"); self.overlap.setChecked(True)
        self.chain = QCheckBox("Küçük boşlukları kapat (zincirleme) — en çok (ms):"); self.chain.setChecked(True)
        self.chain_ms = QSpinBox(); self.chain_ms.setRange(0, 5000); self.chain_ms.setValue(300)
        self.dur = QCheckBox("Kısa süreleri uzat — en az (ms):"); self.dur.setChecked(True)
        self.min_ms = QSpinBox(); self.min_ms.setRange(100, 10000); self.min_ms.setValue(1000)
        self.cps = QCheckBox("CPS'e göre uzat — hedef cps:"); self.cps_v = QDoubleSpinBox(); self.cps_v.setValue(17)
        self.gap = QSpinBox(); self.gap.setRange(0, 1000); self.gap.setValue(0)
        self.sel = QCheckBox("Yalnız seçili satırlar")
        for a, b in ((self.overlap, None), (self.chain, self.chain_ms), (self.dur, self.min_ms), (self.cps, self.cps_v)):
            f.addRow(a, b) if b else f.addRow(a)
        f.addRow("Satırlar arası asgari boşluk (ms):", self.gap); f.addRow(self.sel); f.addRow(_buttons(self))


class QCDialog(QDialog):
    jump = Signal(int)

    def __init__(self, parent, doc, cfg: tools.QCConfig, fr=None):
        super().__init__(parent); self.setWindowTitle("Kalite Kontrol (QC)"); self.resize(720, 460)
        self.doc, self.cfg, self.fr = doc, cfg, fr
        lay = QVBoxLayout(self)
        row = QHBoxLayout()
        self.cps = QDoubleSpinBox(); self.cps.setValue(cfg.max_cps)
        self.cpl = QSpinBox(); self.cpl.setValue(cfg.max_cpl)
        self.lines = QSpinBox(); self.lines.setValue(cfg.max_lines)
        self.mind = QSpinBox(); self.mind.setRange(0, 5000); self.mind.setValue(cfg.min_duration)
        for lab, w in (("Maks CPS", self.cps), ("Maks CPL", self.cpl), ("Maks satır", self.lines), ("Min süre ms", self.mind)):
            row.addWidget(QLabel(lab)); row.addWidget(w)
        b = QPushButton("Tara"); b.clicked.connect(self.scan); row.addWidget(b)
        lay.addLayout(row)
        self.table = QTableWidget(0, 4); self.table.setHorizontalHeaderLabels(["Satır", "Önem", "Tür", "Açıklama"])
        self.table.horizontalHeader().setSectionResizeMode(3, QHeaderView.Stretch)
        self.table.setSelectionBehavior(QAbstractItemView.SelectRows); self.table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.table.cellClicked.connect(lambda r, c: self.jump.emit(self.issues[r].index))
        lay.addWidget(self.table)
        self.status = QLabel(""); lay.addWidget(self.status)
        self.issues = []
        self.scan()

    def scan(self):
        c = self.cfg
        c.max_cps, c.max_cpl, c.max_lines, c.min_duration = self.cps.value(), self.cpl.value(), self.lines.value(), self.mind.value()
        self.issues = tools.run_qc(self.doc, c, self.fr)
        self.table.setRowCount(len(self.issues))
        colors = {"error": "#ff6b6b", "warning": "#f0b429", "info": "#7aa2f7"}
        for r, i in enumerate(self.issues):
            for col, v in enumerate((str(i.index + 1), i.severity, i.code, i.message)):
                it = QTableWidgetItem(v)
                if col == 1: it.setForeground(QColor(colors.get(i.severity, "#aaa")))
                self.table.setItem(r, col, it)
        self.status.setText(f"{len(self.issues)} sorun — {sum(1 for i in self.issues if i.severity == 'error')} hata")


class AutomationDialog(QDialog):
    def __init__(self, parent, engine, dirs: list[str], on_reload, on_add):
        super().__init__(parent); self.setWindowTitle("Otomasyon Yöneticisi"); self.resize(720, 420)
        self.engine = engine
        lay = QVBoxLayout(self)
        self.table = QTableWidget(0, 5); self.table.setHorizontalHeaderLabels(["Betik", "Ad", "Sürüm", "Makrolar", "Durum"])
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.Stretch)
        lay.addWidget(self.table)
        lay.addWidget(QLabel("Betik klasörleri: " + "; ".join(dirs)))
        row = QHBoxLayout()
        for label, fn in (("Betik ekle…", on_add), ("Yeniden yükle", on_reload)):
            b = QPushButton(label); b.clicked.connect(lambda _=False, f=fn: (f(), self.fill())); row.addWidget(b)
        close = QPushButton("Kapat"); close.clicked.connect(self.accept); row.addStretch(1); row.addWidget(close)
        lay.addLayout(row)
        self.fill()

    def fill(self):
        sc = self.engine.scripts
        self.table.setRowCount(len(sc))
        for r, s in enumerate(sc):
            vals = (s.path, s.name, s.version, ", ".join([m.name for m in s.macros] + [f"[filtre] {f.name}" for f in s.filters]),
                    "HATA: " + s.error.splitlines()[0] if s.error else "OK")
            for c, v in enumerate(vals):
                it = QTableWidgetItem(v)
                if s.error: it.setToolTip(s.error); it.setForeground(QColor("#ff6b6b")) if c == 4 else None
                self.table.setItem(r, c, it)


class TextDialog(QDialog):
    def __init__(self, parent, title, text):
        super().__init__(parent); self.setWindowTitle(title); self.resize(560, 360)
        l = QVBoxLayout(self); t = QPlainTextEdit(text); t.setReadOnly(True); l.addWidget(t)
        b = QPushButton("Kapat"); b.clicked.connect(self.accept); l.addWidget(b)


class FixCommonDialog(QDialog):
    def __init__(self, parent):
        super().__init__(parent); self.setWindowTitle("Sık hataları düzelt")
        f = QFormLayout(self)
        self.opts = {}
        for key, label, on in (("ellipsis", "“...” → “…” (üç nokta)", True), ("spaces", "Boşluk/noktalama düzeni (çift boşluk, noktalamadan önceki boşluk)", True),
                               ("dialog_dash", "Diyalog çizgisi: “-Evet” → “- Evet”", True), ("tr_i", "BÜYÜK HARFLİ sözcüklerde I/İ düzelt", True),
                               ("capital_after_stop", "Nokta sonrası küçük harfi büyüt", False), ("quotes", "Düz tırnak → “ ” (etiketsiz satırlarda)", False)):
            c = QCheckBox(label); c.setChecked(on); self.opts[key] = c; f.addRow(c)
        self.sel = QCheckBox("Yalnız seçili satırlar"); f.addRow(self.sel); f.addRow(_buttons(self))

    def options(self):
        return tools.FixOptions(**{k: c.isChecked() for k, c in self.opts.items()})


class SettingsDialog(QDialog):
    def __init__(self, parent, settings):
        super().__init__(parent); self.setWindowTitle("Ayarlar"); self.resize(560, 420)
        self.s = settings
        lay = QVBoxLayout(self)
        f = QFormLayout()
        self.fps = QDoubleSpinBox(); self.fps.setDecimals(3); self.fps.setRange(1, 240); self.fps.setValue(float(settings.value("default_fps", 23.976)))
        self.cps = QDoubleSpinBox(); self.cps.setValue(float(settings.value("qc_cps", 20)))
        self.cpl = QSpinBox(); self.cpl.setRange(10, 200); self.cpl.setValue(int(settings.value("qc_cpl", 42)))
        self.auto = QSpinBox(); self.auto.setRange(0, 60); self.auto.setValue(int(settings.value("autosave_min", 2)))
        self.auto.setSpecialValueText("kapalı"); self.dark = QCheckBox("Koyu tema"); self.dark.setChecked(settings.value("dark", "true") == "true")
        self.bom = QCheckBox("ASS dosyalarını UTF-8 BOM ile kaydet"); self.bom.setChecked(settings.value("bom", "true") == "true")
        for lab, w in (("Varsayılan FPS:", self.fps), ("QC maks CPS:", self.cps), ("QC maks CPL:", self.cpl), ("Otomatik yedek (dk):", self.auto)):
            f.addRow(lab, w)
        f.addRow(self.dark); f.addRow(self.bom)
        lay.addLayout(f)
        lay.addWidget(QLabel("Ek Lua betik klasörleri (açılışta yüklenir):"))
        self.dirs = QListWidget(); self.dirs.addItems([str(x) for x in (settings.value("script_dirs", []) or [])]); lay.addWidget(self.dirs)
        row = QHBoxLayout()
        add = QPushButton("Klasör ekle…"); rem = QPushButton("Seçileni kaldır")
        add.clicked.connect(self._add); rem.clicked.connect(lambda: [self.dirs.takeItem(self.dirs.row(i)) for i in self.dirs.selectedItems()])
        row.addWidget(add); row.addWidget(rem); row.addStretch(1); lay.addLayout(row)
        lay.addWidget(_buttons(self))

    def _add(self):
        d = QFileDialog.getExistingDirectory(self, "Betik klasörü")
        if d: self.dirs.addItem(d)

    def save(self):
        s = self.s
        s.setValue("default_fps", self.fps.value()); s.setValue("qc_cps", self.cps.value()); s.setValue("qc_cpl", self.cpl.value())
        s.setValue("autosave_min", self.auto.value()); s.setValue("dark", "true" if self.dark.isChecked() else "false")
        s.setValue("bom", "true" if self.bom.isChecked() else "false")
        s.setValue("script_dirs", [self.dirs.item(i).text() for i in range(self.dirs.count())])
