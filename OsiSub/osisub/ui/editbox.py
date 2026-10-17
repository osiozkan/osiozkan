"""Düzenleme kutusu (Aegisub SubsEditBox karşılığı).

Metin YALNIZ etkin satıra uygulanır; stil/aktör/efekt/katman/kenar/zaman/yorum
alanları Aegisub'daki gibi TÜM seçili satırlara uygulanır.
Yazarken her tuş belgeye anında işler; aynı satırdaki ardışık yazım tek
geri al adımında birleşir.

CPL kutuları (Osi): "Toplam" = altyazının tüm görünür karakterleri,
"En uzun" = en uzun satırın karakteri; yanında parça gösterimi "43+16".
"""

from __future__ import annotations

from PySide6.QtCore import Qt, QTimer, Signal
from PySide6.QtGui import QFont, QTextCursor
from PySide6.QtWidgets import (QCheckBox, QComboBox, QHBoxLayout, QLabel, QLineEdit, QSizePolicy,
                               QSpinBox, QToolButton, QVBoxLayout, QWidget)

from ..core.textutil import STATUS_ERROR, STATUS_WARN, cpl_info
from . import theme
from .widgets import SubTextEdit, TimeEdit


class EditBox(QWidget):
    moved_next = Signal()

    def __init__(self, ctx):
        super().__init__()
        self.ctx = ctx
        self._loading = False
        self._row = -1

        # ---- satır 1
        self.comment = QCheckBox("Yorum")
        self.style_box = QComboBox()
        self.style_box.setMinimumWidth(140)
        self.style_box.setToolTip("Stil")
        self.actor = QComboBox()
        self.actor.setEditable(True)
        self.actor.setMinimumWidth(110)
        self.actor.lineEdit().setPlaceholderText("Aktör")
        self.effect = QLineEdit()
        self.effect.setPlaceholderText("Efekt")
        self.layer = QSpinBox()
        self.layer.setRange(0, 999)
        self.layer.setToolTip("Katman (Layer)")
        self.layer.setPrefix("K ")

        r1 = QHBoxLayout()
        r1.setSpacing(6)
        r1.addWidget(self.comment)
        r1.addWidget(self.style_box, 2)
        r1.addWidget(self.actor, 2)
        r1.addWidget(self.effect, 2)
        r1.addWidget(self.layer)

        # ---- satır 2
        self.start = TimeEdit()
        self.start.setToolTip("Başlangıç")
        self.end = TimeEdit()
        self.end.setToolTip("Bitiş")
        self.dur = TimeEdit()
        self.dur.setToolTip("Süre")
        self.ml, self.mr, self.mv = QSpinBox(), QSpinBox(), QSpinBox()
        for sb, tip, pre in ((self.ml, "Sol kenar", "Sol "), (self.mr, "Sağ kenar", "Sağ "),
                             (self.mv, "Dikey kenar", "Dik ")):
            sb.setRange(0, 9999)
            sb.setPrefix(pre)
            sb.setToolTip(tip + " (0 = stildeki değer)")
            sb.setFixedWidth(88)

        self.fmt_buttons = []
        r2 = QHBoxLayout()
        r2.setSpacing(6)
        r2.addWidget(self.start)
        r2.addWidget(self.end)
        r2.addWidget(self.dur)
        r2.addSpacing(8)
        for sb in (self.ml, self.mr, self.mv):
            r2.addWidget(sb)
        r2.addSpacing(8)
        for label, tag, tip in (("B", "b", "Kalın"), ("I", "i", "İtalik"),
                                ("U", "u", "Altı çizili"), ("S", "s", "Üstü çizili")):
            b = QToolButton()
            b.setText(label)
            b.setToolTip(f"{tip}: seçili metni {{\\{tag}1}}…{{\\{tag}0}} ile sar")
            f = b.font()
            f.setBold(label == "B")
            f.setItalic(label == "I")
            f.setUnderline(label == "U")
            f.setStrikeOut(label == "S")
            b.setFont(f)
            b.clicked.connect(lambda _=False, t=tag: self._wrap_tag(t))
            r2.addWidget(b)
            self.fmt_buttons.append(b)
        r2.addStretch(1)
        self.cpl_total = QLabel("0")
        self.cpl_total.setObjectName("cplBox")
        self.cpl_total.setToolTip("Toplam: altyazının tüm görünür karakterleri (boşluk ve noktalama dahil)")
        self.cpl_long = QLabel("0")
        self.cpl_long.setObjectName("cplBox")
        self.cpl_long.setToolTip("En uzun satırın karakter sayısı (42 sınır)")
        self.cpl_parts = QLabel("")
        self.cpl_parts.setToolTip("Satır uzunlukları (\\N ile bölünmüş parçalar)")
        r2.addWidget(QLabel("Toplam"))
        r2.addWidget(self.cpl_total)
        r2.addWidget(QLabel("En uzun"))
        r2.addWidget(self.cpl_long)
        r2.addWidget(self.cpl_parts)

        # ---- metin
        self.text = SubTextEdit()
        f = QFont(self.text.font())
        f.setPointSize(int(ctx.config["editbox_font_size"] or 13))
        self.text.setFont(f)
        self.text.setPlaceholderText("Altyazı metni — Enter: sonraki satır · Shift+Enter: \\N · Ctrl+Enter: onayla")
        self.text.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)

        lay = QVBoxLayout(self)
        lay.setContentsMargins(6, 6, 6, 6)
        lay.setSpacing(6)
        lay.addLayout(r1)
        lay.addLayout(r2)
        # ---- referans (çeviri modu)
        self.ref_row = QWidget()
        rr = QHBoxLayout(self.ref_row)
        rr.setContentsMargins(0, 0, 0, 0)
        self.ref_label = QLabel("")
        self.ref_label.setWordWrap(True)
        self.ref_label.setTextInteractionFlags(Qt.TextSelectableByMouse)
        self.ref_label.setStyleSheet(f"color: {theme.colors()['muted']}; font-style: italic;")
        self.ref_copy = QToolButton()
        self.ref_copy.setText("⤵ Kopyala")
        self.ref_copy.setToolTip("Referans metnini bu satıra yaz (Ctrl+Alt+R)")
        self.ref_copy.clicked.connect(self.copy_reference)
        rr.addWidget(QLabel("Referans:"))
        rr.addWidget(self.ref_label, 1)
        rr.addWidget(self.ref_copy)
        self.ref_row.setVisible(False)
        lay.addWidget(self.ref_row)
        lay.addWidget(self.text, 1)

        # ---- sinyaller
        self.text.textChanged.connect(self._text_changed)
        self.text.commit_next.connect(self._commit_next)
        self.text.commit_stay.connect(lambda: self.ctx.undo.break_coalesce())
        self.comment.toggled.connect(lambda v: self._set_sel("comment", bool(v), "Yorum"))
        self.style_box.activated.connect(lambda _i: self._set_sel("style", self.style_box.currentText(), "Stil"))
        self.actor.lineEdit().editingFinished.connect(
            lambda: self._set_sel("actor", self.actor.currentText(), "Aktör"))
        self.actor.activated.connect(lambda _i: self._set_sel("actor", self.actor.currentText(), "Aktör"))
        self.effect.editingFinished.connect(lambda: self._set_sel("effect", self.effect.text(), "Efekt"))
        self.layer.valueChanged.connect(lambda v: self._set_sel("layer", int(v), "Katman", "layer"))
        self.ml.valueChanged.connect(lambda v: self._set_sel("margin_l", int(v), "Sol kenar", "ml"))
        self.mr.valueChanged.connect(lambda v: self._set_sel("margin_r", int(v), "Sağ kenar", "mr"))
        self.mv.valueChanged.connect(lambda v: self._set_sel("margin_v", int(v), "Dikey kenar", "mv"))
        self.start.committed.connect(self._start_changed)
        self.end.committed.connect(self._end_changed)
        self.dur.committed.connect(self._dur_changed)

        ctx.active_changed.connect(lambda _r: self.load())
        ctx.document_reset.connect(self._on_reset)
        ctx.rows_changed.connect(self._rows_changed)
        ctx.styles_changed.connect(self._fill_styles)
        self._on_reset()

    # ---------------------------------------------------------- yükleme
    def _on_reset(self):
        self._fill_styles()
        self._fill_actors()
        self.load()

    def _fill_styles(self):
        self._loading = True
        cur = self.style_box.currentText()
        self.style_box.clear()
        self.style_box.addItems([s.name for s in self.ctx.doc.styles])
        i = self.style_box.findText(cur)
        if i >= 0:
            self.style_box.setCurrentIndex(i)
        self._loading = False
        self.load()

    def _fill_actors(self):
        self._loading = True
        cur = self.actor.currentText()
        actors = sorted({e.actor for e in self.ctx.doc.events if e.actor}, key=str.lower)
        self.actor.clear()
        self.actor.addItems(actors)
        self.actor.setEditText(cur)
        self._loading = False

    def _rows_changed(self, rows):
        if self._loading:
            return
        if self.ctx.active in rows:
            self.load(keep_cursor=True)

    def load(self, keep_cursor: bool = False):
        ev = self.ctx.active_event()
        self._loading = True
        try:
            enabled = ev is not None
            self.setEnabled(enabled)
            if not enabled:
                return
            self._row = self.ctx.active
            self.comment.setChecked(ev.comment)
            i = self.style_box.findText(ev.style)
            if i < 0:
                self.style_box.addItem(ev.style)
                i = self.style_box.findText(ev.style)
            self.style_box.setCurrentIndex(i)
            self.actor.setEditText(ev.actor)
            self.effect.setText(ev.effect)
            self.layer.setValue(ev.layer)
            self.start.setValue(ev.start)
            self.end.setValue(ev.end)
            self.dur.setValue(max(0, ev.end - ev.start))
            self.ml.setValue(ev.margin_l)
            self.mr.setValue(ev.margin_r)
            self.mv.setValue(ev.margin_v)
            if self.text.toPlainText() != ev.text:
                cur = self.text.textCursor().position()
                self.text.setPlainText(ev.text)
                c = self.text.textCursor()
                c.setPosition(min(cur, len(ev.text)) if keep_cursor else len(ev.text))
                self.text.setTextCursor(c)
            self._update_cpl(ev.text)
            self._update_ref(ev)
        finally:
            self._loading = False

    # ---------------------------------------------------------- CPL
    def _update_cpl(self, text):
        info = cpl_info(text)
        c = theme.colors()
        st = info["status"]
        self.cpl_total.setText(str(info["total"]))
        self.cpl_long.setText(str(info["longest"]))
        self.cpl_parts.setText(info["label"] if len(info["lengths"]) > 1 else "")
        if st == STATUS_ERROR:
            css = f"background:{c['err_bg']}; color:{c['err_fg']};"
        elif st == STATUS_WARN:
            css = f"background:{c['warn_bg']}; color:{c['warn_fg']};"
        else:
            css = f"color:{c['ok_fg']};"
        self.cpl_long.setStyleSheet(css)

    # ---------------------------------------------------------- yazma
    def _text_changed(self):
        txt = self.text.toPlainText()
        if "\n" in txt:     # Shift+Enter dışı yollarla gelen satır sonları
            c = self.text.textCursor().position()
            self._loading = True
            self.text.setPlainText(txt.replace("\n", "\\N"))
            cur = self.text.textCursor()
            cur.setPosition(min(c + 1, len(self.text.toPlainText())))
            self.text.setTextCursor(cur)
            self._loading = False
            txt = self.text.toPlainText()
        self._update_cpl(txt)
        if self._loading or self._row < 0:
            return
        self._loading = True
        try:
            self.ctx.update_events({self._row: {"text": txt}}, "Metin düzenleme",
                                   coalesce=f"text:{self._row}")
        finally:
            self._loading = False

    def _set_sel(self, field, value, desc, coalesce=None):
        if self._loading:
            return
        self._loading = True
        try:
            self.ctx.set_field_for_selection(field, value, desc,
                                             coalesce=f"{coalesce}:{self._row}" if coalesce else None)
        finally:
            self._loading = False
        if field == "actor":
            self._fill_actors()

    def _start_changed(self, ms):
        if self._loading:
            return
        ch = {}
        for r in self.ctx.selected_rows():
            ev = self.ctx.doc.events[r]
            ch[r] = {"start": ms, "end": max(ev.end, ms)}
        self.ctx.update_events(ch, "Başlangıç zamanı")
        self.load()

    def _end_changed(self, ms):
        if self._loading:
            return
        ch = {}
        for r in self.ctx.selected_rows():
            ev = self.ctx.doc.events[r]
            ch[r] = {"end": ms, "start": min(ev.start, ms)}
        self.ctx.update_events(ch, "Bitiş zamanı")
        self.load()

    def _dur_changed(self, ms):
        if self._loading:
            return
        ch = {r: {"end": self.ctx.doc.events[r].start + ms} for r in self.ctx.selected_rows()}
        self.ctx.update_events(ch, "Süre")
        self.load()

    def _commit_next(self):
        self.ctx.undo.break_coalesce()
        n = len(self.ctx.doc.events)
        row = self.ctx.active
        if row + 1 >= n:
            # Son satırda Enter: Aegisub gibi arkasına yeni satır aç
            ev = self.ctx.doc.events[row]
            new = ev.fresh().replace(text="", start=ev.end, end=ev.end + 5000, comment=False,
                                     effect="")
            self.ctx.insert_events(row + 1, [new], "Yeni satır")
        else:
            self.ctx.set_active(row + 1, select_only=True)
        self.text.setFocus()
        c = self.text.textCursor()
        c.movePosition(QTextCursor.End)
        self.text.setTextCursor(c)
        self.moved_next.emit()

    def _wrap_tag(self, tag):
        c = self.text.textCursor()
        sel = c.selectedText()
        if sel:
            c.insertText(f"{{\\{tag}1}}{sel}{{\\{tag}0}}")
        else:
            c.insertText(f"{{\\{tag}1}}")
        self.text.setFocus()

    def focus_text(self):
        self.text.setFocus()
        c = self.text.textCursor()
        c.movePosition(QTextCursor.End)
        self.text.setTextCursor(c)

    # arch1t3cht 3.4 Lua uzantıları için
    def cursor_pos(self) -> int:
        return self.text.textCursor().position() + 1

    def set_cursor_pos(self, p: int):
        c = self.text.textCursor()
        c.setPosition(max(0, min(p - 1, len(self.text.toPlainText()))))
        self.text.setTextCursor(c)

    # ---------------------------------------------------------- referans
    def _update_ref(self, ev=None):
        ref = getattr(self.ctx, "reference", None)
        on = ref is not None and ref.loaded
        self.ref_row.setVisible(on)
        if not on:
            return
        ev = ev or self.ctx.active_event()
        txt = ref.text_for(ev) if ev is not None else ""
        self.ref_label.setText(txt.replace("\\N", " ⏎ ") if txt else "(eşleşen referans satırı yok)")

    def copy_reference(self):
        ref = getattr(self.ctx, "reference", None)
        ev = self.ctx.active_event()
        if ref is None or not ref.loaded or ev is None:
            return
        txt = ref.text_for(ev)
        if txt:
            self.ctx.update_events({self.ctx.active: {"text": txt}}, "Referansı kopyala")
            self.focus_text()
