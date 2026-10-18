from __future__ import annotations
from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (
    QCheckBox, QColorDialog, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox, QFormLayout, QGridLayout,
    QHBoxLayout, QLabel, QLineEdit, QListWidget, QPlainTextEdit, QPushButton, QRadioButton, QSpinBox,
    QTableWidget, QTableWidgetItem, QVBoxLayout, QWidget, QFontComboBox, QPushButton, QGroupBox,
)
from ..core.ass import Style


# ---------------------------------------------------------------- generic helpers
def ass_to_qcolor(s: str) -> QColor:
    s = s.strip().lstrip("&Hh").rstrip("&")
    s = s.zfill(8)
    a, b, g, r = (int(s[i:i + 2], 16) for i in (0, 2, 4, 6))
    return QColor(r, g, b, 255 - a)


def qcolor_to_ass(c: QColor) -> str:
    return f"&H{255 - c.alpha():02X}{c.blue():02X}{c.green():02X}{c.red():02X}"


class ColorButton(QPushButton):
    def __init__(self, ass: str = "&H00FFFFFF", parent=None):
        super().__init__(parent)
        self.set_ass(ass)
        self.clicked.connect(self._pick)

    def set_ass(self, ass: str) -> None:
        self._ass = ass
        c = ass_to_qcolor(ass)
        self.setText(ass)
        self.setStyleSheet(f"background-color: rgb({c.red()},{c.green()},{c.blue()});"
                           f"color: {'black' if c.lightness() > 128 else 'white'};")

    def ass(self) -> str:
        return self._ass

    def _pick(self):
        c = QColorDialog.getColor(ass_to_qcolor(self._ass), self, "Renk", QColorDialog.ShowAlphaChannel)
        if c.isValid():
            self.set_ass(qcolor_to_ass(c))


# ---------------------------------------------------------------- shift times
class ShiftDialog(QDialog):
    def __init__(self, parent=None, has_selection=True):
        super().__init__(parent)
        self.setWindowTitle("Zamanlamayı kaydır")
        lay = QVBoxLayout(self)
        form = QFormLayout()
        self.ms = QSpinBox(); self.ms.setRange(-3_600_000, 3_600_000); self.ms.setSuffix(" ms"); self.ms.setSingleStep(100)
        form.addRow("Kaydırma:", self.ms)
        self.what = QComboBox(); self.what.addItems(["Başla ve bitir", "Yalnızca başla", "Yalnızca bitir"])
        form.addRow("Uygula:", self.what)
        lay.addLayout(form)
        g = QGroupBox("Kapsam"); gl = QVBoxLayout(g)
        self.r_all = QRadioButton("Tüm satırlar"); self.r_sel = QRadioButton("Seçili satırlar"); self.r_from = QRadioButton("Seçilenden sona kadar")
        (self.r_sel if has_selection else self.r_all).setChecked(True)
        for r in (self.r_all, self.r_sel, self.r_from):
            gl.addWidget(r)
        lay.addWidget(g)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.accept); bb.rejected.connect(self.reject)
        lay.addWidget(bb)

    def values(self):
        scope = "all" if self.r_all.isChecked() else "sel" if self.r_sel.isChecked() else "from"
        return self.ms.value(), self.what.currentIndex(), scope


# ---------------------------------------------------------------- find / replace
class FindReplaceDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Bul ve değiştir")
        lay = QFormLayout(self)
        self.find = QLineEdit(); self.repl = QLineEdit()
        lay.addRow("Bul:", self.find); lay.addRow("Değiştir:", self.repl)
        self.field = QComboBox(); self.field.addItems(["text", "actor", "effect", "style"])
        lay.addRow("Alan:", self.field)
        self.regex = QCheckBox("Düzenli ifade"); self.case = QCheckBox("Büyük/küçük harf duyarsız"); self.word = QCheckBox("Tam sözcük")
        self.sel_only = QCheckBox("Yalnızca seçili satırlar")
        for w in (self.regex, self.case, self.word, self.sel_only):
            lay.addRow(w)
        bb = QDialogButtonBox(); self.btn_next = bb.addButton("Sonrakini bul", QDialogButtonBox.ActionRole)
        self.btn_all = bb.addButton("Tümünü değiştir", QDialogButtonBox.AcceptRole)
        bb.addButton(QDialogButtonBox.Close).clicked.connect(self.close)
        lay.addRow(bb)


# ---------------------------------------------------------------- style editor
class StyleEditor(QDialog):
    def __init__(self, doc, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Stil yöneticisi")
        self.doc = doc
        self.resize(760, 520)
        root = QHBoxLayout(self)
        left = QVBoxLayout()
        self.list = QListWidget(); left.addWidget(self.list)
        row = QHBoxLayout()
        for text, fn in (("Yeni", self.new), ("Kopyala", self.copy), ("Sil", self.delete)):
            b = QPushButton(text); b.clicked.connect(fn); row.addWidget(b)
        left.addLayout(row); root.addLayout(left, 1)
        form = QFormLayout(); root.addLayout(form, 2)
        self.name = QLineEdit(); self.font = QFontComboBox(); self.size = QDoubleSpinBox(); self.size.setRange(1, 999)
        self.c1, self.c2, self.c3, self.c4 = (ColorButton() for _ in range(4))
        self.bold, self.italic, self.under, self.strike = (QCheckBox(t) for t in ("Kalın", "İtalik", "Altı çizili", "Üstü çizili"))
        self.sx, self.sy = QDoubleSpinBox(), QDoubleSpinBox()
        for s in (self.sx, self.sy):
            s.setRange(1, 1000)
        self.spacing = QDoubleSpinBox(); self.spacing.setRange(-100, 100)
        self.angle = QDoubleSpinBox(); self.angle.setRange(-360, 360)
        self.bstyle = QComboBox(); self.bstyle.addItems(["Kontur+gölge", "Opak kutu"])
        self.outline = QDoubleSpinBox(); self.shadow = QDoubleSpinBox()
        self.align = QComboBox(); self.align.addItems([f"{i}" for i in range(1, 10)])
        self.ml, self.mr, self.mv = QSpinBox(), QSpinBox(), QSpinBox()
        for s in (self.ml, self.mr, self.mv):
            s.setRange(0, 9999)
        self.enc = QSpinBox(); self.enc.setRange(0, 255)
        form.addRow("Ad:", self.name); form.addRow("Yazı tipi:", self.font); form.addRow("Boyut:", self.size)
        form.addRow("Birincil:", self.c1); form.addRow("İkincil:", self.c2); form.addRow("Kontur:", self.c3); form.addRow("Gölge:", self.c4)
        hb = QHBoxLayout()
        for c in (self.bold, self.italic, self.under, self.strike):
            hb.addWidget(c)
        form.addRow(hb)
        form.addRow("Ölçek X / Y:", self._pair(self.sx, self.sy)); form.addRow("Aralık:", self.spacing); form.addRow("Açı:", self.angle)
        form.addRow("Kenarlık stili:", self.bstyle); form.addRow("Kontur / gölge:", self._pair(self.outline, self.shadow))
        form.addRow("Hizalama (numpad):", self.align); form.addRow("Kenar boşluğu S/R/V:", self._pair(self.ml, self.mr, self.mv))
        form.addRow("Kodlama:", self.enc)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.accept); bb.rejected.connect(self.reject); form.addRow(bb)
        self.styles = [Style(**{**s.__dict__}) for s in doc.styles]
        self.cur = -1
        self.list.currentRowChanged.connect(self.select)
        self.refill(0)

    @staticmethod
    def _pair(*ws):
        w = QWidget(); l = QHBoxLayout(w); l.setContentsMargins(0, 0, 0, 0)
        for x in ws:
            l.addWidget(x)
        return w

    def refill(self, row):
        self.cur = -1
        self.list.clear()
        self.list.addItems([s.name for s in self.styles])
        self.list.setCurrentRow(min(row, len(self.styles) - 1))

    def store(self):
        if not (0 <= self.cur < len(self.styles)):
            return
        s = self.styles[self.cur]
        s.name = self.name.text() or s.name; s.fontname = self.font.currentFont().family(); s.fontsize = self.size.value()
        s.primary, s.secondary, s.outline_c, s.back = self.c1.ass(), self.c2.ass(), self.c3.ass(), self.c4.ass()
        s.bold, s.italic, s.underline, s.strikeout = self.bold.isChecked(), self.italic.isChecked(), self.under.isChecked(), self.strike.isChecked()
        s.scale_x, s.scale_y, s.spacing, s.angle = self.sx.value(), self.sy.value(), self.spacing.value(), self.angle.value()
        s.border_style = 3 if self.bstyle.currentIndex() == 1 else 1
        s.outline, s.shadow = self.outline.value(), self.shadow.value()
        s.align = self.align.currentIndex() + 1
        s.margin_l, s.margin_r, s.margin_v, s.encoding = self.ml.value(), self.mr.value(), self.mv.value(), self.enc.value()

    def select(self, row):
        self.store()
        self.cur = row
        if row < 0:
            return
        s = self.styles[row]
        self.name.setText(s.name); self.font.setCurrentFont(__import__("PySide6.QtGui", fromlist=["QFont"]).QFont(s.fontname))
        self.size.setValue(s.fontsize)
        self.c1.set_ass(s.primary); self.c2.set_ass(s.secondary); self.c3.set_ass(s.outline_c); self.c4.set_ass(s.back)
        self.bold.setChecked(s.bold); self.italic.setChecked(s.italic); self.under.setChecked(s.underline); self.strike.setChecked(s.strikeout)
        self.sx.setValue(s.scale_x); self.sy.setValue(s.scale_y); self.spacing.setValue(s.spacing); self.angle.setValue(s.angle)
        self.bstyle.setCurrentIndex(1 if s.border_style == 3 else 0)
        self.outline.setValue(s.outline); self.shadow.setValue(s.shadow)
        self.align.setCurrentIndex(max(0, min(8, s.align - 1)))
        self.ml.setValue(s.margin_l); self.mr.setValue(s.margin_r); self.mv.setValue(s.margin_v); self.enc.setValue(s.encoding)

    def new(self):
        self.store(); self.styles.append(Style(name=f"Style{len(self.styles) + 1}")); self.refill(len(self.styles) - 1)

    def copy(self):
        self.store()
        if self.cur >= 0:
            c = Style(**{**self.styles[self.cur].__dict__}); c.name += " copy"; self.styles.append(c); self.refill(len(self.styles) - 1)

    def delete(self):
        if len(self.styles) > 1 and self.cur >= 0:
            del self.styles[self.cur]; self.cur = -1; self.refill(0)

    def result_styles(self):
        self.store()
        return self.styles


# ---------------------------------------------------------------- issues
class IssuesDialog(QDialog):
    def __init__(self, issues, parent=None, jump=None):
        super().__init__(parent)
        self.setWindowTitle("Kalite kontrol")
        self.resize(640, 420)
        lay = QVBoxLayout(self)
        t = QTableWidget(len(issues), 3)
        t.setHorizontalHeaderLabels(["Satır", "Tür", "Açıklama"])
        for r, i in enumerate(issues):
            for c, v in enumerate((str(i.index + 1), i.kind, i.message)):
                t.setItem(r, c, QTableWidgetItem(v))
        t.horizontalHeader().setStretchLastSection(True)
        t.setEditTriggers(QTableWidget.NoEditTriggers)
        t.setSelectionBehavior(QTableWidget.SelectRows)
        if jump:
            t.cellDoubleClicked.connect(lambda r, c: jump(issues[r].index))
        lay.addWidget(t)
        lay.addWidget(QLabel(f"{len(issues)} sorun bulundu"))


# ---------------------------------------------------------------- automation dialog
class AutomationDialog(QDialog):
    """Builds a Qt dialog from an Aegisub Automation dialog description."""

    def __init__(self, controls, buttons, ids, parent=None, title="Otomasyon"):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.controls = controls
        self.buttons = buttons
        self.ids = ids or {}
        self.widgets = {}
        self.clicked = False
        lay = QVBoxLayout(self)
        grid = QGridLayout()
        lay.addLayout(grid)
        for c in controls:
            cls = c.get("class")
            x, y = int(c.get("x", 0) or 0), int(c.get("y", 0) or 0)
            w, h = int(c.get("width", 1) or 1), int(c.get("height", 1) or 1)
            widget = None
            if cls == "label":
                widget = QLabel(str(c.get("label", "")))
            elif cls == "edit":
                widget = QLineEdit(str(c.get("text", ""))); widget.setToolTip(str(c.get("hint", "")))
            elif cls == "textbox":
                widget = QPlainTextEdit(str(c.get("text", ""))); widget.setMinimumHeight(20 * max(h, 2))
            elif cls == "intedit":
                widget = QSpinBox(); widget.setRange(int(c.get("min", -2**31)), int(c.get("max", 2**31 - 1)))
                widget.setValue(int(c.get("value", 0) or 0))
            elif cls == "floatedit":
                widget = QDoubleSpinBox(); widget.setDecimals(4)
                widget.setRange(float(c.get("min", -1e9)), float(c.get("max", 1e9)))
                widget.setSingleStep(float(c.get("step", 0.1) or 0.1)); widget.setValue(float(c.get("value", 0) or 0))
            elif cls == "checkbox":
                widget = QCheckBox(str(c.get("label", ""))); widget.setChecked(bool(c.get("value", False)))
            elif cls == "dropdown":
                widget = QComboBox()
                items = c.get("items") or {}
                widget.addItems([str(items[k]) for k in sorted(items)])
                widget.setCurrentText(str(c.get("value", "")))
            elif cls in ("color", "coloralpha", "alpha"):
                widget = ColorButton(str(c.get("value", c.get("text", "&H00FFFFFF"))) if cls != "color" or True else "")
            if widget is not None:
                if cls == "label" or cls == "checkbox":
                    pass
                grid.addWidget(widget, y, x, h, w)
                if c.get("name"):
                    self.widgets[c["name"]] = (cls, widget)
        bb = QDialogButtonBox()
        self.btns = {}
        for b in buttons:
            btn = bb.addButton(str(b), QDialogButtonBox.ActionRole)
            btn.clicked.connect(lambda _=False, name=b: self._press(name))
            self.btns[b] = btn
        lay.addWidget(bb)

    def _press(self, name):
        self.clicked = name
        cancel = self.ids.get("cancel")
        if name == cancel or str(name).lower() in ("cancel", "iptal"):
            self.reject()
        else:
            self.accept()

    def results(self):
        out = {}
        for name, (cls, w) in self.widgets.items():
            if cls in ("edit",):
                out[name] = w.text()
            elif cls == "textbox":
                out[name] = w.toPlainText()
            elif cls in ("intedit",):
                out[name] = int(w.value())
            elif cls == "floatedit":
                out[name] = float(w.value())
            elif cls == "checkbox":
                out[name] = w.isChecked()
            elif cls == "dropdown":
                out[name] = w.currentText()
            elif cls in ("color", "coloralpha", "alpha"):
                out[name] = w.ass()
        return out
