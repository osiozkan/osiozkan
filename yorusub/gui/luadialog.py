"""aegisub.dialog.display için Qt uygulaması."""
from PySide6.QtWidgets import (QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox, QGridLayout, QLabel,
                               QLineEdit, QPlainTextEdit, QPushButton, QSpinBox, QColorDialog)


def show_dialog(parent, items: list[dict], buttons: list[str], title="Otomasyon"):
    dlg = QDialog(parent)
    dlg.setWindowTitle(title)
    grid = QGridLayout(dlg)
    getters = {}
    for it in items:
        cls = it.get("class")
        x, y = int(it.get("x", 0)), int(it.get("y", 0))
        w, h = int(it.get("width", 1)), int(it.get("height", 1))
        val = it.get("value", it.get("text", ""))
        if cls == "label":
            wid = QLabel(str(it.get("label", it.get("text", ""))))
        elif cls == "edit":
            wid = QLineEdit(str(val or "")); getters[it["name"]] = wid.text
        elif cls == "textbox":
            wid = QPlainTextEdit(str(val or "")); getters[it["name"]] = wid.toPlainText
        elif cls in ("intedit", "slider"):
            wid = QSpinBox(); wid.setRange(int(it.get("min", -2**31)), int(it.get("max", 2**31 - 1)))
            wid.setValue(int(val or 0)); getters[it["name"]] = wid.value
        elif cls == "floatedit":
            wid = QDoubleSpinBox(); wid.setDecimals(3); wid.setRange(float(it.get("min", -1e9)), float(it.get("max", 1e9)))
            wid.setValue(float(val or 0)); getters[it["name"]] = wid.value
        elif cls == "checkbox":
            wid = QCheckBox(str(it.get("label", ""))); wid.setChecked(bool(val)); getters[it["name"]] = wid.isChecked
        elif cls == "dropdown":
            wid = QComboBox(); wid.addItems([str(v) for v in (it.get("items") or {}).values()])
            wid.setCurrentText(str(val or "")); getters[it["name"]] = wid.currentText
        elif cls in ("color", "coloralpha", "alpha"):
            wid = QPushButton(str(val or "&H000000&")); state = {"v": str(val or "&H000000&")}

            def pick(_=False, b=wid, s=state):
                c = QColorDialog.getColor(parent=dlg)
                if c.isValid():
                    s["v"] = "&H%02X%02X%02X&" % (c.blue(), c.green(), c.red()); b.setText(s["v"])
            wid.clicked.connect(pick); getters[it["name"]] = lambda s=state: s["v"]
        else:
            wid = QLabel(str(val or ""))
        grid.addWidget(wid, y, x, h, w)
    bb = QDialogButtonBox()
    chosen = {}
    for name in buttons:
        b = bb.addButton(str(name), QDialogButtonBox.ActionRole)
        b.clicked.connect(lambda _=False, n=name: (chosen.setdefault("b", n), dlg.accept()))
    grid.addWidget(bb, 999, 0, 1, 12)
    if not dlg.exec() or "b" not in chosen:
        return None
    return chosen["b"], {k: g() for k, g in getters.items()}
