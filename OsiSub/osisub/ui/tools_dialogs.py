"""Subtitle Edit araçlarının pencereleri: ortak önizlemeli toplu araç penceresi + her araç.

Her pencere: seçenekler, kapsam (tüm/seçili satırlar), stil süzgeci, Önizle (önce → sonra listesi),
Uygula (tek geri al adımı). Seçenekler config.json > tools altında hatırlanır.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QButtonGroup, QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox,
                               QFormLayout, QHBoxLayout, QHeaderView, QLabel, QListWidget, QListWidgetItem,
                               QPushButton, QRadioButton, QSpinBox, QTextBrowser, QTreeWidget, QTreeWidgetItem,
                               QVBoxLayout, QWidget)

from ..core import setools as T
from ..core.asstime import format_time
from ..core.textutil import CPL_LIMIT
from .widgets import TimeEdit

_SIGNISH = ("sign", "tabela", "song", "op", "ed", "kara", "title", "typeset", "ts")


@dataclass
class Outcome:
    changes: dict | None = None          # {satır: {alan: değer}}
    events: list | None = None           # yapısal değişiklikte yeni olay listesi
    summary: str = ""
    preview: list = field(default_factory=list)      # (etiket, önce, sonra)


def _vis(text: str) -> str:
    return text.replace("\\N", " ⏎ ")


def _limit(ctx) -> int:
    try:
        return int(ctx.config["cpl_limit"] or CPL_LIMIT)
    except (TypeError, ValueError):
        return CPL_LIMIT


class BatchToolDialog(QDialog):
    key = ""                 # config anahtarı
    title = ""
    desc = ""
    undo_desc = ""
    style_filter = True
    skip_signs = False       # zamanlama araçları: tabela/şarkı stilleri varsayılan işaretsiz

    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle(self.title)
        self.resize(760, 560)
        self.cfg = dict((ctx.config["tools"] or {}).get(self.key, {}))
        lay = QVBoxLayout(self)
        if self.desc:
            d = QLabel(self.desc)
            d.setWordWrap(True)
            lay.addWidget(d)
        self.form = QFormLayout()
        self.scope = QComboBox()
        self.scope.addItems(["Tüm satırlar", "Seçili satırlar"])
        self.scope.setCurrentIndex(1 if len(ctx.selected_rows()) > 1 else 0)
        self.form.addRow("Kapsam", self.scope)
        self.styles = None
        if self.style_filter:
            self.styles = QListWidget()
            self.styles.setMaximumHeight(96)
            used: dict[str, int] = {}
            for e in ctx.doc.events:
                if not e.comment:
                    used[e.style] = used.get(e.style, 0) + 1
            for name, n in sorted(used.items(), key=lambda kv: -kv[1]):
                it = QListWidgetItem(f"{name}  ({n})")
                it.setData(Qt.UserRole, name)
                it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
                sign = self.skip_signs and len(used) > 1 and any(k in name.lower() for k in _SIGNISH)
                it.setCheckState(Qt.Unchecked if sign else Qt.Checked)
                self.styles.addItem(it)
            self.form.addRow("Stiller", self.styles)
        self.build(self.form)
        lay.addLayout(self.form)
        self.summary = QLabel("")
        self.summary.setWordWrap(True)
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["#", "Önce", "Sonra"])
        self.tree.setRootIsDecorated(False)
        self.tree.setUniformRowHeights(True)
        self.tree.header().setSectionResizeMode(1, QHeaderView.Stretch)
        self.tree.header().setSectionResizeMode(2, QHeaderView.Stretch)
        self.tree.setColumnWidth(0, 50)
        b_prev = QPushButton("Önizle")
        b_prev.clicked.connect(self.preview)
        h = QHBoxLayout()
        h.addWidget(b_prev)
        h.addWidget(self.summary, 1)
        lay.addLayout(h)
        lay.addWidget(self.tree, 1)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.button(QDialogButtonBox.Ok).setText("Uygula")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay.addWidget(bb)

    # ---------------------------------------------------------- alt sınıflar
    def build(self, form: QFormLayout):
        pass

    def compute(self, events, rows) -> Outcome:
        raise NotImplementedError

    def save_cfg(self) -> dict:
        return {}

    # ---------------------------------------------------------- ortak
    def rows(self) -> list[int]:
        evs = self.ctx.doc.events
        rows = range(len(evs)) if self.scope.currentIndex() == 0 else self.ctx.selected_rows()
        if self.styles is None:
            return list(rows)
        ok = {self.styles.item(i).data(Qt.UserRole) for i in range(self.styles.count())
              if self.styles.item(i).checkState() == Qt.Checked}
        return [r for r in rows if evs[r].style in ok]

    def _remember(self):
        allc = dict(self.ctx.config["tools"] or {})
        allc[self.key] = self.save_cfg()
        self.ctx.config["tools"] = allc
        self.ctx.config.save()

    def preview(self) -> Outcome:
        out = self.compute(self.ctx.doc.events, self.rows())
        self.tree.clear()
        items = []
        for label, a, b in out.preview[:2000]:
            it = QTreeWidgetItem([str(label), a, b])
            it.setToolTip(1, a)
            it.setToolTip(2, b)
            items.append(it)
        self.tree.addTopLevelItems(items)
        self.summary.setText(out.summary)
        return out

    def apply(self) -> Outcome | None:
        out = self.compute(self.ctx.doc.events, self.rows())
        self._remember()
        if out.events is not None:
            self.ctx.replace_all_events(out.events, self.undo_desc)
        elif out.changes:
            self.ctx.update_events(out.changes, self.undo_desc)
        else:
            self.summary.setText(out.summary or "Değişiklik yok")
            self.ctx.status.emit(f"{self.title}: değişiklik yok")
            return None
        self.ctx.status.emit(f"{self.title}: {out.summary}")
        self.accept()
        return out


def _text_preview(events, changes):
    return [(r + 1, _vis(events[r].text), _vis(ch["text"])) for r, ch in sorted(changes.items())]


def _time_preview(events, changes):
    out = []
    for r, ch in sorted(changes.items()):
        e = events[r]
        a = f"{format_time(e.start)} → {format_time(e.end)}"
        b = f"{format_time(ch.get('start', e.start))} → {format_time(ch.get('end', e.end))}"
        out.append((r + 1, a, b))
    return out


def _spin(lo, hi, val, suffix="", step=1):
    sb = QSpinBox()
    sb.setRange(lo, hi)
    sb.setSingleStep(step)
    sb.setValue(int(val))
    if suffix:
        sb.setSuffix(suffix)
    return sb


def _dspin(lo, hi, val, suffix="", step=0.1, dec=3):
    sb = QDoubleSpinBox()
    sb.setRange(lo, hi)
    sb.setDecimals(dec)
    sb.setSingleStep(step)
    sb.setValue(float(val))
    if suffix:
        sb.setSuffix(suffix)
    return sb


# ---------------------------------------------------------------------------
# Metin araçları
# ---------------------------------------------------------------------------

def break_changes(ctx, rows, force=False) -> dict:
    lim = _limit(ctx)
    evs = ctx.doc.events
    out = {}
    for r in rows:
        e = evs[r]
        if e.comment:
            continue
        t = T.auto_break(e.text, lim, force=force)
        if t != e.text:
            out[r] = {"text": t}
    return out


def unbreak_changes(ctx, rows) -> dict:
    evs = ctx.doc.events
    out = {}
    for r in rows:
        t = T.unbreak(evs[r].text)
        if t != evs[r].text:
            out[r] = {"text": t}
    return out


class CaseDialog(BatchToolDialog):
    key = "case"
    title = "Büyük/küçük harf"
    desc = "Yalnız görünen metin değişir (etiketler korunur). Türkçe i/İ, ı/I doğru çevrilir."
    undo_desc = "Büyük/küçük harf"

    def build(self, form):
        self.mode_group = QButtonGroup(self)
        h = QHBoxLayout()
        cur = self.cfg.get("mode", "sentence")
        for k, label in T.CASE_MODES.items():
            rb = QRadioButton(label)
            rb.setProperty("mode", k)
            rb.setChecked(k == cur)
            self.mode_group.addButton(rb)
            h.addWidget(rb)
        w = QWidget()
        w.setLayout(h)
        form.addRow("Biçim", w)

    def mode(self):
        b = self.mode_group.checkedButton()
        return b.property("mode") if b else "sentence"

    def save_cfg(self):
        return {"mode": self.mode()}

    def compute(self, events, rows):
        m = self.mode()
        ch = {}
        for r in rows:
            if events[r].comment:
                continue
            t = T.change_case(events[r].text, m)
            if t != events[r].text:
                ch[r] = {"text": t}
        return Outcome(changes=ch, summary=f"{len(ch)} satır değişecek", preview=_text_preview(events, ch))


class HiDialog(BatchToolDialog):
    key = "hi"
    title = "İşitme engelli metnini kaldır"
    desc = ("[ses efekti], (açıklama), ♪ şarkı sözü ♪ ve KONUŞMACI: etiketlerini siler; boşalan satırlar "
            "silinir. Resmi EN (SDH) altyazıyı referans yapmadan önce temizlemek için.")
    undo_desc = "İşitme engelli metnini kaldır"

    def build(self, form):
        c = self.cfg
        self.cb = {}
        for k, label, default in (("brackets", "[köşeli parantez]", True), ("parens", "(normal parantez)", True),
                                  ("music", "♪ şarkı ♪ / # … #", True), ("speaker", "KONUŞMACI: etiketleri", True),
                                  ("speaker_mixed_case", "Küçük harfli konuşmacı adları da (Ali:)", False),
                                  ("remove_empty", "Boşalan satırları sil", True)):
            cb = QCheckBox(label)
            cb.setChecked(bool(c.get(k, default)))
            self.cb[k] = cb
            form.addRow("", cb)

    def opts(self):
        return T.HiOptions(**{k: cb.isChecked() for k, cb in self.cb.items()})

    def save_cfg(self):
        return {k: cb.isChecked() for k, cb in self.cb.items()}

    def compute(self, events, rows):
        o = self.opts()
        target = set(rows)
        out, prev, changed, deleted = [], [], 0, 0
        for i, e in enumerate(events):
            if i not in target or e.comment:
                out.append(e)
                continue
            t = T.remove_hi(e.text, o)
            if t == e.text:
                out.append(e)
                continue
            if t == "" and o.remove_empty:
                deleted += 1
                prev.append((i + 1, _vis(e.text), "(silinecek)"))
                continue
            changed += 1
            prev.append((i + 1, _vis(e.text), _vis(t)))
            out.append(e.replace(text=t))
        if not prev:
            return Outcome(summary="Değişiklik yok")
        if not out:
            from ..core.assfile import Event
            out = [Event()]
        return Outcome(events=out, summary=f"{changed} satır değişecek, {deleted} satır silinecek", preview=prev)


class MergeDialog(BatchToolDialog):
    key = "merge"
    title = "Kısa satırları birleştir"
    desc = "Arasında az boşluk olan kısa ardışık satırları tek altyazıda birleştirir; metin Osi kuralıyla kırılır."
    undo_desc = "Kısa satırları birleştir"
    skip_signs = True

    def build(self, form):
        c = self.cfg
        lim = _limit(self.ctx)
        self.gap = _spin(0, 5000, c.get("gap", 250), " ms", 50)
        self.chars = _spin(10, 400, c.get("chars", 2 * lim), " karakter")
        self.dur = _spin(500, 30000, c.get("dur", 7000), " ms", 500)
        self.same_style = QCheckBox("Yalnız aynı stil")
        self.same_style.setChecked(c.get("same_style", True))
        self.same_actor = QCheckBox("Yalnız aynı aktör")
        self.same_actor.setChecked(c.get("same_actor", True))
        form.addRow("En çok boşluk", self.gap)
        form.addRow("Birleşik metin en çok", self.chars)
        form.addRow("Birleşik süre en çok", self.dur)
        form.addRow("", self.same_style)
        form.addRow("", self.same_actor)

    def save_cfg(self):
        return {"gap": self.gap.value(), "chars": self.chars.value(), "dur": self.dur.value(),
                "same_style": self.same_style.isChecked(), "same_actor": self.same_actor.isChecked()}

    def compute(self, events, rows):
        o = T.MergeOptions(max_gap_ms=self.gap.value(), max_chars=self.chars.value(),
                           max_duration_ms=self.dur.value(), same_style=self.same_style.isChecked(),
                           same_actor=self.same_actor.isChecked(), limit=_limit(self.ctx))
        out, n = T.merge_short(events, rows, o)
        if not n:
            return Outcome(summary="Birleştirilecek satır yok")
        old = {id(e) for e in events}
        prev = [("", "", _vis(e.text)) for e in out if id(e) not in old]
        return Outcome(events=out, summary=f"{n} satır bir öncekine katılacak ({len(events)} → {len(out)})",
                       preview=prev)


class SplitDialog(BatchToolDialog):
    key = "split"
    title = "Uzun satırları böl"
    desc = ("İki satıra sığmayan altyazıları ortaya en yakın cümle sınırından ikiye böler; süre metin "
            "uzunluğuyla paylaşılır, parçalar Osi kuralıyla kırılır.")
    undo_desc = "Uzun satırları böl"

    def build(self, form):
        c = self.cfg
        self.lim = _spin(10, 200, c.get("limit", _limit(self.ctx)), " karakter/satır")
        self.lines = _spin(1, 5, c.get("lines", 2), " satır")
        form.addRow("Satır sınırı", self.lim)
        form.addRow("En çok satır", self.lines)

    def save_cfg(self):
        return {"limit": self.lim.value(), "lines": self.lines.value()}

    def compute(self, events, rows):
        out, n = T.split_long(events, rows, T.SplitOptions(limit=self.lim.value(), max_lines=self.lines.value()),
                              self.ctx.frame_map)
        if not n:
            return Outcome(summary="Bölünecek satır yok")
        old = {id(e) for e in events}
        new = [e for e in out if id(e) not in old]
        prev = [("", "", f"{format_time(e.start)} {_vis(e.text)}") for e in new]
        return Outcome(events=out, summary=f"{n} satır ikiye bölünecek", preview=prev)


class SortDialog(BatchToolDialog):
    key = "sort"
    title = "Satırları sırala"
    desc = "Seçili satırlar kendi konumları içinde, diğerleri yerinde kalarak (kararlı) sıralanır."
    undo_desc = "Sırala"
    style_filter = False

    def build(self, form):
        c = self.cfg
        self.keys = []
        saved = c.get("keys", ["start", "", ""])
        for i in range(3):
            cb = QComboBox()
            if i:
                cb.addItem("—", "")
            for k, (label, _f) in T.SORT_KEYS.items():
                cb.addItem(label, k)
            j = cb.findData(saved[i] if i < len(saved) else "")
            cb.setCurrentIndex(max(0, j))
            self.keys.append(cb)
            form.addRow(f"{i + 1}. anahtar", cb)
        self.rev = QCheckBox("Azalan")
        self.rev.setChecked(c.get("reverse", False))
        form.addRow("", self.rev)

    def key_list(self):
        return [cb.currentData() for cb in self.keys if cb.currentData()]

    def save_cfg(self):
        return {"keys": [cb.currentData() for cb in self.keys], "reverse": self.rev.isChecked()}

    def compute(self, events, rows):
        keys = self.key_list()
        out = T.sort_rows(events, rows, keys, self.rev.isChecked())
        moved = sum(1 for a, b in zip(events, out) if a is not b)
        if not moved:
            return Outcome(summary="Sıra zaten bu")
        prev = [(i + 1, _vis(a.text), _vis(b.text)) for i, (a, b) in enumerate(zip(events, out)) if a is not b]
        return Outcome(events=out, summary=f"{moved} satırın yeri değişecek", preview=prev)


# ---------------------------------------------------------------------------
# Zamanlama araçları
# ---------------------------------------------------------------------------

class DurationsDialog(BatchToolDialog):
    key = "durations"
    title = "Süreleri ayarla"
    desc = ("Okuma hızına göre (Subtitle Edit gibi), yüzdeyle ya da sabit miktarla bitişleri ayarlar. "
            "Sonraki satıra taşmaz; en az boşluk payı korunur.")
    undo_desc = "Süreleri ayarla"
    skip_signs = True

    def build(self, form):
        c = self.cfg
        self.mode = QComboBox()
        self.mode.addItem("Okuma hızına göre (CPS)", "cps")
        self.mode.addItem("Yüzde", "percent")
        self.mode.addItem("Sabit ekle", "fixed")
        self.mode.setCurrentIndex(max(0, self.mode.findData(c.get("mode", "cps"))))
        self.cps = _dspin(5, 40, c.get("cps", 15.0), " kr/sn", 0.5, 1)
        self.percent = _dspin(10, 500, c.get("percent", 110.0), " %", 5, 1)
        self.fixed = _spin(-5000, 5000, c.get("fixed", 200), " ms", 50)
        self.min_ms = _spin(0, 10000, c.get("min", 700), " ms", 50)
        self.max_ms = _spin(500, 60000, c.get("max", 7000), " ms", 500)
        self.gap = _spin(0, 2000, c.get("gap", 84), " ms", 10)
        self.only_ext = QCheckBox("Yalnız uzat (süre asla kısalmaz)")
        self.only_ext.setChecked(c.get("only_extend", True))
        form.addRow("Yöntem", self.mode)
        form.addRow("Hedef okuma hızı", self.cps)
        form.addRow("Yüzde", self.percent)
        form.addRow("Sabit", self.fixed)
        form.addRow("En az süre", self.min_ms)
        form.addRow("En çok süre", self.max_ms)
        form.addRow("Sonraki satıra en az boşluk", self.gap)
        form.addRow("", self.only_ext)
        self.mode.currentIndexChanged.connect(self._mode_changed)
        self._mode_changed()

    def _mode_changed(self, *_):
        m = self.mode.currentData()
        self.cps.setEnabled(m == "cps")
        self.percent.setEnabled(m == "percent")
        self.fixed.setEnabled(m == "fixed")

    def opts(self):
        return T.DurationOptions(mode=self.mode.currentData(), cps=self.cps.value(), percent=self.percent.value(),
                                 fixed_ms=self.fixed.value(), min_ms=self.min_ms.value(), max_ms=self.max_ms.value(),
                                 min_gap_ms=self.gap.value(), only_extend=self.only_ext.isChecked())

    def save_cfg(self):
        return {"mode": self.mode.currentData(), "cps": self.cps.value(), "percent": self.percent.value(),
                "fixed": self.fixed.value(), "min": self.min_ms.value(), "max": self.max_ms.value(),
                "gap": self.gap.value(), "only_extend": self.only_ext.isChecked()}

    def compute(self, events, rows):
        ch = T.adjust_durations(events, rows, self.opts(), self.ctx.frame_map)
        return Outcome(changes=ch, summary=f"{len(ch)} satırın bitişi değişecek", preview=_time_preview(events, ch))


class BridgeDialog(BatchToolDialog):
    key = "bridge"
    title = "Boşlukları köprüle"
    desc = "Ardışık satırlar arasındaki küçük boşlukları kapatır (titreşimi önler); istenen en az boşluk bırakılır."
    undo_desc = "Boşlukları köprüle"
    skip_signs = True

    def build(self, form):
        c = self.cfg
        self.max_gap = _spin(10, 5000, c.get("max_gap", 500), " ms", 50)
        self.min_gap = _spin(0, 1000, c.get("min_gap", 0), " ms", 10)
        self.min_gap.setToolTip("Kapatılan boşlukta kalacak pay (ör. 2 kare ≈ 84 ms)")
        self.bias = _spin(0, 100, c.get("bias", 100), " %", 10)
        self.bias.setToolTip("Boşluğun ne kadarını önceki satırın uzaması alır (100 = önceki uzar, 0 = sonraki erken başlar)")
        form.addRow("Bundan küçük boşluklar", self.max_gap)
        form.addRow("Bırakılacak boşluk", self.min_gap)
        form.addRow("Önceki satırın payı", self.bias)

    def save_cfg(self):
        return {"max_gap": self.max_gap.value(), "min_gap": self.min_gap.value(), "bias": self.bias.value()}

    def compute(self, events, rows):
        ch = T.bridge_gaps(events, rows, self.max_gap.value(), self.min_gap.value(), self.bias.value() / 100.0)
        return Outcome(changes=ch, summary=f"{len(ch)} satır değişecek", preview=_time_preview(events, ch))


FPS_PRESETS = [("23.976", 24000 / 1001), ("24", 24.0), ("25", 25.0), ("29.97", 30000 / 1001), ("30", 30.0),
               ("50", 50.0), ("59.94", 60000 / 1001), ("60", 60.0)]


class FpsDialog(BatchToolDialog):
    key = "fps"
    title = "Kare hızını / hızı değiştir"
    desc = ("Kare hızı: başka hızdaki sürümün altyazısını uyarlar (ör. 25 fps PAL → 23.976). "
            "Hız (%): tüm zamanları ölçekler (100 = değişmez).")
    undo_desc = "Kare hızı / hız"
    style_filter = False

    def build(self, form):
        c = self.cfg
        self.mode = QComboBox()
        self.mode.addItem("Kare hızı", "fps")
        self.mode.addItem("Hız (%)", "speed")
        self.mode.setCurrentIndex(max(0, self.mode.findData(c.get("mode", "fps"))))
        self.src = QComboBox()
        self.dst = QComboBox()
        for cb in (self.src, self.dst):
            cb.setEditable(True)
            for label, _v in FPS_PRESETS:
                cb.addItem(label)
        self.src.setCurrentText(c.get("src", "25"))
        self.dst.setCurrentText(c.get("dst", "23.976"))
        self.speed = _dspin(10, 1000, c.get("speed", 100.0), " %", 0.1, 3)
        form.addRow("Yöntem", self.mode)
        form.addRow("Kaynak kare hızı", self.src)
        form.addRow("Hedef kare hızı", self.dst)
        form.addRow("Hız", self.speed)
        self.mode.currentIndexChanged.connect(self._mode_changed)
        self._mode_changed()

    def _mode_changed(self, *_):
        fps = self.mode.currentData() == "fps"
        self.src.setEnabled(fps)
        self.dst.setEnabled(fps)
        self.speed.setEnabled(not fps)

    @staticmethod
    def _fps(text):
        t = text.strip().replace(",", ".")
        for label, v in FPS_PRESETS:
            if t == label:
                return v
        return float(t)

    def save_cfg(self):
        return {"mode": self.mode.currentData(), "src": self.src.currentText(), "dst": self.dst.currentText(),
                "speed": self.speed.value()}

    def compute(self, events, rows):
        try:
            if self.mode.currentData() == "fps":
                a, b = self._fps(self.src.currentText()), self._fps(self.dst.currentText())
                if a <= 0 or b <= 0:
                    raise ValueError
                ch = T.change_fps(events, rows, a, b)
            else:
                ch = T.change_speed(events, rows, self.speed.value())
        except ValueError:
            return Outcome(summary="Kare hızı geçersiz")
        return Outcome(changes=ch, summary=f"{len(ch)} satır değişecek", preview=_time_preview(events, ch))


class PointSyncDialog(BatchToolDialog):
    key = "pointsync"
    title = "Noktalı senkron"
    desc = ("Seçili satırlar senkron noktasıdır: her birinin DOĞRU başlangıç zamanını gir (ya da videoyu doğru "
            "kareye getirip 'Videodan al'). Tek nokta düz kaydırır; iki ve daha çok nokta arası doğrusal esnetilir "
            "(farklı kare hızı / reklam arası kaymaları).")
    undo_desc = "Noktalı senkron"
    style_filter = False

    def build(self, form):
        self.anchor_rows = list(self.ctx.selected_rows())[:12]
        self.edits = []
        evs = self.ctx.doc.events
        media = getattr(self.ctx, "media", None)
        for r in self.anchor_rows:
            e = evs[r]
            te = TimeEdit()
            te.setValue(e.start)
            row = QHBoxLayout()
            row.addWidget(QLabel(f"{format_time(e.start)} →"))
            row.addWidget(te)
            b = QPushButton("Videodan al")
            b.setEnabled(media is not None and media.index is not None)
            b.clicked.connect(lambda _=False, t=te: t.setValue(self._video_ms()))
            row.addWidget(b)
            w = QWidget()
            w.setLayout(row)
            label = T.strip_tags(e.text).replace("\\N", " ")[:40]
            form.addRow(f"#{r + 1} {label}", w)
            self.edits.append(te)
        self.scope.setCurrentIndex(0)
        if not self.anchor_rows:
            form.addRow("", QLabel("Önce ızgarada en az bir satır seçin."))

    def _video_ms(self):
        media = getattr(self.ctx, "media", None)
        fm = self.ctx.frame_map
        if media is None or fm is None:
            return 0
        return fm.time_at_frame(media.frame, 1)

    def points(self):
        evs = self.ctx.doc.events
        return [(evs[r].start, te.value()) for r, te in zip(self.anchor_rows, self.edits)]

    def compute(self, events, rows):
        pts = self.points()
        if not pts:
            return Outcome(summary="Senkron noktası yok")
        ch = T.point_sync(events, rows, pts)
        return Outcome(changes=ch, summary=f"{len(pts)} nokta · {len(ch)} satır değişecek",
                       preview=_time_preview(events, ch))


# ---------------------------------------------------------------------------
# İstatistik
# ---------------------------------------------------------------------------

def stats_html(st: dict) -> str:
    def ms(v):
        s = int(v) // 1000
        return f"{s // 3600}:{s % 3600 // 60:02d}:{s % 60:02d}"
    rows = [("Satır", f"{st['lines']} ({st['dialogue']} diyalog, {st['comments']} yorum)"),
            ("Kelime / karakter", f"{st['words']} / {st['chars']}"),
            ("Toplam gösterim süresi", ms(st["total_ms"])), ("İlk–son satır arası", ms(st["span_ms"])),
            ("Ortalama süre", f"{st['avg_ms'] / 1000:.2f} sn"),
            ("CPS ortalama / ortanca / en yüksek", f"{st['cps_avg']:.1f} / {st['cps_median']:.1f} / {st['cps_max']:.1f}"),
            ("CPS sınırını aşan", str(st["cps_over"])), ("CPL sınırını aşan", str(st["cpl_over"])),
            ("İki ve daha çok satırlı", str(st["two_lines"]))]
    h = ["<table cellpadding=3>"] + [f"<tr><td>{a}</td><td><b>{b}</b></td></tr>" for a, b in rows] + ["</table>"]
    if st["styles"]:
        h.append("<p><b>Stiller:</b> " + ", ".join(f"{n} ({c})" for n, c in st["styles"]) + "</p>")
    if st["actors"]:
        h.append("<p><b>Aktörler:</b> " + ", ".join(f"{n} ({c})" for n, c in st["actors"][:30]) + "</p>")
    if st["top_words"]:
        h.append("<p><b>Sık kelimeler:</b> " + ", ".join(f"{w} ({c})" for w, c in st["top_words"]) + "</p>")
    return "".join(h)


class StatsDialog(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.setWindowTitle("İstatistikler")
        self.resize(560, 520)
        rows = ctx.selected_rows()
        evs = ctx.doc.events
        sel = [evs[r] for r in rows] if len(rows) > 1 else list(evs)
        self.data = T.statistics(sel, _limit(ctx), float(ctx.config["qc_cps_max"] or 17))
        tb = QTextBrowser()
        tb.setHtml(("<p><i>Seçili satırlar</i></p>" if len(rows) > 1 else "") + stats_html(self.data))
        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addWidget(tb)
        lay.addWidget(bb)


TOOL_DIALOGS = {
    "case": CaseDialog, "hi": HiDialog, "merge": MergeDialog, "split": SplitDialog, "sort": SortDialog,
    "durations": DurationsDialog, "bridge": BridgeDialog, "fps": FpsDialog, "pointsync": PointSyncDialog,
}
