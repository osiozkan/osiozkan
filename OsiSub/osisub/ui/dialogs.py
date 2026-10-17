"""Stil yöneticisi, Bul/Değiştir, Betik özellikleri, Zaman kaydırma."""

from __future__ import annotations

import re

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QFont, QFontDatabase
from PySide6.QtWidgets import (QAbstractItemView, QButtonGroup, QCheckBox, QComboBox, QDialog,
                               QDialogButtonBox, QDoubleSpinBox, QFontComboBox, QFormLayout,
                               QGridLayout, QGroupBox, QHBoxLayout, QInputDialog, QLabel,
                               QLineEdit, QListWidget, QMessageBox, QPushButton, QRadioButton,
                               QSpinBox, QVBoxLayout, QWidget)

from ..core.assfile import Style
from ..core.asstime import format_time, parse_time
from .automation_ui import ColorButton, parse_any_color
from .widgets import TimeEdit


# ---------------------------------------------------------------------------
# Renk biçimleri (stil: &HAABBGGRR)
# ---------------------------------------------------------------------------

def style_color_to_btn(s: str) -> str:
    r, g, b, a = parse_any_color(s)
    return f"#{r:02X}{g:02X}{b:02X}{a:02X}"


def btn_to_style_color(v: str) -> str:
    r, g, b, a = parse_any_color(v)
    return f"&H{a:02X}{b:02X}{g:02X}{r:02X}"


# ---------------------------------------------------------------------------
# Stil düzenleyici
# ---------------------------------------------------------------------------

class StyleEditor(QDialog):
    def __init__(self, style: Style, existing_names: list[str], parent=None):
        super().__init__(parent)
        self.setWindowTitle(f"Stil: {style.name}")
        self.orig = style
        self.existing = [n for n in existing_names if n != style.name]
        self.name = QLineEdit(style.name)
        self.font = QFontComboBox()
        self.font.setEditable(True)
        self.font.setCurrentText(style.fontname)
        self.size = QDoubleSpinBox()
        self.size.setRange(0, 10000)
        self.size.setDecimals(2)
        self.size.setValue(style.fontsize)
        self.bold, self.italic = QCheckBox("Kalın"), QCheckBox("İtalik")
        self.underline, self.strike = QCheckBox("Altı çizili"), QCheckBox("Üstü çizili")
        self.bold.setChecked(style.bold)
        self.italic.setChecked(style.italic)
        self.underline.setChecked(style.underline)
        self.strike.setChecked(style.strikeout)
        self.c1 = ColorButton(style_color_to_btn(style.color1), with_alpha=True)
        self.c2 = ColorButton(style_color_to_btn(style.color2), with_alpha=True)
        self.c3 = ColorButton(style_color_to_btn(style.color3), with_alpha=True)
        self.c4 = ColorButton(style_color_to_btn(style.color4), with_alpha=True)
        self.outline = QDoubleSpinBox()
        self.outline.setRange(0, 1000)
        self.outline.setValue(style.outline)
        self.shadow = QDoubleSpinBox()
        self.shadow.setRange(0, 1000)
        self.shadow.setValue(style.shadow)
        self.opaque = QCheckBox("Opak kutu (BorderStyle 3)")
        self.opaque.setChecked(style.borderstyle == 3)
        self.align = QButtonGroup(self)
        ag = QGridLayout()
        for i, n in enumerate((7, 8, 9, 4, 5, 6, 1, 2, 3)):
            rb = QRadioButton(str(n))
            self.align.addButton(rb, n)
            ag.addWidget(rb, i // 3, i % 3)
            if n == style.align:
                rb.setChecked(True)
        self.ml, self.mr, self.mv = QSpinBox(), QSpinBox(), QSpinBox()
        for sb, v in ((self.ml, style.margin_l), (self.mr, style.margin_r), (self.mv, style.margin_v)):
            sb.setRange(0, 9999)
            sb.setValue(v)
        self.sx, self.sy, self.sp, self.ang = (QDoubleSpinBox() for _ in range(4))
        for sb, v, lo, hi in ((self.sx, style.scale_x, 0, 10000), (self.sy, style.scale_y, 0, 10000),
                              (self.sp, style.spacing, -1000, 1000), (self.ang, style.angle, -360, 360)):
            sb.setRange(lo, hi)
            sb.setDecimals(2)
            sb.setValue(v)
        self.enc = QSpinBox()
        self.enc.setRange(0, 255)
        self.enc.setValue(style.encoding)
        self.enc.setToolTip("1 = varsayılan, 162 = Türkçe (VSFilter)")

        form = QFormLayout()
        form.addRow("Ad", self.name)
        fh = QHBoxLayout()
        fh.addWidget(self.font, 3)
        fh.addWidget(self.size, 1)
        form.addRow("Yazı tipi", fh)
        bh = QHBoxLayout()
        for w in (self.bold, self.italic, self.underline, self.strike):
            bh.addWidget(w)
        form.addRow("", bh)
        ch = QHBoxLayout()
        for lbl, w in (("Ana", self.c1), ("İkincil", self.c2), ("Kenar", self.c3), ("Gölge", self.c4)):
            v = QVBoxLayout()
            v.addWidget(QLabel(lbl))
            v.addWidget(w)
            ch.addLayout(v)
        form.addRow("Renkler", ch)
        oh = QHBoxLayout()
        oh.addWidget(QLabel("Kenar"))
        oh.addWidget(self.outline)
        oh.addWidget(QLabel("Gölge"))
        oh.addWidget(self.shadow)
        oh.addWidget(self.opaque)
        form.addRow("Kenarlık", oh)
        mh = QHBoxLayout()
        for lbl, w in (("Sol", self.ml), ("Sağ", self.mr), ("Dikey", self.mv)):
            mh.addWidget(QLabel(lbl))
            mh.addWidget(w)
        form.addRow("Kenar boşlukları", mh)
        gw = QWidget()
        gw.setLayout(ag)
        form.addRow("Hizalama", gw)
        sh = QHBoxLayout()
        for lbl, w in (("Ölçek X", self.sx), ("Ölçek Y", self.sy), ("Aralık", self.sp), ("Açı", self.ang)):
            sh.addWidget(QLabel(lbl))
            sh.addWidget(w)
        form.addRow("Biçim", sh)
        form.addRow("Kodlama", self.enc)
        self.preview = QLabel("Örnek metin: ĞÜŞİÖÇ ğüşıöç 0123")
        self.preview.setMinimumHeight(70)
        self.preview.setAlignment(Qt.AlignCenter)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self._ok)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(self.preview)
        lay.addWidget(bb)
        for w in (self.font, ):
            w.currentTextChanged.connect(self._update_preview)
        for w in (self.size, self.sx, self.sy, self.sp):
            w.valueChanged.connect(self._update_preview)
        for w in (self.bold, self.italic, self.underline, self.strike):
            w.toggled.connect(self._update_preview)
        self.c1.clicked.connect(self._update_preview)
        self.c3.clicked.connect(self._update_preview)
        self._update_preview()

    def _update_preview(self, *_):
        f = QFont(self.font.currentText())
        f.setPixelSize(max(8, min(64, int(self.size.value() * 0.6))))
        f.setBold(self.bold.isChecked())
        f.setItalic(self.italic.isChecked())
        f.setUnderline(self.underline.isChecked())
        f.setStrikeOut(self.strike.isChecked())
        f.setLetterSpacing(QFont.AbsoluteSpacing, self.sp.value() * 0.6)
        f.setStretch(max(1, min(4000, int(self.sx.value()))))
        self.preview.setFont(f)
        r, g, b, _ = self.c1.rgba
        r3, g3, b3, _ = self.c3.rgba
        self.preview.setStyleSheet(f"color: rgb({r},{g},{b}); background: rgb({max(r3, 40)},{max(g3, 40)},{max(b3, 40)});")

    def _ok(self):
        name = self.name.text().strip()
        if not name:
            QMessageBox.warning(self, "Stil", "Stil adı boş olamaz.")
            return
        if name in self.existing:
            QMessageBox.warning(self, "Stil", f"'{name}' adında başka bir stil var.")
            return
        if "," in name:
            QMessageBox.warning(self, "Stil", "Stil adında virgül olamaz.")
            return
        self.accept()

    def result_style(self) -> Style:
        return self.orig.replace(
            name=self.name.text().strip(), fontname=self.font.currentText().strip(),
            fontsize=self.size.value(), bold=self.bold.isChecked(), italic=self.italic.isChecked(),
            underline=self.underline.isChecked(), strikeout=self.strike.isChecked(),
            color1=btn_to_style_color(self.c1.value()), color2=btn_to_style_color(self.c2.value()),
            color3=btn_to_style_color(self.c3.value()), color4=btn_to_style_color(self.c4.value()),
            outline=self.outline.value(), shadow=self.shadow.value(),
            borderstyle=3 if self.opaque.isChecked() else 1,
            align=self.align.checkedId() if self.align.checkedId() > 0 else 2,
            margin_l=self.ml.value(), margin_r=self.mr.value(), margin_v=self.mv.value(),
            scale_x=self.sx.value(), scale_y=self.sy.value(), spacing=self.sp.value(),
            angle=self.ang.value(), encoding=self.enc.value())


class StyleManager(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Stil yöneticisi")
        self.resize(460, 420)
        self.styles = list(ctx.doc.styles)
        self.renames: dict[str, str] = {}
        self.list = QListWidget()
        self.list.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.list.itemDoubleClicked.connect(lambda *_: self._edit())
        btns = {}
        v = QVBoxLayout()
        for key, label in (("new", "Yeni"), ("edit", "Düzenle"), ("copy", "Kopyala"),
                           ("delete", "Sil"), ("up", "Yukarı"), ("down", "Aşağı"),
                           ("sort", "A→Z sırala")):
            b = QPushButton(label)
            btns[key] = b
            v.addWidget(b)
        v.addStretch(1)
        btns["new"].clicked.connect(self._new)
        btns["edit"].clicked.connect(self._edit)
        btns["copy"].clicked.connect(self._copy)
        btns["delete"].clicked.connect(self._delete)
        btns["up"].clicked.connect(lambda: self._move(-1))
        btns["down"].clicked.connect(lambda: self._move(1))
        btns["sort"].clicked.connect(self._sort)
        h = QHBoxLayout()
        h.addWidget(self.list, 1)
        h.addLayout(v)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self._apply)
        bb.rejected.connect(self.reject)
        self.usage = QLabel("")
        lay = QVBoxLayout(self)
        lay.addLayout(h)
        lay.addWidget(self.usage)
        lay.addWidget(bb)
        self.list.currentRowChanged.connect(self._usage)
        self._fill()

    def _fill(self, select: int | None = None):
        self.list.clear()
        for s in self.styles:
            self.list.addItem(s.name)
        if select is not None and 0 <= select < len(self.styles):
            self.list.setCurrentRow(select)

    def _usage(self, row):
        if 0 <= row < len(self.styles):
            name = self.styles[row].name
            n = sum(1 for e in self.ctx.doc.events if e.style == name)
            s = self.styles[row]
            self.usage.setText(f"{name}: {s.fontname} {s.fontsize:g} · {n} satırda kullanılıyor")

    def _names(self):
        return [s.name for s in self.styles]

    def _new(self):
        base = Style()
        name, ok = QInputDialog.getText(self, "Yeni stil", "Ad:", text="Yeni stil")
        if ok and name.strip():
            st = base.replace(name=name.strip())
            ed = StyleEditor(st, self._names(), self)
            if ed.exec():
                self.styles.append(ed.result_style())
                self._fill(len(self.styles) - 1)

    def _edit(self):
        row = self.list.currentRow()
        if not (0 <= row < len(self.styles)):
            return
        old = self.styles[row]
        ed = StyleEditor(old, self._names(), self)
        if ed.exec():
            new = ed.result_style()
            if new.name != old.name:
                ans = QMessageBox.question(self, "Stil adı değişti",
                                           f"'{old.name}' kullanan satırlar da '{new.name}' olarak güncellensin mi?")
                if ans == QMessageBox.Yes:
                    self.renames[old.name] = new.name
            self.styles[row] = new
            self._fill(row)

    def _copy(self):
        row = self.list.currentRow()
        if not (0 <= row < len(self.styles)):
            return
        src = self.styles[row]
        names = self._names()
        n = 2
        name = f"{src.name} - Kopya"
        while name in names:
            name = f"{src.name} - Kopya {n}"
            n += 1
        new = src.replace(name=name, raw=None, sig=None, lead=())
        self.styles.insert(row + 1, new)
        self._fill(row + 1)

    def _delete(self):
        rows = sorted({i.row() for i in self.list.selectedIndexes()}, reverse=True)
        if not rows:
            return
        names = [self.styles[r].name for r in rows]
        used = [n for n in names if any(e.style == n for e in self.ctx.doc.events)]
        msg = "Silinsin mi: " + ", ".join(names)
        if used:
            msg += "\n\nUyarı: şu stiller satırlarda kullanılıyor: " + ", ".join(used)
        if QMessageBox.question(self, "Stil sil", msg) == QMessageBox.Yes:
            for r in rows:
                del self.styles[r]
            self._fill(min(rows[-1], len(self.styles) - 1))

    def _move(self, d):
        row = self.list.currentRow()
        j = row + d
        if 0 <= row < len(self.styles) and 0 <= j < len(self.styles):
            self.styles[row], self.styles[j] = self.styles[j], self.styles[row]
            self._fill(j)

    def _sort(self):
        self.styles.sort(key=lambda s: s.name.lower())
        self._fill(0)

    def _apply(self):
        self.ctx.set_styles(self.styles, "Stil yöneticisi", self.renames or None)
        self.accept()


# ---------------------------------------------------------------------------
# Bul / Değiştir
# ---------------------------------------------------------------------------

class FindReplace(QDialog):
    FIELDS = {"Metin": "text", "Aktör": "actor", "Stil": "style", "Efekt": "effect"}

    def __init__(self, ctx, parent=None, replace: bool = True):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Bul ve değiştir" if replace else "Bul")
        self.find = QLineEdit()
        self.repl = QLineEdit()
        self.field = QComboBox()
        self.field.addItems(list(self.FIELDS))
        self.case = QCheckBox("Büyük/küçük harf duyarlı")
        self.regex = QCheckBox("Düzenli ifade")
        self.sel_only = QCheckBox("Yalnız seçili satırlar")
        self.skip_tags = QCheckBox("Etiket bloklarını atla ({…})")
        self.skip_tags.setChecked(True)
        self.skip_comments = QCheckBox("Yorum satırlarını atla")
        form = QFormLayout()
        form.addRow("Bul", self.find)
        if replace:
            form.addRow("Değiştir", self.repl)
        form.addRow("Alan", self.field)
        for w in (self.case, self.regex, self.sel_only, self.skip_tags, self.skip_comments):
            form.addRow("", w)
        b_next = QPushButton("Sonrakini bul")
        b_next.setDefault(True)
        b_next.clicked.connect(self.find_next)
        h = QHBoxLayout()
        h.addWidget(b_next)
        if replace:
            b_one = QPushButton("Değiştir")
            b_all = QPushButton("Tümünü değiştir")
            b_one.clicked.connect(self.replace_one)
            b_all.clicked.connect(self.replace_all)
            h.addWidget(b_one)
            h.addWidget(b_all)
        b_close = QPushButton("Kapat")
        b_close.clicked.connect(self.close)
        h.addWidget(b_close)
        self.info = QLabel("")
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(self.info)
        lay.addLayout(h)

    def _pattern(self):
        pat = self.find.text()
        if not pat:
            return None
        flags = 0 if self.case.isChecked() else re.IGNORECASE
        try:
            return re.compile(pat if self.regex.isChecked() else re.escape(pat), flags)
        except re.error as e:
            self.info.setText(f"Düzenli ifade hatası: {e}")
            return None

    def _rows(self):
        if self.sel_only.isChecked():
            return self.ctx.selected_rows()
        return list(range(len(self.ctx.doc.events)))

    def _spans(self, text, rx):
        """Eşleşmeler; etiket bloklarının içi isteğe göre atlanır."""
        out = []
        blocks = [(m.start(), m.end()) for m in re.finditer(r"\{[^}]*\}", text)] if self.skip_tags.isChecked() else []
        for m in rx.finditer(text):
            if m.end() == m.start():
                continue
            if any(a <= m.start() < b for a, b in blocks):
                continue
            out.append(m)
        return out

    def find_next(self):
        rx = self._pattern()
        if not rx:
            return
        field = self.FIELDS[self.field.currentText()]
        rows = self._rows()
        if not rows:
            return
        start = self.ctx.active
        order = [r for r in rows if r > start] + [r for r in rows if r <= start]
        for r in order:
            ev = self.ctx.doc.events[r]
            if self.skip_comments.isChecked() and ev.comment:
                continue
            if self._spans(getattr(ev, field), rx):
                self.ctx.set_active(r, select_only=True)
                self.info.setText(f"Bulundu: satır {r + 1}")
                return r
        self.info.setText("Bulunamadı.")
        return None

    def _sub(self, text, rx, count=0):
        spans = self._spans(text, rx)
        if count:
            spans = spans[:count]
        if not spans:
            return text, 0
        out, pos = [], 0
        repl = self.repl.text()
        for m in spans:
            out.append(text[pos:m.start()])
            out.append(m.expand(repl) if self.regex.isChecked() else repl)
            pos = m.end()
        out.append(text[pos:])
        return "".join(out), len(spans)

    def replace_one(self):
        rx = self._pattern()
        if not rx:
            return
        field = self.FIELDS[self.field.currentText()]
        r = self.ctx.active
        ev = self.ctx.active_event()
        if ev is not None:
            new, n = self._sub(getattr(ev, field), rx, count=1)
            if n:
                self.ctx.update_events({r: {field: new}}, "Değiştir")
        self.find_next()

    def replace_all(self):
        rx = self._pattern()
        if not rx:
            return
        field = self.FIELDS[self.field.currentText()]
        changes, total = {}, 0
        for r in self._rows():
            ev = self.ctx.doc.events[r]
            if self.skip_comments.isChecked() and ev.comment:
                continue
            new, n = self._sub(getattr(ev, field), rx)
            if n:
                changes[r] = {field: new}
                total += n
        if changes:
            self.ctx.update_events(changes, f"Tümünü değiştir ({total})")
        self.info.setText(f"{total} değişiklik, {len(changes)} satır.")


# ---------------------------------------------------------------------------
# Betik özellikleri
# ---------------------------------------------------------------------------

class ScriptProps(QDialog):
    KEYS = [("Title", "Başlık"), ("Original Script", "Özgün betik"), ("Original Translation", "Çeviri"),
            ("Original Editing", "Düzenleme"), ("Original Timing", "Zamanlama"),
            ("Script Updated By", "Güncelleyen")]

    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Betik özellikleri")
        d = ctx.doc
        form = QFormLayout()
        self.edits = {}
        for k, lbl in self.KEYS:
            e = QLineEdit(d.get_info(k, "") or "")
            self.edits[k] = e
            form.addRow(lbl, e)
        self.resx, self.resy = QSpinBox(), QSpinBox()
        for sb in (self.resx, self.resy):
            sb.setRange(0, 20000)
        x, y = d.play_res()
        self.resx.setValue(x)
        self.resy.setValue(y)
        rh = QHBoxLayout()
        rh.addWidget(self.resx)
        rh.addWidget(QLabel("×"))
        rh.addWidget(self.resy)
        form.addRow("Çözünürlük (PlayRes)", rh)
        self.wrap = QComboBox()
        self.wrap.addItems(["0: Akıllı (üst satır geniş)", "1: Satır sonunda kır", "2: Kırma yok",
                            "3: Akıllı (alt satır geniş)"])
        try:
            self.wrap.setCurrentIndex(int(d.get_info("WrapStyle", "0") or 0))
        except ValueError:
            pass
        form.addRow("Satır kaydırma", self.wrap)
        self.sbas = QCheckBox("Kenarlık ve gölgeyi ölçekle (ScaledBorderAndShadow)")
        self.sbas.setChecked((d.get_info("ScaledBorderAndShadow", "yes") or "").lower() == "yes")
        form.addRow("", self.sbas)
        self.ycbcr = QComboBox()
        self.ycbcr.addItems(["None", "TV.601", "TV.709", "TV.FCC", "TV.240M", "PC.601", "PC.709"])
        self.ycbcr.setCurrentText(d.get_info("YCbCr Matrix", "None") or "None")
        form.addRow("YCbCr Matrix", self.ycbcr)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self._ok)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(bb)

    def _ok(self):
        d = self.ctx.doc
        for k, e in self.edits.items():
            v = e.text()
            if v or d.get_info(k) is not None:
                d.set_info(k, v)
        d.set_info("PlayResX", str(self.resx.value()))
        d.set_info("PlayResY", str(self.resy.value()))
        d.set_info("WrapStyle", str(self.wrap.currentIndex()))
        d.set_info("ScaledBorderAndShadow", "yes" if self.sbas.isChecked() else "no")
        d.set_info("YCbCr Matrix", self.ycbcr.currentText())
        self.ctx.commit_external("Betik özellikleri", selection=self.ctx.selection, active=self.ctx.active)
        self.accept()


# ---------------------------------------------------------------------------
# Zaman kaydırma
# ---------------------------------------------------------------------------

class ShiftTimes(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Zamanları kaydır")
        self.by_time = QRadioButton("Zaman")
        self.by_time.setChecked(True)
        self.by_frames = QRadioButton("Kare")
        self.by_frames.setEnabled(ctx.frame_map is not None)
        self.time = TimeEdit()
        self.frames = QSpinBox()
        self.frames.setRange(0, 10_000_000)
        self.fwd = QRadioButton("İleri")
        self.fwd.setChecked(True)
        self.back = QRadioButton("Geri")
        # Aynı pencerede iki ayrı seçim grubu: Qt'nin kendiliğinden tek grup
        # yapmasını engellemek için açık QButtonGroup kullanılır.
        self._g_mode = QButtonGroup(self)
        self._g_mode.addButton(self.by_time)
        self._g_mode.addButton(self.by_frames)
        self._g_dir = QButtonGroup(self)
        self._g_dir.addButton(self.fwd)
        self._g_dir.addButton(self.back)
        self.scope = QComboBox()
        self.scope.addItems(["Tüm satırlar", "Seçili satırlar", "Etkin satır ve sonrası"])
        self.scope.setCurrentIndex(1 if len(ctx.selected_rows()) > 1 else 0)
        self.which = QComboBox()
        self.which.addItems(["Başlangıç ve bitiş", "Yalnız başlangıç", "Yalnız bitiş"])
        g1 = QGridLayout()
        g1.addWidget(self.by_time, 0, 0)
        g1.addWidget(self.time, 0, 1)
        g1.addWidget(self.by_frames, 1, 0)
        g1.addWidget(self.frames, 1, 1)
        dh = QHBoxLayout()
        dh.addWidget(self.fwd)
        dh.addWidget(self.back)
        form = QFormLayout()
        form.addRow("Miktar", g1)
        form.addRow("Yön", dh)
        form.addRow("Satırlar", self.scope)
        form.addRow("Zamanlar", self.which)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self._ok)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(bb)

    def _ok(self):
        ctx = self.ctx
        n = len(ctx.doc.events)
        idx = self.scope.currentIndex()
        rows = list(range(n)) if idx == 0 else ctx.selected_rows() if idx == 1 else list(range(ctx.active, n))
        sign = 1 if self.fwd.isChecked() else -1
        which = self.which.currentIndex()
        fm = ctx.frame_map
        changes = {}
        for r in rows:
            ev = ctx.doc.events[r]
            s, e = ev.start, ev.end
            if self.by_frames.isChecked() and fm is not None:
                df = sign * self.frames.value()
                if which in (0, 1):
                    s = fm.time_at_frame(fm.frame_at_time(s, 1) + df, 1)
                if which in (0, 2):
                    e = fm.time_at_frame(fm.frame_at_time(e, 2) + df, 2)
            else:
                d = sign * self.time.value()
                if which in (0, 1):
                    s = max(0, s + d)
                if which in (0, 2):
                    e = max(0, e + d)
            changes[r] = {"start": s, "end": max(s, e)}
        ctx.update_events(changes, "Zamanları kaydır")
        self.accept()
