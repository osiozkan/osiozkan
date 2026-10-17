"""Kaynakla karşılaştır bölmesi (aligncompare uygulama içinde).

Kaynak = yüklü referans altyazı (EN), aday = açık belge (TR). Sonuç grupları ızgaraya
bağlıdır: çift tıkla satıra git; bayraklı satırları seç; EKSİK kaynak satırlarını
zamanca doğru yere yorum satırı olarak ekle (tek geri al). HTML/CSV rapor ve işaretli
kopya komut satırındaki aligncompare ile birebir aynı çıktıyı verir.
"""

from __future__ import annotations

import os

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QCheckBox, QComboBox, QDoubleSpinBox, QFileDialog, QFormLayout, QGroupBox,
                               QHBoxLayout, QHeaderView, QLabel, QLineEdit, QPushButton, QSpinBox,
                               QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget)

from ..core.assfile import Event
from ..core.asstime import format_time, round_ms
from ..core.compare import FLAG_HELP, FLAG_ORDER, CompareOptions, compare, marked_document, summary_lines, \
    to_csv, to_html
from . import theme

_SEVERE = {"EKSİK", "FAZLA", "BOŞ", "İNGİLİZCE?"}


class ComparePanel(QWidget):
    def __init__(self, ctx, win):
        super().__init__()
        self.ctx = ctx
        self.win = win
        self.result = None
        self.stale = False
        cfg = dict(ctx.config["compare"] or {})

        self.info = QLabel("")
        self.info.setWordWrap(True)
        b_ref = QPushButton("Kaynak aç…")
        b_ref.setToolTip("Kaynak = referans altyazı (Çeviri > Referans altyazı aç)")
        b_ref.clicked.connect(lambda: win.open_reference())
        self.b_run = QPushButton("Karşılaştır")
        self.b_run.setDefault(True)
        self.b_run.clicked.connect(self.run)
        top = QHBoxLayout()
        top.addWidget(self.info, 1)
        top.addWidget(b_ref)
        top.addWidget(self.b_run)

        # ---- seçenekler (aligncompare parametreleri)
        self.auto_shift = QCheckBox("Kaymayı otomatik bul (± aralık)")
        self.auto_shift.setChecked(cfg.get("auto", True))
        self.shift = QDoubleSpinBox()
        self.shift.setRange(-3600, 3600)
        self.shift.setDecimals(2)
        self.shift.setSuffix(" sn")
        self.shift.setValue(float(cfg.get("shift", 0.0)))
        self.shift.setToolTip("Elle kayma: aday kaynaktan bu kadar GEÇ (eksi: erken)")
        self.range = QDoubleSpinBox()
        self.range.setRange(1, 600)
        self.range.setSuffix(" sn")
        self.range.setValue(float(cfg.get("range", 15.0)))
        self.auto_shift.toggled.connect(lambda on: (self.shift.setEnabled(not on), self.range.setEnabled(on)))
        self.shift.setEnabled(not self.auto_shift.isChecked())
        self.range.setEnabled(self.auto_shift.isChecked())

        def dspin(lo, hi, val, step, dec=2):
            sb = QDoubleSpinBox()
            sb.setRange(lo, hi)
            sb.setSingleStep(step)
            sb.setDecimals(dec)
            sb.setValue(val)
            return sb
        self.ratio_min = dspin(0.05, 5, float(cfg.get("ratio_min", 0.45)), 0.05)
        self.ratio_max = dspin(0.1, 10, float(cfg.get("ratio_max", 2.2)), 0.1)
        self.sim = dspin(0.5, 1.01, float(cfg.get("sim", 0.85)), 0.01)
        self.sim.setToolTip("Aday kaynağa bu kadar benziyorsa İNGİLİZCE? (özel adlar için 0,92 gevşetir)")
        self.max_diff = dspin(0.05, 30, float(cfg.get("max_start_diff", 0.5)), 0.1)
        self.max_diff.setSuffix(" sn")
        self.min_ov = dspin(0.05, 1.0, float(cfg.get("min_overlap", 0.25)), 0.05)
        self.min_len = QSpinBox()
        self.min_len.setRange(0, 200)
        self.min_len.setValue(int(cfg.get("min_len", 10)))
        self.min_len.setToolTip("UZUNLUK yalnız kaynak bu kadar karakterden uzunsa denetlenir")
        self.typeset = QCheckBox("Typeset ve ekran yazılarını da kat")
        self.typeset.setChecked(bool(cfg.get("include_typeset", False)))
        self.apply_ref = QCheckBox("Bulunan kaymayı Referans sütununa uygula")
        self.apply_ref.setChecked(bool(cfg.get("apply_ref", True)))
        self.apply_ref.setToolTip("Referans sütunu, düzenleme kutusundaki referans ve aktarma bu kaymayla eşlenir")
        self.skip_src = QLineEdit(cfg.get("skip_src", ""))
        self.skip_src.setPlaceholderText("regex, ör. ^(OP|ED)$")
        self.skip_cand = QLineEdit(cfg.get("skip_cand", ""))
        self.skip_cand.setPlaceholderText("regex, ör. Karaoke|OP|ED")
        sh = QHBoxLayout()
        sh.addWidget(self.auto_shift)
        sh.addWidget(self.range)
        sh.addWidget(QLabel("ya da"))
        sh.addWidget(self.shift)
        rh = QHBoxLayout()
        rh.addWidget(self.ratio_min)
        rh.addWidget(QLabel("–"))
        rh.addWidget(self.ratio_max)
        form = QFormLayout()
        form.addRow("Kayma", sh)
        form.addRow("Uzunluk oranı", rh)
        form.addRow("Benzerlik eşiği", self.sim)
        form.addRow("ZAMAN eşiği", self.max_diff)
        form.addRow("En az örtüşme", self.min_ov)
        form.addRow("UZUNLUK için en kısa kaynak", self.min_len)
        form.addRow("Kaynakta atlanacak stiller", self.skip_src)
        form.addRow("Adayda atlanacak stiller", self.skip_cand)
        form.addRow("", self.typeset)
        form.addRow("", self.apply_ref)
        self.opts_box = QGroupBox("Seçenekler")
        self.opts_box.setCheckable(True)
        self.opts_box.setChecked(bool(cfg.get("show_opts", False)))
        inner = QWidget()
        inner.setLayout(form)
        ob = QVBoxLayout(self.opts_box)
        ob.setContentsMargins(6, 2, 6, 2)
        ob.addWidget(inner)
        inner.setVisible(self.opts_box.isChecked())
        self.opts_box.toggled.connect(inner.setVisible)

        # ---- süzgeç + özet
        self.filter = QComboBox()
        self.filter.addItem("Tümü", "all")
        self.filter.addItem("Yalnız bayraklı", "flagged")
        for k in FLAG_ORDER:
            self.filter.addItem(k, k)
            self.filter.setItemData(self.filter.count() - 1, FLAG_HELP[k], Qt.ToolTipRole)
        self.filter.setCurrentIndex(1)
        self.filter.currentIndexChanged.connect(self._fill)
        self.summary = QLabel("")
        self.summary.setWordWrap(True)
        self.summary.setTextInteractionFlags(Qt.TextSelectableByMouse)
        fh = QHBoxLayout()
        fh.addWidget(QLabel("Göster"))
        fh.addWidget(self.filter)
        fh.addStretch(1)

        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Tür", "Zaman", "Kaynak", "Aday", "Bayraklar"])
        self.tree.setRootIsDecorated(False)
        self.tree.setUniformRowHeights(True)
        hd = self.tree.header()
        hd.setSectionResizeMode(2, QHeaderView.Stretch)
        hd.setSectionResizeMode(3, QHeaderView.Stretch)
        self.tree.setColumnWidth(0, 40)
        self.tree.setColumnWidth(1, 82)
        self.tree.setColumnWidth(4, 160)
        self.tree.itemDoubleClicked.connect(self._go)
        self.tree.itemActivated.connect(self._go)

        b_sel = QPushButton("Bayraklıları seç")
        b_sel.clicked.connect(self.select_flagged)
        b_add = QPushButton("Eksikleri yorum olarak ekle")
        b_add.setToolTip("EKSİK kaynak satırlarını zamanca doğru yere {QC: EKSİK} yorum satırı olarak ekler (tek geri al)")
        b_add.clicked.connect(self.add_missing)
        b_html = QPushButton("HTML…")
        b_html.clicked.connect(lambda: self.export("html"))
        b_csv = QPushButton("CSV…")
        b_csv.clicked.connect(lambda: self.export("csv"))
        b_marked = QPushButton("İşaretli kopya…")
        b_marked.setToolTip("Bayraklı satırlara {QC: …} eklenmiş ayrı .ass (açık belge değişmez)")
        b_marked.clicked.connect(lambda: self.export("marked"))
        bh = QHBoxLayout()
        for b in (b_sel, b_add):
            bh.addWidget(b)
        bh.addStretch(1)
        for b in (b_html, b_csv, b_marked):
            bh.addWidget(b)

        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addLayout(top)
        lay.addWidget(self.opts_box)
        lay.addWidget(self.summary)
        lay.addLayout(fh)
        lay.addWidget(self.tree, 1)
        lay.addLayout(bh)
        ref = getattr(ctx, "reference", None)
        if ref is not None:
            ref.changed.connect(self._update_info)
        ctx.document_reset.connect(self._doc_reset)
        self._update_info()

    # ---------------------------------------------------------- durum
    def _update_info(self):
        ref = getattr(self.ctx, "reference", None)
        if ref is None or not ref.loaded:
            self.info.setText("Kaynak yok — önce referans altyazıyı (EN) aç. Aday: açık belge.")
            self.b_run.setEnabled(False)
            return
        self.b_run.setEnabled(True)
        self.info.setText(f"Kaynak: {os.path.basename(ref.path)} ({len(ref.events)} satır) ↔ "
                          f"Aday: {self.ctx.title.lstrip('● ')}")

    def _doc_reset(self):
        self._update_info()
        if self.result is not None:
            self.stale = True
            self.summary.setText(self.summary.text().split("\n⚠")[0] +
                                 "\n⚠ Belge değişti — satır numaraları kaymış olabilir; yeniden karşılaştırın.")

    def options(self) -> CompareOptions:
        return CompareOptions(
            shift="auto" if self.auto_shift.isChecked() else self.shift.value(),
            shift_range=self.range.value(), ratio_min=self.ratio_min.value(), ratio_max=self.ratio_max.value(),
            sim=self.sim.value(), max_start_diff=self.max_diff.value(), min_overlap=self.min_ov.value(),
            min_len=self.min_len.value(), skip_styles_src=self.skip_src.text().strip(),
            skip_styles_cand=self.skip_cand.text().strip(), include_typeset=self.typeset.isChecked())

    def _save_cfg(self):
        self.ctx.config["compare"] = {
            "auto": self.auto_shift.isChecked(), "shift": self.shift.value(), "range": self.range.value(),
            "ratio_min": self.ratio_min.value(), "ratio_max": self.ratio_max.value(), "sim": self.sim.value(),
            "max_start_diff": self.max_diff.value(), "min_overlap": self.min_ov.value(),
            "min_len": self.min_len.value(), "include_typeset": self.typeset.isChecked(),
            "skip_src": self.skip_src.text(), "skip_cand": self.skip_cand.text(),
            "show_opts": self.opts_box.isChecked(), "apply_ref": self.apply_ref.isChecked()}
        self.ctx.config.save()

    # ---------------------------------------------------------- çalıştır
    def run(self):
        ref = getattr(self.ctx, "reference", None)
        if ref is None or not ref.loaded:
            self.ctx.status.emit("Karşılaştırma için önce kaynak (referans) altyazıyı açın")
            return None
        import re
        for w in (self.skip_src, self.skip_cand):
            try:
                re.compile(w.text().strip())
            except re.error as e:
                self.summary.setText(f"Stil regex hatası: {e}")
                return None
        self._save_cfg()
        self.result = compare(ref.events, self.ctx.doc.events, self.options())
        self.stale = False
        res = self.result
        if self.apply_ref.isChecked() and not res.warnings and res.shift_ms != ref.offset:
            ref.set_offset(res.shift_ms)
        lines = summary_lines(self.result, os.path.basename(ref.path), self.ctx.title.lstrip("● "))
        self.summary.setText("\n".join(lines[2:]))
        self._fill()
        n = len(self.result.flagged())
        self.ctx.status.emit(f"Karşılaştırma: {len(self.result.groups)} grup · {n} bayraklı")
        return self.result

    def _fill(self, *_):
        self.tree.clear()
        res = self.result
        if res is None:
            return
        mode = self.filter.currentData()
        c = theme.colors()
        items = []
        for k, g in enumerate(res.groups):
            if mode == "flagged" and not g.flags:
                continue
            if mode not in ("all", "flagged") and mode not in g.codes:
                continue
            t0 = min([x.start for x in g.src] or [x.start - res.shift_ms for x in g.cand])
            cand = " / ".join(f"[{x.row + 1}] {x.plain or '(boş)'}" for x in g.cand)
            it = QTreeWidgetItem([g.kind, format_time(max(0, t0)), g.src_text(), cand,
                                  " · ".join(f.label() for f in g.flags)])
            it.setData(0, Qt.UserRole, k)
            it.setToolTip(2, g.src_text("\n"))
            it.setToolTip(3, g.cand_text("\n"))
            it.setToolTip(4, "\n".join(f"{f.code}: {FLAG_HELP.get(f.code, '')} {f.detail}" for f in g.flags))
            if g.flags:
                sev = any(f.code in _SEVERE for f in g.flags)
                it.setForeground(4, QColor(c["err_fg"] if sev else c["warn_fg"]))
            items.append(it)
        self.tree.addTopLevelItems(items)

    # ---------------------------------------------------------- satıra git / seçim
    def group(self, item):
        k = item.data(0, Qt.UserRole)
        return None if (k is None or self.result is None) else self.result.groups[int(k)]

    def _go(self, item, _col=0):
        g = self.group(item)
        if g is None:
            return
        n = len(self.ctx.doc.events)
        rows = [x.row for x in g.cand if x.row < n]
        if rows:
            self.ctx.set_selection(rows, rows[0])
            return
        t = g.src[0].start + self.result.shift_ms
        best = min(range(n), key=lambda i: abs(self.ctx.doc.events[i].start - t), default=None)
        if best is not None:
            self.ctx.set_active(best, select_only=True)
        media = getattr(self.ctx, "media", None)
        if media is not None and media.index is not None:
            self.win.video.seek_ms(t)

    def select_flagged(self):
        if self.result is None:
            return []
        n = len(self.ctx.doc.events)
        rows = sorted(r for r in self.result.flags_by_cand_row() if r < n)
        if rows:
            self.ctx.set_selection(rows, rows[0])
        self.ctx.status.emit(f"{len(rows)} bayraklı satır seçildi")
        return rows

    def add_missing(self):
        """EKSİK kaynak satırlarını adayın zaman ekseninde yorum satırı olarak ekler."""
        res = self.result
        if res is None or self.stale:
            res = self.run()
        if res is None:
            return 0
        evs = list(self.ctx.doc.events)
        have = {(e.start, e.text) for e in evs if e.comment}
        miss = [s for g in res.groups if g.src and not g.cand for s in g.src
                if (round_ms(s.start + res.shift_ms), "{QC: EKSİK}" + s.text) not in have]
        if not miss:
            self.ctx.status.emit("Eklenecek eksik kaynak satırı yok")
            return 0
        styles: dict[str, int] = {}
        for e in evs:
            if not e.comment:
                styles[e.style] = styles.get(e.style, 0) + 1
        style = max(styles, key=styles.get) if styles else "Default"
        for s in sorted(miss, key=lambda x: x.start):
            st = round_ms(s.start + res.shift_ms)
            new = Event(comment=True, start=st, end=round_ms(s.end + res.shift_ms), style=style,
                        text="{QC: EKSİK}" + s.text)
            pos = len(evs)
            for i, e in enumerate(evs):
                if not e.comment and e.start > st:
                    pos = i
                    break
            evs.insert(pos, new)
        self.ctx.replace_all_events(evs, f"Eksik kaynak satırları ({len(miss)})")
        self.run()
        return len(miss)

    # ---------------------------------------------------------- dışa aktarma
    def _default_path(self, kind):
        base = os.path.splitext(self.ctx.doc.path or getattr(self.ctx.doc, "source_path", "") or "aday")[0]
        return {"html": base + "_karsilastirma.html", "csv": base + "_qc.csv", "marked": base + "_qc.ass"}[kind]

    def export(self, kind: str, path: str | None = None):
        res = self.result if (self.result is not None and not self.stale) else self.run()
        if res is None:
            return None
        if path is None:
            flt = {"html": "HTML (*.html)", "csv": "CSV (*.csv)", "marked": "ASS (*.ass)"}[kind]
            path, _ = QFileDialog.getSaveFileName(self, "Kaydet", self._default_path(kind), flt)
            if not path:
                return None
        ref = self.ctx.reference
        only = self.filter.currentData() == "flagged"
        if kind == "html":
            with open(path, "w", encoding="utf-8") as f:
                f.write(to_html(res, ref.path or "kaynak", self.ctx.doc.path or self.ctx.title, only_flags=only))
        elif kind == "csv":
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(to_csv(res, only_flags=only))
        else:
            if self.ctx.doc.path and os.path.abspath(path) == os.path.abspath(self.ctx.doc.path):
                self.ctx.status.emit("İşaretli kopya açık belgenin üstüne yazılamaz")
                return None
            marked_document(self.ctx.doc, res).save(path)
        self.ctx.status.emit(f"Kaydedildi: {path}")
        return path
