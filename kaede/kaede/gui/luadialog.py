"""Build Qt dialogs from Aegisub automation dialog tables."""
from __future__ import annotations
from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QDialog, QGridLayout, QLabel, QLineEdit, QSpinBox, QDoubleSpinBox, QPlainTextEdit,
                               QComboBox, QCheckBox, QPushButton, QDialogButtonBox, QColorDialog, QVBoxLayout,
                               QHBoxLayout, QWidget)

from ..core.ass import parse_color, format_color


class ColorButton(QPushButton):
    def __init__(self, text: str, alpha: bool):
        super().__init__()
        self.alpha = alpha
        r, g, b, a = parse_color(text or "&H00FFFFFF")
        self.col = (r, g, b, a)
        self.clicked.connect(self.pick)
        self._paint()

    def _paint(self):
        r, g, b, a = self.col
        self.setStyleSheet(f"background: rgb({r},{g},{b}); min-width:56px; min-height:22px; border:1px solid #888")

    def pick(self):
        r, g, b, a = self.col
        c = QColorDialog.getColor(QColor(r, g, b), self, "Renk")
        if c.isValid():
            self.col = (c.red(), c.green(), c.blue(), a); self._paint()

    def value(self) -> str:
        r, g, b, a = self.col
        return format_color(r, g, b, a) if self.alpha else "&H%02X%02X%02X&" % (b, g, r)


def run_lua_dialog(parent, controls: list, buttons: list, ids: dict, title: str = "Otomasyon"):
    dlg = QDialog(parent)
    dlg.setWindowTitle(title)
    grid = QGridLayout()
    widgets: dict[str, tuple[str, object]] = {}
    for c in controls:
        cls = c.get("class")
        x, y = int(c.get("x", 0)), int(c.get("y", 0))
        w, h = int(c.get("width", 1)), int(c.get("height", 1))
        name = c.get("name")
        hint = c.get("hint")
        wd = None
        if cls == "label":
            wd = QLabel(str(c.get("label", ""))); wd.setWordWrap(True)
        elif cls == "edit":
            wd = QLineEdit(str(c.get("text", c.get("value", ""))))
        elif cls == "textbox":
            wd = QPlainTextEdit(str(c.get("text", c.get("value", "")))); wd.setMinimumHeight(24 * max(2, h))
        elif cls == "intedit":
            wd = QSpinBox(); wd.setRange(int(c.get("min", -2**31)), int(c.get("max", 2**31 - 1)))
            wd.setValue(int(c.get("value", 0)))
        elif cls == "floatedit":
            wd = QDoubleSpinBox(); wd.setDecimals(4)
            wd.setRange(float(c.get("min", -1e9)), float(c.get("max", 1e9)))
            if c.get("step"): wd.setSingleStep(float(c["step"]))
            wd.setValue(float(c.get("value", 0)))
        elif cls == "dropdown":
            wd = QComboBox(); items = c.get("items") or []
            if isinstance(items, dict): items = list(items.values())
            wd.addItems([str(i) for i in items])
            v = c.get("value")
            if v is not None:
                i = wd.findText(str(v))
                if i >= 0: wd.setCurrentIndex(i)
        elif cls == "checkbox":
            wd = QCheckBox(str(c.get("label", ""))); wd.setChecked(bool(c.get("value", False)))
        elif cls in ("color", "coloralpha", "alpha"):
            wd = ColorButton(str(c.get("value", "&H00FFFFFF")), cls != "color")
        else:
            wd = QLabel(f"[{cls}?]")
        if hint: wd.setToolTip(str(hint))
        grid.addWidget(wd, y, x, h, w)
        if name:
            widgets[name] = (cls, wd)

    res_btn = {"v": None}
    labels = list(buttons) if buttons else ["OK", "Cancel"]
    default_buttons = not buttons
    ok_label = ids.get("ok") if ids else None
    cancel_label = ids.get("cancel") if ids else None
    ok_label = ok_label or labels[0]
    cancel_label = cancel_label or labels[-1]
    row = QHBoxLayout(); row.addStretch(1)
    for lab in labels:
        b = QPushButton(str(lab))
        if lab == ok_label: b.setDefault(True)
        def click(_=False, l=lab):
            res_btn["v"] = l; dlg.accept() if l != cancel_label else dlg.reject()
        b.clicked.connect(click)
        row.addWidget(b)
    lay = QVBoxLayout(dlg); lay.addLayout(grid); lay.addLayout(row)
    dlg.exec()
    clicked = res_btn["v"]
    cancelled = clicked is None or clicked == cancel_label
    if cancelled:
        return (False if default_buttons else (cancel_label if clicked is None else clicked)), {}
    out = {}
    for name, (cls, wd) in widgets.items():
        if cls in ("edit",): out[name] = wd.text()
        elif cls == "textbox": out[name] = wd.toPlainText()
        elif cls == "intedit": out[name] = wd.value()
        elif cls == "floatedit": out[name] = wd.value()
        elif cls == "dropdown": out[name] = wd.currentText()
        elif cls == "checkbox": out[name] = wd.isChecked()
        elif cls in ("color", "coloralpha", "alpha"): out[name] = wd.value()
    return (True if default_buttons else clicked), out
