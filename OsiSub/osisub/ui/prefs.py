"""Ayarlar penceresi (Ctrl+,): config.json'u elle düzenlemeden tüm ayarlar + kısayol düzenleyici +
harici araç düzenleyici. Tamam'da kaydedilir; tema, yazı boyları ve kısayollar hemen uygulanır."""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QKeySequence
from PySide6.QtWidgets import (QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox, QFormLayout,
                               QHBoxLayout, QHeaderView, QKeySequenceEdit, QLabel, QLineEdit, QPushButton,
                               QSpinBox, QTableWidget, QTableWidgetItem, QTabWidget, QVBoxLayout, QWidget)

from . import theme
from .palette import key_text


class KeyEdit(QKeySequenceEdit):
    """QKeySequenceEdit; kısayolu Qt'nin Türkçe adlarıyla değil Ctrl+Shift+N biçiminde gösterir."""

    def __init__(self, seq=None, parent=None):
        super().__init__(parent)
        from PySide6.QtWidgets import QLineEdit as _LE
        self._le = self.findChild(_LE)
        self.keySequenceChanged.connect(self._show)
        if seq is not None:
            self.setKeySequence(seq)

    def setKeySequence(self, seq):
        super().setKeySequence(seq)
        self._show()

    def _show(self, *_):
        if self._le is not None and not self.keySequence().isEmpty():
            self._le.setText(key_text(self.keySequence()))


def _spin(lo, hi, val, suffix="", step=1):
    sb = QSpinBox()
    sb.setRange(lo, hi)
    sb.setSingleStep(step)
    sb.setValue(int(val))
    if suffix:
        sb.setSuffix(suffix)
    return sb


class PrefsDialog(QDialog):
    def __init__(self, win):
        super().__init__(win)
        self.win = win
        self.cfg = win.ctx.config
        self.setWindowTitle("Ayarlar")
        self.resize(760, 560)
        c = self.cfg
        self.tabs = QTabWidget()

        # ---- Genel
        g = QFormLayout()
        self.theme = QComboBox()
        self.theme.addItem("Koyu", "dark")
        self.theme.addItem("Açık", "light")
        self.theme.setCurrentIndex(0 if c["theme"] != "light" else 1)
        self.autosave = _spin(0, 3600, c["autosave_seconds"] or 0, " sn", 10)
        self.autosave.setToolTip("0 = kapalı")
        self.auto_media = QCheckBox(".ass açılınca kayıtlı video/sesi aç")
        self.auto_media.setChecked(bool(c["auto_load_media"]))
        self.follow = QCheckBox("Satır değişince videoyu satırın başına götür")
        self.follow.setChecked(bool(c["video_seek_on_line"]))
        self.aeg = QCheckBox("Kurulu Aegisub'ın Automation betiklerini de yükle")
        self.aeg.setChecked(bool(c["load_aegisub_scripts"]))
        self.spell = QCheckBox("Yazım denetimi (Türkçe sözlük)")
        self.spell.setChecked(bool(c["spell_enabled"]))
        g.addRow("Tema", self.theme)
        g.addRow("Otomatik yedek aralığı", self.autosave)
        for w in (self.auto_media, self.follow, self.aeg, self.spell):
            g.addRow("", w)
        self.tabs.addTab(self._page(g), "Genel")

        # ---- Düzenleyici
        e = QFormLayout()
        self.eb_font = _spin(7, 40, c["editbox_font_size"] or 13, " pt")
        self.grid_font = _spin(6, 30, c["grid_font_size"] or 10, " pt")
        self.enter_next = QCheckBox("Enter onaylayıp sonraki satıra geçsin")
        self.enter_next.setChecked(bool(c["enter_moves_next"]))
        self.marker = QLineEdit(c["grid_tag_marker"] or "☀")
        self.marker.setMaxLength(3)
        self.marker.setToolTip("Izgarada {…} etiket bloklarının yerine gösterilecek simge")
        e.addRow("Düzenleme kutusu yazı boyu", self.eb_font)
        e.addRow("Izgara yazı boyu", self.grid_font)
        e.addRow("", self.enter_next)
        e.addRow("Etiket simgesi", self.marker)
        self.tabs.addTab(self._page(e), "Düzenleyici")

        # ---- Denetim
        q = QFormLayout()
        self.cpl = _spin(20, 100, c["cpl_limit"] or 42, " karakter")
        self.cpl.setToolTip("Osi kuralı: satır başına en çok karakter (43+ uyarı; \\N'li satırda aşan parça hata)")
        self.cps = QDoubleSpinBox()
        self.cps.setRange(5, 60)
        self.cps.setDecimals(1)
        self.cps.setValue(float(c["qc_cps_max"] or 25))
        self.cps.setSuffix(" kr/sn")
        self.min_dur = _spin(0, 10000, c["qc_min_duration"] or 700, " ms", 50)
        self.max_dur = _spin(500, 60000, c["qc_max_duration"] or 7000, " ms", 500)
        self.ellipsis = QComboBox()
        self.ellipsis.addItem("... (üç nokta)", "...")
        self.ellipsis.addItem("… (tek karakter)", "…")
        self.ellipsis.setCurrentIndex(0 if (c["qc_ellipsis"] or "...") == "..." else 1)
        q.addRow("CPL sınırı", self.cpl)
        q.addRow("CPS sınırı", self.cps)
        q.addRow("En kısa süre", self.min_dur)
        q.addRow("En uzun süre", self.max_dur)
        q.addRow("Üç nokta biçimi", self.ellipsis)
        self.tabs.addTab(self._page(q), "Denetim")

        # ---- Ses / video / dışa aktarma
        m = QFormLayout()
        self.rate = QComboBox()
        for r in (16000, 22050, 32000, 44100, 48000):
            self.rate.addItem(f"{r} Hz", r)
        self.rate.setCurrentIndex(max(0, self.rate.findData(int(c["audio_rate"] or 22050))))
        self.scene = QDoubleSpinBox()
        self.scene.setRange(1, 100)
        self.scene.setValue(float(c["scene_threshold"] or 10))
        self.scene.setToolTip("ffmpeg scdet eşiği: düşük = daha çok sahne kesmesi")
        self.ttml_time = QComboBox()
        self.ttml_time.addItem("Saat (00:00:01.000)", "clock")
        self.ttml_time.addItem("Tik (10 MHz, Netflix)", "ticks")
        self.ttml_time.setCurrentIndex(0 if (c["ttml_time"] or "clock") == "clock" else 1)
        self.lang = QLineEdit(c["export_lang"] or "tr")
        m.addRow("Ses bölmesi örnekleme hızı", self.rate)
        m.addRow("Sahne değişimi eşiği", self.scene)
        m.addRow("TTML/DFXP zaman biçimi", self.ttml_time)
        m.addRow("TTML dil kodu (xml:lang)", self.lang)
        self.tabs.addTab(self._page(m), "Ses / video / dışa aktarma")

        # ---- Kısayollar
        self.tabs.addTab(self._shortcuts_page(), "Kısayollar")
        # ---- Harici araçlar
        self.tabs.addTab(self._tools_page(), "Harici araçlar")

        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addWidget(self.tabs, 1)
        lay.addWidget(bb)

    @staticmethod
    def _page(form):
        w = QWidget()
        w.setLayout(form)
        return w

    # ---------------------------------------------------------- kısayollar
    def _shortcuts_page(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        self.sc_filter = QLineEdit()
        self.sc_filter.setPlaceholderText("Eylem ara…")
        self.sc_table = QTableWidget(0, 3)
        self.sc_table.setHorizontalHeaderLabels(["Eylem", "Kısayol", "Varsayılan"])
        self.sc_table.horizontalHeader().setSectionResizeMode(0, QHeaderView.Stretch)
        self.sc_table.setColumnWidth(1, 180)
        self.sc_table.setColumnWidth(2, 130)
        self.sc_table.verticalHeader().setVisible(False)
        self.sc_edits = {}
        defaults = self.win.default_shortcuts
        for key, a in sorted(self.win.acts.items(), key=lambda kv: kv[1].text().replace("&", "").casefold()):
            r = self.sc_table.rowCount()
            self.sc_table.insertRow(r)
            it = QTableWidgetItem(a.text().replace("&", ""))
            it.setFlags(it.flags() & ~Qt.ItemIsEditable)
            it.setData(Qt.UserRole, key)
            self.sc_table.setItem(r, 0, it)
            ed = KeyEdit(a.shortcut())
            ed.keySequenceChanged.connect(lambda *_: self._check_conflicts())
            self.sc_table.setCellWidget(r, 1, ed)
            d = QTableWidgetItem(key_text(defaults.get(key, QKeySequence())))
            d.setFlags(d.flags() & ~Qt.ItemIsEditable)
            self.sc_table.setItem(r, 2, d)
            self.sc_edits[key] = ed
        self.sc_info = QLabel("Kısayolu silmek için kutuya tıklayıp Geri tuşuna basın. Ctrl+Alt+rakam/harf "
                              "birleşimlerinden kaçının: Türkçe klavyede AltGr (@ € { } [ ] \\ | ₺) ile çakışır.")
        self.sc_info.setWordWrap(True)
        b_reset = QPushButton("Tümünü varsayılana döndür")
        b_reset.clicked.connect(self.reset_shortcuts)
        self.sc_filter.textChanged.connect(self._filter_shortcuts)
        lay.addWidget(self.sc_filter)
        lay.addWidget(self.sc_table, 1)
        h = QHBoxLayout()
        h.addWidget(self.sc_info, 1)
        h.addWidget(b_reset)
        lay.addLayout(h)
        return w

    def _filter_shortcuts(self, text):
        from .palette import fold
        f = fold(text)
        for r in range(self.sc_table.rowCount()):
            self.sc_table.setRowHidden(r, bool(f) and f not in fold(self.sc_table.item(r, 0).text()))

    def shortcut_map(self) -> dict[str, str]:
        return {k: ed.keySequence().toString(QKeySequence.PortableText) for k, ed in self.sc_edits.items()}

    def conflicts(self) -> dict[str, list[str]]:
        seen: dict[str, list[str]] = {}
        for k, sc in self.shortcut_map().items():
            if sc:
                seen.setdefault(sc, []).append(k)
        return {sc: ks for sc, ks in seen.items() if len(ks) > 1}

    def _check_conflicts(self):
        bad = {k for ks in self.conflicts().values() for k in ks}
        c = theme.colors()
        for r in range(self.sc_table.rowCount()):
            key = self.sc_table.item(r, 0).data(Qt.UserRole)
            self.sc_table.item(r, 0).setForeground(QColor(c["err_fg"] if key in bad else c["text"]))
        return bad

    def reset_shortcuts(self):
        for k, ed in self.sc_edits.items():
            ed.setKeySequence(self.win.default_shortcuts.get(k, QKeySequence()))
        self._check_conflicts()

    # ---------------------------------------------------------- harici araçlar
    def _tools_page(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        self.tools = QTableWidget(0, 4)
        self.tools.setHorizontalHeaderLabels(["Ad", "Program", "Argümanlar", "Yeniden yükle"])
        self.tools.horizontalHeader().setSectionResizeMode(2, QHeaderView.Stretch)
        self.tools.setColumnWidth(0, 160)
        self.tools.setColumnWidth(1, 140)
        self.tools.verticalHeader().setVisible(False)
        for t in self.cfg["external_tools"] or []:
            self._add_tool_row(t)
        hint = QLabel("Yer tutucular: {ass} {dir} {video} {audio} {sel} {active} {start_ms} {end_ms} "
                      "{start_frame} {end_frame}. Araç Araçlar menüsünde görünür; .ass'ı değiştirirse yeniden yüklenir.")
        hint.setWordWrap(True)
        b_add = QPushButton("Araç ekle")
        b_add.clicked.connect(lambda: self._add_tool_row({"name": "Yeni araç", "program": "python", "args": "",
                                                          "reload": True}))
        b_del = QPushButton("Seçiliyi sil")
        b_del.clicked.connect(lambda: self.tools.removeRow(self.tools.currentRow()))
        h = QHBoxLayout()
        h.addWidget(b_add)
        h.addWidget(b_del)
        h.addStretch(1)
        lay.addWidget(self.tools, 1)
        lay.addWidget(hint)
        lay.addLayout(h)
        return w

    def _add_tool_row(self, t: dict):
        r = self.tools.rowCount()
        self.tools.insertRow(r)
        for col, key in enumerate(("name", "program", "args")):
            self.tools.setItem(r, col, QTableWidgetItem(str(t.get(key, ""))))
        it = QTableWidgetItem()
        it.setFlags(Qt.ItemIsUserCheckable | Qt.ItemIsEnabled | Qt.ItemIsSelectable)
        it.setCheckState(Qt.Checked if t.get("reload", True) else Qt.Unchecked)
        self.tools.setItem(r, 3, it)
        it.setData(Qt.UserRole, {k: v for k, v in t.items() if k not in ("name", "program", "args", "reload")})

    def tool_list(self) -> list[dict]:
        out = []
        for r in range(self.tools.rowCount()):
            name = self.tools.item(r, 0).text().strip() if self.tools.item(r, 0) else ""
            prog = self.tools.item(r, 1).text().strip() if self.tools.item(r, 1) else ""
            if not name or not prog:
                continue
            extra = self.tools.item(r, 3).data(Qt.UserRole) or {}
            out.append({**extra, "name": name, "program": prog,
                        "args": self.tools.item(r, 2).text() if self.tools.item(r, 2) else "",
                        "reload": self.tools.item(r, 3).checkState() == Qt.Checked})
        return out

    # ---------------------------------------------------------- uygula
    def apply(self):
        if self.conflicts():
            self.tabs.setCurrentIndex(self.tabs.count() - 2)
            self.sc_info.setText("Çakışan kısayollar var (kırmızı). Düzeltip yeniden deneyin.")
            self._check_conflicts()
            return False
        c = self.cfg
        old_theme = c["theme"]
        c["theme"] = self.theme.currentData()
        c["autosave_seconds"] = self.autosave.value()
        c["auto_load_media"] = self.auto_media.isChecked()
        c["video_seek_on_line"] = self.follow.isChecked()
        c["load_aegisub_scripts"] = self.aeg.isChecked()
        c["spell_enabled"] = self.spell.isChecked()
        c["editbox_font_size"] = self.eb_font.value()
        c["grid_font_size"] = self.grid_font.value()
        c["enter_moves_next"] = self.enter_next.isChecked()
        c["grid_tag_marker"] = self.marker.text() or "☀"
        c["cpl_limit"] = self.cpl.value()
        c["qc_cps_max"] = self.cps.value()
        c["qc_min_duration"] = self.min_dur.value()
        c["qc_max_duration"] = self.max_dur.value()
        c["qc_ellipsis"] = self.ellipsis.currentData()
        c["audio_rate"] = self.rate.currentData()
        c["scene_threshold"] = self.scene.value()
        c["ttml_time"] = self.ttml_time.currentData()
        c["export_lang"] = self.lang.text().strip() or "tr"
        c["external_tools"] = self.tool_list()
        defaults = self.win.default_shortcuts
        overrides = {}
        for k, sc in self.shortcut_map().items():
            d = defaults.get(k, QKeySequence()).toString(QKeySequence.PortableText)
            if sc != d:
                overrides[k] = sc
        c["shortcuts"] = overrides
        c.save()
        self.win.apply_prefs(theme_changed=c["theme"] != old_theme)
        self.accept()
        return True
