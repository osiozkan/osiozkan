from __future__ import annotations
import os
import sys
import threading

from PySide6.QtCore import QSettings, QTimer, Qt, Signal, QObject
from PySide6.QtGui import QAction, QKeySequence, QFont
from PySide6.QtWidgets import (
    QApplication, QCheckBox, QComboBox, QFileDialog, QFormLayout, QHBoxLayout, QInputDialog, QLabel, QLineEdit,
    QMainWindow, QMenu, QMessageBox, QPlainTextEdit, QProgressDialog, QPushButton, QSpinBox, QSplitter,
    QStatusBar, QTableView, QVBoxLayout, QWidget, QAbstractItemView, QDockWidget, QPlainTextEdit,
)

from .. import __version__, media
from ..media import AudioPlayer
from ..automation import Engine
from ..core import tools, assutil
from ..core.ass import Document, Event
from ..core.history import History
from ..core.timeutil import format_ass_time, parse_ass_time
from ..formats import load_any, save_any
from .dialogs import (StylingAssistant, SpellDialog, ParamsDialog, AutomationDialog, FindReplaceDialog, IssuesDialog, ShiftDialog, StyleEditor)
from .models import EventModel
from .widgets import PreviewWidget, WaveformWidget, VideoPlayback

APP = "Osisub"
OPEN_FILTER = "Altyazılar (*.ass *.ssa *.srt *.vtt);;Tüm dosyalar (*)"


class _PeakSignal(QObject):
    spell_ready = Signal(object, str)
    ready = Signal(object, int)
    keyframes = Signal(object)


class TimeEdit(QLineEdit):
    """H:MM:SS.cc editor; commit with Enter/focus out."""
    committed = Signal(int)

    def __init__(self):
        super().__init__("0:00:00.00")
        self.setMaximumWidth(110)
        self.editingFinished.connect(lambda: self.committed.emit(parse_ass_time(self.text())))

    def set_ms(self, ms: int):
        self.blockSignals(True)
        self.setText(format_ass_time(ms))
        self.blockSignals(False)


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.settings = QSettings("osisub", "osisub")
        self.doc = Document.new()
        self.history = History(self.doc)
        self.model = EventModel(self.doc)
        self.video = None
        self.fps = 23.976
        self.keyframes: list[int] = []
        self.engine = Engine(include_dirs=self._automation_dirs(), dialog_handler=self._automation_dialog,
                             file_dialog_handler=self._automation_file_dialog, log_handler=self._automation_log)
        self.model.max_cps = self.pref("max_cps", 20.0)
        self.macro_log: list[str] = []
        self._progress = None
        self.player = AudioPlayer()
        self.play_timer = QTimer(self, interval=40)
        self.play_timer.timeout.connect(self._play_tick)
        self.ref_doc = None
        self.playback = None
        self.recent: list[str] = [p for p in (self.settings.value("recent", []) or []) if p]
        self.setAcceptDrops(True)
        self._build_ui()
        self._build_menus()
        self._peak_sig = _PeakSignal()
        self._peak_sig.ready.connect(self._on_peaks)
        self._peak_sig.spell_ready.connect(self._spell_loaded)
        self._spell = {}
        self._peak_sig.keyframes.connect(self._on_keyframes)
        self._commit_timer = QTimer(self, singleShot=True, interval=700)
        self._commit_timer.timeout.connect(lambda: self.commit("Düzenleme"))
        self._loading_edit = False
        self.autosave_timer = QTimer(self, interval=120_000)
        self.autosave_timer.timeout.connect(self.autosave)
        self.autosave_timer.start()
        self.load_automation()
        self._update_title()
        self.resize(int(self.settings.value("w", 1280)), int(self.settings.value("h", 860)))

    # ------------------------------------------------------------ ui
    def _build_ui(self):
        top = QSplitter(Qt.Horizontal)
        self.preview = PreviewWidget()
        top.addWidget(self.preview)
        top.addWidget(self._build_edit_panel())
        top.setStretchFactor(0, 3); top.setStretchFactor(1, 2)
        self.wave = WaveformWidget()
        self.wave.seek.connect(self._wave_seek)
        self.wave.timing_changed.connect(self._wave_timing)
        self.wave.timing_committed.connect(lambda: self.commit("Dalga formunda zamanlama"))
        self.wave.kara_changed.connect(self._wave_kara)
        self.table = QTableView()
        self.table.setModel(self.model)
        self.table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.table.verticalHeader().setVisible(False)
        self.table.verticalHeader().setDefaultSectionSize(22)
        self.table.horizontalHeader().setStretchLastSection(True)
        for c, w in enumerate((46, 52, 92, 92, 52, 46, 100, 80, 80)):
            self.table.setColumnWidth(c, w)
        self.table.selectionModel().selectionChanged.connect(self._selection_changed)
        self.table.setContextMenuPolicy(Qt.CustomContextMenu)
        self.table.customContextMenuRequested.connect(self._table_menu)
        split = QSplitter(Qt.Vertical)
        split.addWidget(top); split.addWidget(self.wave); split.addWidget(self.table)
        split.setStretchFactor(0, 3); split.setStretchFactor(1, 1); split.setStretchFactor(2, 3)
        self.setCentralWidget(split)
        self.setStatusBar(QStatusBar())
        self.log_dock = QDockWidget("Otomasyon günlüğü", self)
        self.log_view = QPlainTextEdit(readOnly=True)
        self.log_dock.setWidget(self.log_view)
        self.addDockWidget(Qt.BottomDockWidgetArea, self.log_dock)
        self.log_dock.hide()
        self.ref_dock = QDockWidget("Referans altyazı", self)
        self.ref_view = QPlainTextEdit(readOnly=True)
        self.ref_dock.setWidget(self.ref_view)
        self.addDockWidget(Qt.RightDockWidgetArea, self.ref_dock)
        self.ref_dock.hide()
        self.preview.clicked.connect(self._preview_clicked)
        self.preview.dragged.connect(self._preview_dragged)
        self._move_start = None

    def _build_edit_panel(self) -> QWidget:
        w = QWidget()
        lay = QVBoxLayout(w)
        row = QHBoxLayout()
        self.e_comment = QCheckBox("Yorum")
        self.e_layer = QSpinBox(); self.e_layer.setRange(0, 9999); self.e_layer.setPrefix("K ")
        self.e_style = QComboBox(); self.e_actor = QLineEdit(); self.e_actor.setPlaceholderText("Aktör")
        self.e_effect = QLineEdit(); self.e_effect.setPlaceholderText("Efekt")
        for x in (self.e_comment, self.e_layer, self.e_style, self.e_actor, self.e_effect):
            row.addWidget(x)
        lay.addLayout(row)
        row2 = QHBoxLayout()
        self.e_start, self.e_end = TimeEdit(), TimeEdit()
        self.e_dur = QLabel("0.00 s"); self.e_cps = QLabel("")
        for t, x in (("Başla", self.e_start), ("Bitir", self.e_end)):
            row2.addWidget(QLabel(t)); row2.addWidget(x)
        row2.addWidget(self.e_dur); row2.addWidget(self.e_cps); row2.addStretch(1)
        lay.addLayout(row2)
        row3 = QHBoxLayout()
        for label, tag in (("B", "\\b1"), ("I", "\\i1"), ("U", "\\u1"), ("S", "\\s1")):
            b = QPushButton(label); b.setFixedWidth(30); b.clicked.connect(lambda _=False, t=tag: self.insert_tag(t)); row3.addWidget(b)
        for label, tag in (("\\an8", "\\an8"), ("\\an5", "\\an5"), ("\\pos", "\\pos(0,0)"), ("\\fad", "\\fad(200,200)")):
            b = QPushButton(label); b.clicked.connect(lambda _=False, t=tag: self.insert_tag(t)); row3.addWidget(b)
        row3.addStretch(1)
        lay.addLayout(row3)
        self.e_text = QPlainTextEdit()
        self.e_text.setFont(QFont("monospace", 11))
        self.e_text.setPlaceholderText("Metin (satır sonu için \\N)")
        lay.addWidget(self.e_text, 1)
        self.e_text.textChanged.connect(self._edit_text)
        self.e_comment.toggled.connect(lambda v: self._edit_field("comment", v))
        self.e_layer.valueChanged.connect(lambda v: self._edit_field("layer", v))
        self.e_style.activated.connect(lambda _: self._edit_field("style", self.e_style.currentText()))
        self.e_actor.textEdited.connect(lambda v: self._edit_field("actor", v))
        self.e_effect.textEdited.connect(lambda v: self._edit_field("effect", v))
        self.e_start.committed.connect(lambda ms: self._edit_time(True, ms))
        self.e_end.committed.connect(lambda ms: self._edit_time(False, ms))
        return w

    def _act(self, menu: QMenu, text: str, slot, shortcut: str | None = None) -> QAction:
        a = QAction(text, self)
        if shortcut:
            a.setShortcut(QKeySequence(shortcut))
        a.triggered.connect(lambda _=False: slot())
        menu.addAction(a)
        return a

    def _build_menus(self):
        mb = self.menuBar()
        f = mb.addMenu("&Dosya")
        self._act(f, "Yeni", self.new_doc, "Ctrl+N")
        self._act(f, "Aç…", self.open_dialog, "Ctrl+O")
        self._act(f, "Kaydet", self.save, "Ctrl+S")
        self._act(f, "Farklı kaydet…", self.save_as, "Ctrl+Shift+S")
        f.addSeparator()
        self.recent_menu = f.addMenu("Son açılanlar")
        self._rebuild_recent()
        self._act(f, "Video aç…", self.open_video)
        self._act(f, "Referans altyazı aç…", self.open_reference)
        self._act(f, "Altyazı dosyası ekle (birleştir)…", self.join_file)
        self._act(f, "İkiye böl (zamanda)…", self.split_file)
        self._act(f, "Betik bilgisi (Script Info)…", self.script_info)
        f.addSeparator()
        self._act(f, "Çıkış", self.close, "Ctrl+Q")
        e = mb.addMenu("&Düzen")
        self._act(e, "Geri al", self.undo, "Ctrl+Z")
        self._act(e, "Yinele", self.redo, "Ctrl+Y")
        e.addSeparator()
        self._act(e, "Tercihler…", self.preferences)
        self._act(e, "Bul ve değiştir…", self.find_replace, "Ctrl+F")
        self._act(e, "Satır ekle", self.insert_line, "Ctrl+Return")
        self._act(e, "Satırı çoğalt", self.duplicate, "Ctrl+D")
        self._act(e, "Seçili satırları sil", self.delete_lines, "Ctrl+Delete")
        self._act(e, "Seçili satırları birleştir", self.merge_lines, "Ctrl+M")
        self._act(e, "Satırı böl (imleçten)", self.split_line, "Ctrl+B")
        self._act(e, "Tümünü seç", lambda: self.table.selectAll(), "Ctrl+A")
        self._act(e, "Satıra git…", self.goto_line, "Ctrl+G")
        self._act(e, "Satırları seç (desen)…", self.select_by_pattern, "Ctrl+Shift+A")
        s = mb.addMenu("&Altyazı")
        self._act(s, "Stil yöneticisi…", self.style_manager, "Ctrl+Shift+T")
        self._act(s, "Stil asistanı…", self.styling_assistant)
        c = s.addMenu("Harf büyüklüğü")
        for label, mode in (("BÜYÜK HARF", "upper"), ("küçük harf", "lower"), ("Başlık Düzeni", "title"), ("Cümle düzeni", "sentence")):
            self._act(c, label, lambda m=mode: self.case(m))
        self._act(s, "Etiketleri temizle", self.strip_tags_sel)
        self._act(s, "Yaygın hataları düzelt", self.fix_errors)
        self._act(s, "Otomatik satır böl", self.auto_break)
        self._act(s, "İşitme engelli notlarını sil ([..], (..), AD:)", self.remove_hi)
        self._act(s, "Bozuk Türkçe karakterleri onar (Ã¼ → ü)", self.fix_mojibake)
        self._act(s, "Boş satırları sil", self.remove_empty)
        self._act(s, "Yinelenenleri birleştir", self.merge_dupes)
        self._act(s, "Sırala (başlangıç)", lambda: self.mutate("Sırala", lambda d: tools.sort_events(d, "start")))
        s.addSeparator()
        self._act(s, "Çözünürlüğü yeniden örnekle…", self.resample_dialog)
        self._act(s, "Font denetimi…", self.font_check)
        self._act(s, "Başka dosyadan stil al…", self.import_styles)
        self._act(s, "Kullanılmayan stilleri sil", self.remove_unused_styles)
        self._act(s, "Font göm (attach)…", self.attach_font)
        self._act(s, "Gömülü fontları çıkar…", self.extract_fonts)
        k = mb.addMenu("&Karaoke")
        self._act(k, "Otomatik \\k (hece)", lambda: self.auto_karaoke("syllable"))
        self._act(k, "Otomatik \\k (sözcük)", lambda: self.auto_karaoke("word"))
        self._act(k, "Otomatik \\k (karakter)", lambda: self.auto_karaoke("char"))
        self._act(k, "Lead-in / lead-out…", self.lead_in_out)
        t = mb.addMenu("&Zamanlama")
        self._act(t, "Zamanlamayı kaydır…", self.shift_dialog, "Ctrl+Shift+Left")
        self._act(t, "Zamanlama son işlemcisi…", self.postprocess_dialog)
        self._act(t, "Çakışmaları düzelt", self.fix_overlaps)
        self._act(t, "Süreleri zorla (min/CPS)", self.enforce_duration)
        self._act(t, "Kare hızına oturt", self.snap_frames)
        self._act(t, "Anahtar karelere oturt", self.snap_keyframes)
        self._act(t, "Kare hızı dönüştür…", self.convert_fps)
        self._act(t, "İki nokta ile eşitle…", self.two_point_sync)
        self._act(t, "Seçili satırı çal", self.play_line, "Ctrl+P")
        self._act(t, "Videoyu oynat / durdur", self.toggle_video, "Ctrl+Space")
        self._act(t, "Bir kare ileri", lambda: self.step_frames(1), "Alt+Right")
        self._act(t, "Bir kare geri", lambda: self.step_frames(-1), "Alt+Left")
        self._act(t, "Bitişi ata ve sonraki satıra geç", self.end_and_next, "Ctrl+3")
        self._act(t, "Çalmayı durdur", self.stop_play, "Escape")
        self.pick_action = self._act(t, "Önizleme etkileşimi açık (typesetting)", self.toggle_pick, "Ctrl+L")
        self.pick_action.setCheckable(True)
        ts = mb.addMenu("&Typesetting")
        from PySide6.QtGui import QActionGroup
        self.ts_group = QActionGroup(self)
        self.ts_mode = "pos"
        for key, label in (("pos", "\\pos — tıkla"), ("org", "\\org — tıkla"), ("move", "\\move — iki tıklama"),
                           ("clip", "\\clip — dikdörtgen sürükle"), ("iclip", "\\iclip — dikdörtgen sürükle")):
            a = QAction(label, self, checkable=True)
            a.setChecked(key == "pos")
            a.triggered.connect(lambda _=False, k=key: self._set_ts_mode(k))
            self.ts_group.addAction(a)
            ts.addAction(a)
        ts.addSeparator()
        self._act(ts, "Etkileşimi aç/kapat", lambda: (self.pick_action.setChecked(not self.pick_action.isChecked()), self.toggle_pick()))
        self._act(t, "Başlangıcı oynatma konumuna al", lambda: self.set_from_playhead(True), "Ctrl+1")
        self._act(t, "Bitişi oynatma konumuna al", lambda: self.set_from_playhead(False), "Ctrl+2")
        a = mb.addMenu("&Araçlar")
        self._act(a, "Kalite kontrol (CPS/CPL/çakışma)…", self.run_checks, "F7")
        self._act(a, "Yazım denetimi (Hunspell)…", self.spell_check, "F8")
        self._act(a, "İstatistikler…", self.show_stats)
        self._act(a, "Dalga formu çıkar (ffmpeg)", self.load_audio)
        self.auto_menu = mb.addMenu("&Otomasyon")
        v = mb.addMenu("&Görünüm")
        self._act(v, "Otomasyon günlüğü", lambda: self.log_dock.setVisible(not self.log_dock.isVisible()))
        h = mb.addMenu("&Yardım")
        self._act(h, "Hakkında", lambda: QMessageBox.about(self, APP, f"{APP} {__version__}\nAegisub uyumlu anime altyazı editörü"))

    # ------------------------------------------------------------ helpers
    def _automation_dirs(self=None):
        dirs = [os.path.expanduser("~/.osisub/automation/include"), os.path.expanduser("~/.aegisub/automation/include"),
                os.path.join(os.getcwd(), "automation", "include")]
        extra = str(self.settings.value("lua_include", "")) if self is not None else ""
        return dirs + ([extra] if extra else [])

    def selected_rows(self) -> list[int]:
        return sorted({i.row() for i in self.table.selectionModel().selectedRows()})

    def current_row(self) -> int:
        rows = self.selected_rows()
        if rows:
            return rows[0]
        idx = self.table.currentIndex()
        return idx.row() if idx.isValid() else -1

    def commit(self, label: str):
        self._commit_timer.stop()
        self.history.commit(label)
        self._refresh_keep_selection()
        self._update_title()

    def _refresh_keep_selection(self):
        from PySide6.QtCore import QItemSelectionModel
        rows, cur = self.selected_rows(), self.table.currentIndex().row()
        sm = self.table.selectionModel()
        sm.blockSignals(True)
        self.model.refresh()
        n = len(self.doc.events)
        for r in rows:
            if r < n:
                sm.select(self.model.index(r, 0), QItemSelectionModel.Select | QItemSelectionModel.Rows)
        if 0 <= cur < n:
            sm.setCurrentIndex(self.model.index(cur, 0), QItemSelectionModel.NoUpdate)
        sm.blockSignals(False)

    def mutate(self, label: str, fn, select=None):
        self._flush_edit()
        fn(self.doc)
        self.commit(label)
        if select is not None:
            self.select_rows(select)

    def _flush_edit(self):
        if self._commit_timer.isActive():
            self.commit("Düzenleme")

    def select_rows(self, rows):
        sm = self.table.selectionModel()
        sm.clearSelection()
        from PySide6.QtCore import QItemSelectionModel
        for r in rows:
            if 0 <= r < len(self.doc.events):
                sm.select(self.model.index(r, 0), QItemSelectionModel.Select | QItemSelectionModel.Rows)
        if rows:
            self.table.setCurrentIndex(self.model.index(rows[0], 0))
            self.table.scrollTo(self.model.index(rows[0], 0))

    def _update_title(self):
        name = os.path.basename(self.doc.filename) if self.doc.filename else "Adsız"
        self.setWindowTitle(f"{name}{'*' if self.history.dirty else ''} — {APP}")

    def status(self, msg: str, ms: int = 4000):
        self.statusBar().showMessage(msg, ms)

    # ------------------------------------------------------------ file
    def set_document(self, doc: Document):
        self.doc = doc
        self.history = History(doc)
        self.model.set_document(doc)
        self._fill_styles()
        self.wave.set_events(self.doc.events, -1)
        if doc.events:
            self.select_rows([0])
        else:
            self._load_edit(-1)
        self._update_title()
        self._refresh_preview()

    def new_doc(self):
        if self._confirm_discard():
            self.set_document(Document.new())

    def _confirm_discard(self) -> bool:
        if not self.history.dirty:
            return True
        r = QMessageBox.question(self, APP, "Kaydedilmemiş değişiklikler var. Devam edilsin mi?")
        return r == QMessageBox.Yes

    def open_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Altyazı aç", self.settings.value("last_dir", ""), OPEN_FILTER)
        if p:
            self.open_path(p)

    def open_path(self, path: str):
        if not self._confirm_discard():
            return
        try:
            doc = load_any(path)
        except Exception as e:
            QMessageBox.critical(self, APP, f"Açılamadı:\n{e}")
            return
        self.settings.setValue("last_dir", os.path.dirname(path))
        self._remember(path)
        if not doc.filename:
            doc.filename = None
        self.set_document(doc)
        self._import_path = path
        self.status(f"Açıldı: {path}")

    def autosave(self):
        if not self.history.dirty:
            return
        import time
        d = os.path.expanduser("~/.osisub/autosave")
        name = os.path.splitext(os.path.basename(self.doc.filename or "untitled"))[0]
        try:
            os.makedirs(d, exist_ok=True)
            self.doc.save(os.path.join(d, f"{name}.{time.strftime('%Y%m%d-%H%M%S')}.autosave.ass"), set_filename=False)
        except OSError:
            return
        self.status("Otomatik kaydedildi")

    def save(self):
        self._flush_edit()
        if not self.doc.filename:
            return self.save_as()
        if os.path.exists(self.doc.filename):
            try:
                import shutil
                shutil.copy2(self.doc.filename, self.doc.filename + ".bak")
            except OSError:
                pass
        self.doc.save()
        self.history.mark_saved()
        self._update_title()
        self.status("Kaydedildi")

    def save_as(self):
        self._flush_edit()
        p, flt = QFileDialog.getSaveFileName(self, "Farklı kaydet", self.settings.value("last_dir", ""),
                                             "ASS (*.ass);;SRT (*.srt);;WebVTT (*.vtt);;Metin (*.txt)")
        if not p:
            return
        ext = {"ASS": ".ass", "SRT": ".srt", "WebVTT": ".vtt", "Metin": ".txt"}.get(flt.split(" ")[0], ".ass")
        if not os.path.splitext(p)[1]:
            p += ext
        save_any(self.doc, p)
        if p.lower().endswith(".ass"):
            self.doc.filename = p
            self.history.mark_saved()
        self._update_title()
        self.status(f"Kaydedildi: {p}")

    def closeEvent(self, e):
        self.stop_play()
        if not self._confirm_discard():
            e.ignore()
            return
        self.settings.setValue("w", self.width()); self.settings.setValue("h", self.height())
        e.accept()

    # ------------------------------------------------------------ edit panel
    def _fill_styles(self):
        self.e_style.blockSignals(True)
        self.e_style.clear()
        self.e_style.addItems([s.name for s in self.doc.styles])
        self.e_style.blockSignals(False)

    def _selection_changed(self, *_):
        row = self.current_row()
        self._load_edit(row)

    def _load_edit(self, row: int):
        self._loading_edit = True
        try:
            ok = 0 <= row < len(self.doc.events)
            for w in (self.e_text, self.e_start, self.e_end, self.e_style, self.e_actor, self.e_effect, self.e_layer, self.e_comment):
                w.setEnabled(ok)
            if not ok:
                self.e_text.setPlainText("")
                return
            e = self.doc.events[row]
            self.e_text.setPlainText(e.text)
            self.e_start.set_ms(e.start); self.e_end.set_ms(e.end)
            self.e_actor.setText(e.actor); self.e_effect.setText(e.effect); self.e_layer.setValue(e.layer)
            self.e_comment.setChecked(e.comment)
            i = self.e_style.findText(e.style)
            if i < 0:
                self.e_style.addItem(e.style); i = self.e_style.findText(e.style)
            self.e_style.setCurrentIndex(i)
            self._update_info(row)
            self.wave.set_events(self.doc.events, row)
            self._set_wave_kara(row)
            self.wave.pos_ms = e.start
            self.wave.center_on(e.start)
        finally:
            self._loading_edit = False
        self._refresh_preview()
        self._update_reference()

    def _set_wave_kara(self, row):
        e = self.doc.events[row] if 0 <= row < len(self.doc.events) else None
        if e and assutil.has_karaoke(e.text):
            durs = assutil.kara_durations(e.text)
            texts = assutil.kara_syllable_texts(e.text)
            first = assutil.parse_syllables(e.text)[0]
            if first["text"] or True:
                self.wave.set_kara(row, durs, texts[: len(durs)])
                return
        self.wave.set_kara(-1, [], [])

    def _wave_kara(self, idx, durs):
        e = self.doc.events[idx]
        e.text = assutil.set_kara_durations(e.text, durs)
        self._loading_edit = True
        self.e_text.setPlainText(e.text)
        self._loading_edit = False
        self.model.refresh_row(idx)
        self._update_title_dirty()
        self._commit_timer.start()

    def _update_info(self, row):
        e = self.doc.events[row]
        self.e_dur.setText(f"{e.duration / 1000:.2f} s")
        self.e_cps.setText(f"{self.model.cps(row):.1f} CPS")

    def _edit_field(self, field: str, value):
        if self._loading_edit:
            return
        row = self.current_row()
        if row < 0:
            return
        rows = self.selected_rows() or [row]
        for r in rows:
            setattr(self.doc.events[r], field, value)
        self._after_edit(rows)

    def _edit_text(self):
        if self._loading_edit:
            return
        row = self.current_row()
        if row < 0:
            return
        text = self.e_text.toPlainText().replace("\n", "\\N")
        if text != self.doc.events[row].text:
            self.doc.events[row].text = text
            self._after_edit([row])

    def _edit_time(self, is_start: bool, ms: int):
        row = self.current_row()
        if row < 0:
            return
        e = self.doc.events[row]
        if is_start:
            e.start = ms
        else:
            e.end = ms
        self._after_edit([row])
        self.e_start.set_ms(e.start); self.e_end.set_ms(e.end)

    def _after_edit(self, rows):
        self._commit_timer.start()
        for r in rows:
            self.model.refresh_row(r)
        self._update_info(rows[0])
        self.wave.set_events(self.doc.events, self.current_row())
        self._refresh_preview()
        self._update_title_dirty()

    def _update_title_dirty(self):
        name = os.path.basename(self.doc.filename) if self.doc.filename else "Adsız"
        self.setWindowTitle(f"{name}* — {APP}")

    def insert_tag(self, tag: str):
        self.e_text.textCursor().insertText("{" + tag + "}")
        self.e_text.setFocus()

    def _refresh_preview(self):
        row = self.current_row()
        ms = self.doc.events[row].start if 0 <= row < len(self.doc.events) else 0
        if 0 <= row < len(self.doc.events):
            e = self.doc.events[row]
            ms = e.start + min(e.duration // 2, 500)
        self.preview.request(self._frame_ass(ms), ms, self.doc.res)

    def _frame_ass(self, ms: int) -> str:
        """ASS text restricted to events visible at `ms` (keeps preview fast for huge files)."""
        if len(self.doc.events) < 500:
            return self.doc.dumps()
        sub = Document()
        sub.info, sub.styles, sub.attachments = self.doc.info, self.doc.styles, self.doc.attachments
        sub.events = [e for e in self.doc.events if e.start <= ms < e.end]
        return sub.dumps()

    # ------------------------------------------------------------ structural edits
    def insert_line(self):
        self._flush_edit()
        row = self.current_row()
        base = self.doc.events[row] if row >= 0 else None
        ev = Event(start=base.end if base else 0, end=(base.end + 2000) if base else 2000,
                   style=base.style if base else self.doc.styles[0].name)
        at = row + 1 if row >= 0 else len(self.doc.events)
        self.doc.events.insert(at, ev)
        self.commit("Satır ekle")
        self.select_rows([at])
        self.e_text.setFocus()

    def duplicate(self):
        rows = self.selected_rows()
        if rows:
            self._flush_edit()
            new = tools.duplicate_events(self.doc, rows)
            self.commit("Çoğalt")
            self.select_rows(new)

    def delete_lines(self):
        rows = self.selected_rows()
        if not rows:
            return
        self._flush_edit()
        for r in reversed(rows):
            del self.doc.events[r]
        self.commit("Sil")
        self.select_rows([min(rows[0], len(self.doc.events) - 1)])

    def merge_lines(self):
        rows = self.selected_rows()
        if len(rows) > 1:
            self._flush_edit()
            first = tools.merge_events(self.doc, rows)
            self.commit("Birleştir")
            self.select_rows([first])

    def split_line(self):
        row = self.current_row()
        if row < 0:
            return
        self._flush_edit()
        pos = self.e_text.textCursor().position()
        text = self.doc.events[row].text
        pos = max(1, min(pos, len(text) - 1)) if len(text) > 1 else None
        tools.split_event(self.doc, row, at_char=pos)
        self.commit("Böl")
        self.select_rows([row + 1])

    def undo(self):
        self._commit_timer.stop() if False else None
        self._flush_edit()
        if self.history.undo():
            self._after_history()

    def redo(self):
        if self.history.redo():
            self._after_history()

    def _after_history(self):
        row = self.current_row()
        self._refresh_keep_selection()
        self._fill_styles()
        self._load_edit(min(row, len(self.doc.events) - 1))
        self._update_title()

    # ------------------------------------------------------------ text tools
    def _sel_or_all(self):
        rows = self.selected_rows()
        return rows if len(rows) > 1 else None

    def case(self, mode):
        sel = self._sel_or_all()
        self.mutate("Harf büyüklüğü", lambda d: tools.change_case(d, mode, True, sel))
        self._load_edit(self.current_row())

    def strip_tags_sel(self):
        sel = self._sel_or_all()
        self.mutate("Etiket temizle", lambda d: tools.strip_all_tags(d, sel))
        self._load_edit(self.current_row())

    def fix_errors(self):
        n = [0]
        self.mutate("Hata düzelt", lambda d: n.__setitem__(0, tools.fix_common_errors(d, self._sel_or_all())))
        self.status(f"{n[0]} satır düzeltildi"); self._load_edit(self.current_row())

    def auto_break(self):
        n = [0]
        self.mutate("Otomatik böl", lambda d: n.__setitem__(0, tools.auto_break(d, 42, self._sel_or_all())))
        self.status(f"{n[0]} satır bölündü"); self._load_edit(self.current_row())

    def remove_hi(self):
        n = [0]
        self.mutate("İE sil", lambda d: n.__setitem__(0, tools.remove_hearing_impaired(d, self._sel_or_all())))
        self.status(f"{n[0]} satır temizlendi")

    def fix_mojibake(self):
        n = [0]
        self.mutate("Karakter onar", lambda d: n.__setitem__(0, tools.fix_mojibake(d, self._sel_or_all())))
        self.status(f"{n[0]} satır onarıldı"); self._load_edit(self.current_row())

    def spell_check(self):
        lang = str(self.pref("spell_lang", "tr_TR"))
        if lang in self._spell:
            return self._spell_loaded(self._spell[lang], "")
        self.status("Sözlük yükleniyor…", 0)

        def work():
            try:
                from ..core.spell import SpellChecker
                self._peak_sig.spell_ready.emit(SpellChecker(lang), "")
            except Exception as e:
                self._peak_sig.spell_ready.emit(None, str(e))
        threading.Thread(target=work, daemon=True).start()

    def _spell_loaded(self, sc, err):
        if sc is None:
            QMessageBox.warning(self, APP, "Yazım denetimi kullanılamıyor:\n" + err +
                                "\n\n`pip install spylls` ve bir Hunspell sözlüğü (örn. hunspell-tr) gerekir.")
            return
        self._spell[sc.lang] = sc
        self.status("Sözlük hazır")
        self._flush_edit()
        dlg = SpellDialog(sc, self.doc, max(0, self.current_row()),
                          on_change=lambda: (self.model.refresh_row(self.current_row()) if self.current_row() >= 0 else None),
                          on_goto=lambda r: self.select_rows([r]), parent=self)
        dlg.finished.connect(lambda _=0: dlg.changed and self.commit("Yazım denetimi"))
        dlg.show()
        self._spell_dlg = dlg

    def show_stats(self):
        st = tools.statistics(self.doc)
        QMessageBox.information(self, "İstatistikler", "\n".join(f"{k}: {v}" for k, v in st.items()))

    def remove_empty(self):
        n = [0]
        self.mutate("Boşları sil", lambda d: n.__setitem__(0, tools.remove_empty(d)))
        self.status(f"{n[0]} boş satır silindi")

    def merge_dupes(self):
        n = [0]
        self.mutate("Yinelenenleri birleştir", lambda d: n.__setitem__(0, tools.merge_duplicates(d)))
        self.status(f"{n[0]} satır birleştirildi")

    def find_replace(self):
        dlg = FindReplaceDialog(self)

        def do_all():
            sel = self.selected_rows() if dlg.sel_only.isChecked() else None
            cnt = [0]
            try:
                self.mutate("Değiştir", lambda d: cnt.__setitem__(0, tools.search_replace(
                    d, dlg.find.text(), dlg.repl.text(), dlg.field.currentText(), dlg.regex.isChecked(),
                    dlg.case.isChecked(), dlg.word.isChecked(), sel)))
            except Exception as e:
                QMessageBox.warning(dlg, APP, f"Hata: {e}")
                return
            self.status(f"{cnt[0]} değişiklik yapıldı")
            self._load_edit(self.current_row())

        def find_next():
            import re
            pat = dlg.find.text()
            if not pat:
                return
            flags = re.I if dlg.case.isChecked() else 0
            try:
                rx = re.compile(pat if dlg.regex.isChecked() else re.escape(pat), flags)
            except re.error:
                return
            start = self.current_row() + 1
            n = len(self.doc.events)
            for k in range(n):
                r = (start + k) % n
                if rx.search(getattr(self.doc.events[r], dlg.field.currentText())):
                    self.select_rows([r]); return
            self.status("Bulunamadı")

        dlg.btn_all.clicked.connect(do_all)
        dlg.btn_next.clicked.connect(find_next)
        dlg.show()
        self._fr = dlg

    def styling_assistant(self):
        def apply(name):
            row = self.current_row()
            if row < 0:
                return
            self.doc.events[row].style = name
            self.commit("Stil")
            if row + 1 < len(self.doc.events):
                self.select_rows([row + 1])
            else:
                self._load_edit(row)
        dlg = StylingAssistant([s.name for s in self.doc.styles], apply, self)
        dlg.show()
        self._assistant = dlg

    def style_manager(self):
        dlg = StyleEditor(self.doc, self)
        if dlg.exec():
            new = dlg.result_styles()
            old_names = [s.name for s in self.doc.styles]
            renames = {o: n.name for o, n in zip(old_names, new) if o != n.name}
            self._flush_edit()
            self.doc.styles = new
            for e in self.doc.events:
                e.style = renames.get(e.style, e.style)
            self.commit("Stiller")
            self._fill_styles()
            self._load_edit(self.current_row())

    def script_info(self):
        text, ok = QInputDialog.getMultiLineText(self, "Script Info", "Anahtar: değer (satır başına bir)",
                                                 "\n".join(f"{k}: {v}" if k else v for k, v in self.doc.info))
        if ok:
            info = []
            for line in text.splitlines():
                if line.startswith(";"):
                    info.append(("", line))
                elif ":" in line:
                    k, v = line.split(":", 1); info.append((k.strip(), v.strip()))
            self.doc.info = info
            self.commit("Script Info")
            self._refresh_preview()

    # ------------------------------------------------------------ ass utilities
    def resample_dialog(self):
        rx, ry = self.doc.res
        x, ok = QInputDialog.getInt(self, "Yeniden örnekle", "Yeni PlayResX:", rx, 1, 20000)
        if not ok:
            return
        y, ok = QInputDialog.getInt(self, "Yeniden örnekle", "Yeni PlayResY:", ry, 1, 20000)
        if ok:
            self.mutate("Yeniden örnekle", lambda d: assutil.resample(d, x, y))
            self._load_edit(self.current_row())

    def font_check(self):
        miss = assutil.missing_fonts(self.doc)
        if not miss:
            QMessageBox.information(self, APP, "Eksik font bulunamadı (veya fc-list yok).")
            return
        QMessageBox.warning(self, APP, "Eksik fontlar:\n" + "\n".join(
            f"• {f}  ({', '.join(sorted(w)[:4])})" for f, w in sorted(miss.items())))

    def import_styles(self):
        p, _ = QFileDialog.getOpenFileName(self, "Stil kaynağı", self.settings.value("last_dir", ""), OPEN_FILTER)
        if p:
            src = load_any(p)
            n = [0]
            self.mutate("Stil içe aktar", lambda d: n.__setitem__(0, assutil.import_styles(d, src)))
            self._fill_styles(); self.status(f"{n[0]} stil alındı")

    def remove_unused_styles(self):
        names = assutil.unused_styles(self.doc)
        self.mutate("Kullanılmayan stiller", lambda d: setattr(d, "styles", [s for s in d.styles if s.name not in names]))
        self._fill_styles(); self.status(f"{len(names)} stil silindi")

    def auto_karaoke(self, mode):
        rows = self.selected_rows() or ([self.current_row()] if self.current_row() >= 0 else [])
        self.mutate("Otomatik karaoke", lambda d: assutil.apply_auto_karaoke(d, rows, mode))
        self._load_edit(self.current_row())

    def lead_in_out(self):
        li, ok = QInputDialog.getInt(self, "Lead-in", "Lead-in (ms):", 300, 0, 10000)
        if not ok:
            return
        lo, ok = QInputDialog.getInt(self, "Lead-out", "Lead-out (ms):", 300, 0, 10000)
        if ok:
            rows = self.selected_rows() or list(range(len(self.doc.events)))
            self.mutate("Lead-in/out", lambda d: assutil.lead_in_out(d, rows, li, lo))
            self._load_edit(self.current_row())

    def select_by_pattern(self):
        dlg = ParamsDialog("Satırları seç", [("pat", "Desen", ""), ("field", "Alan", ["text", "actor", "effect", "style"]),
                                             ("regex", "Düzenli ifade", False), ("case", "Büyük/küçük duyarlı", False),
                                             ("invert", "Tersine çevir", False)], self)
        if dlg.exec():
            v = dlg.values()
            try:
                rows = tools.select_lines(self.doc, v["pat"], ["text", "actor", "effect", "style"][v["field"]],
                                          v["regex"], v["case"], v["invert"])
            except Exception as e:
                QMessageBox.warning(self, APP, str(e)); return
            self.select_rows(rows)
            self.status(f"{len(rows)} satır seçildi")

    def postprocess_dialog(self):
        dlg = ParamsDialog("Zamanlama son işlemcisi", [("li", "Lead-in (ms)", 120), ("lo", "Lead-out (ms)", 500),
                                                       ("join", "Şu kadar boşluğu birleştir (ms)", 300),
                                                       ("kb", "Anahtar kare öncesi (ms)", 0), ("ka", "Anahtar kare sonrası (ms)", 0),
                                                       ("selonly", "Yalnızca seçililer", False)], self)
        if dlg.exec():
            v = dlg.values()
            sel = self.selected_rows() if v["selonly"] else None
            n = [0]
            self.mutate("Son işlemci", lambda d: n.__setitem__(0, tools.timing_postprocess(
                d, v["li"], v["lo"], v["join"], self.keyframes, v["kb"], v["ka"], sel)))
            self.status(f"{n[0]} satır değişti"); self._load_edit(self.current_row())

    def attach_font(self):
        ps, _ = QFileDialog.getOpenFileNames(self, "Font seç", "", "Fontlar (*.ttf *.otf *.ttc);;Tümü (*)")
        if ps:
            self.mutate("Font göm", lambda d: [assutil.attach_font(d, p) for p in ps])
            self.status(f"{len(ps)} font gömüldü")

    def extract_fonts(self):
        out = QFileDialog.getExistingDirectory(self, "Fontların çıkarılacağı klasör")
        if out:
            n = len(assutil.extract_fonts(self.doc, out))
            self.status(f"{n} font çıkarıldı")

    # ------------------------------------------------------------ timing
    def shift_dialog(self):
        rows = self.selected_rows()
        dlg = ShiftDialog(self, bool(rows))
        if dlg.exec():
            ms, what, scope = dlg.values()
            cur = rows[0] if rows else 0
            sel = None if scope == "all" else rows if scope == "sel" else range(cur, len(self.doc.events))
            self.mutate("Kaydır", lambda d: tools.shift(d, ms, sel, start=what in (0, 1), end=what in (0, 2)))
            self._load_edit(self.current_row())

    def fix_overlaps(self):
        n = [0]
        self.mutate("Çakışma düzelt", lambda d: n.__setitem__(0, tools.fix_overlaps(d)))
        self.status(f"{n[0]} çakışma düzeltildi"); self._load_edit(self.current_row())

    def enforce_duration(self):
        n = [0]
        self.mutate("Süre", lambda d: n.__setitem__(0, tools.enforce_duration(d, 1000, 7000, 17, self._sel_or_all())))
        self.status(f"{n[0]} satır ayarlandı"); self._load_edit(self.current_row())

    def snap_frames(self):
        self.mutate("Kareye oturt", lambda d: tools.snap_to_frames(d, self.fps, self._sel_or_all()))
        self._load_edit(self.current_row())

    def snap_keyframes(self):
        if not self.keyframes:
            self.status("Anahtar kare yok — önce video açın"); return
        n = [0]
        self.mutate("Anahtar kare", lambda d: n.__setitem__(0, tools.snap_to_keyframes(d, self.keyframes, self._sel_or_all())))
        self.status(f"{n[0]} satır oturtuldu"); self._load_edit(self.current_row())

    def convert_fps(self):
        s, ok = QInputDialog.getDouble(self, "Kare hızı", "Kaynak fps:", 25.0, 1, 240, 3)
        if not ok:
            return
        d_, ok = QInputDialog.getDouble(self, "Kare hızı", "Hedef fps:", 23.976, 1, 240, 3)
        if ok:
            self.mutate("FPS dönüştür", lambda d: tools.convert_framerate(d, s, d_))

    def two_point_sync(self):
        if len(self.doc.events) < 2:
            return
        n = len(self.doc.events)
        a, ok = QInputDialog.getInt(self, "İki nokta", "Birinci satır no:", 1, 1, n)
        if not ok: return
        an, ok = QInputDialog.getText(self, "İki nokta", "Birinci satırın yeni başlangıcı (H:MM:SS.cc):", text=format_ass_time(self.doc.events[a - 1].start))
        if not ok: return
        b, ok = QInputDialog.getInt(self, "İki nokta", "İkinci satır no:", n, 1, n)
        if not ok: return
        bn, ok = QInputDialog.getText(self, "İki nokta", "İkinci satırın yeni başlangıcı (H:MM:SS.cc):", text=format_ass_time(self.doc.events[b - 1].start))
        if ok:
            ea, eb = self.doc.events[a - 1], self.doc.events[b - 1]
            self.mutate("İki nokta eşitle", lambda d: tools.sync_two_points(d, ea.start, parse_ass_time(an), eb.start, parse_ass_time(bn)))

    def set_from_playhead(self, start: bool):
        row = self.current_row()
        if row < 0:
            return
        ms = self.wave.pos_ms
        e = self.doc.events[row]
        if start:
            e.start = min(ms, e.end)
        else:
            e.end = max(ms, e.start)
        self.commit("Zamanlama")
        self._load_edit(row)

    def pref(self, key, default):
        v = self.settings.value(key, default)
        try:
            return type(default)(v)
        except (TypeError, ValueError):
            return default

    def preferences(self):
        dlg = ParamsDialog("Tercihler", [
            ("cps", "En yüksek CPS", float(self.pref("max_cps", 20.0))),
            ("cpl", "Satır başına en çok karakter", int(self.pref("max_cpl", 42))),
            ("inc", "Ek Lua include klasörü", str(self.pref("lua_include", ""))),
            ("auto", "Ek Lua autoload klasörü", str(self.pref("lua_autoload", "")))], self)
        if dlg.exec():
            v = dlg.values()
            self.settings.setValue("max_cps", v["cps"]); self.settings.setValue("max_cpl", v["cpl"])
            self.settings.setValue("lua_include", v["inc"]); self.settings.setValue("lua_autoload", v["auto"])
            self.model.max_cps = v["cps"]
            self.model.refresh()
            self.reload_scripts()

    def run_checks(self):
        self._flush_edit()
        issues = tools.check(self.doc, max_cps=self.pref("max_cps", 20.0), max_cpl=self.pref("max_cpl", 42))
        dlg = IssuesDialog(issues, self, jump=lambda i: self.select_rows([i]))
        dlg.show()
        self._issues_dlg = dlg

    # ------------------------------------------------------------ media
    def open_video(self):
        p, _ = QFileDialog.getOpenFileName(self, "Video aç", self.settings.value("last_dir", ""), "Video (*.mkv *.mp4 *.avi *.webm *.mov *.m2ts);;Tümü (*)")
        if p:
            self.load_video(p)

    def load_video(self, p: str):
        if not media.available():
            QMessageBox.warning(self, APP, "ffmpeg/ffprobe bulunamadı.")
            return
        try:
            info = media.probe(p)
        except Exception as e:
            QMessageBox.critical(self, APP, f"Video okunamadı: {e}")
            return
        self.video = info
        self.fps = info.fps
        self.engine.fps.fps = info.fps
        self.engine.video_res = (info.width, info.height)
        self.engine.project_props["video_file"] = info.path
        self.engine.project_props["audio_file"] = info.path if info.has_audio else ""
        self.preview.video = info
        if self.doc.get_info("PlayResX") in ("", "0"):
            self.doc.set_info("PlayResX", str(info.width)); self.doc.set_info("PlayResY", str(info.height))
        self.status(f"Video: {info.width}x{info.height} @ {info.fps:.3f} fps")
        self.load_audio()
        threading.Thread(target=lambda: self._peak_sig.keyframes.emit(media.keyframes(p)), daemon=True).start()
        self._refresh_preview()

    def load_audio(self):
        if not self.video or not self.video.has_audio:
            self.status("Ses yok — önce video açın"); return
        path = self.video.path
        self.status("Dalga formu çıkarılıyor…", 0)

        def work():
            try:
                pk, rate = media.audio_peaks(path)
                self._peak_sig.ready.emit(pk, rate)
            except Exception:
                pass
        threading.Thread(target=work, daemon=True).start()

    def _on_peaks(self, pk, rate):
        self.wave.set_peaks(pk, rate)
        self.status("Dalga formu hazır")

    def _on_keyframes(self, kf):
        self.keyframes = kf
        self.wave.keyframes = kf
        self.wave.update()
        self.engine.keyframe_list = kf
        self.status(f"{len(kf)} anahtar kare")

    def _wave_seek(self, ms):
        self.preview.request(self._frame_ass(ms), ms, self.doc.res)

    def _wave_timing(self, idx, s, e):
        self.e_start.set_ms(s); self.e_end.set_ms(e)
        self.model.refresh_row(idx)
        self._update_info(idx)
        self._update_title_dirty()

    # ------------------------------------------------------------ automation
    def load_automation(self):
        dirs = [os.path.expanduser("~/.osisub/automation/autoload"), os.path.expanduser("~/.aegisub/automation/autoload"),
                os.path.join(os.getcwd(), "automation", "autoload")]
        extra = str(self.settings.value("lua_autoload", ""))
        for d in dirs + ([extra] if extra else []):
            self.engine.load_dir(d)
        self.rebuild_automation_menu()

    def rebuild_automation_menu(self):
        m = self.auto_menu
        m.clear()
        self._act(m, "Betik yükle…", self.load_script_dialog)
        self._act(m, "Betikleri yeniden yükle", self.reload_scripts)
        m.addSeparator()
        for s in self.engine.scripts:
            if s.error:
                a = m.addAction(f"⚠ {os.path.basename(s.path)}")
                a.triggered.connect(lambda _=False, e=s.error: QMessageBox.warning(self, APP, e))
                continue
            for mac in s.macros:
                self._act(m, mac.name, lambda mac=mac: self.run_macro(mac))
            for flt in s.filters:
                self._act(m, f"{flt.name} (filtre)", lambda f=flt: self.run_filter(f))

    def load_script_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Lua betiği", self.settings.value("last_dir", ""), "Lua (*.lua);;Tümü (*)")
        if p:
            s = self.engine.load_script(p)
            if s.error:
                QMessageBox.warning(self, APP, s.error)
            self.rebuild_automation_menu()

    def reload_scripts(self):
        paths = [s.path for s in self.engine.scripts]
        self.engine = Engine(include_dirs=self._automation_dirs(), dialog_handler=self._automation_dialog,
                             file_dialog_handler=self._automation_file_dialog, log_handler=self._automation_log)
        if self.video:
            self.engine.fps.fps = self.fps
            self.engine.video_res = (self.video.width, self.video.height)
        self.engine.keyframe_list = self.keyframes
        for p in paths:
            self.engine.load_script(p)
        self.rebuild_automation_menu()

    def _automation_log(self, level, msg):
        self.log_view.appendPlainText(msg.rstrip("\n"))
        if level <= 1:
            self.log_dock.show()

    def _automation_dialog(self, controls, buttons, ids):
        dlg = AutomationDialog(controls, buttons, ids, self)
        dlg.exec()
        if not dlg.clicked or dlg.clicked == ids.get("cancel") or str(dlg.clicked).lower() in ("cancel", "iptal"):
            return False, None
        return dlg.clicked, dlg.results()

    def _automation_file_dialog(self, kind, args):
        title = args[0] if args else ""
        if kind == "open":
            p, _ = QFileDialog.getOpenFileName(self, str(title))
        else:
            p, _ = QFileDialog.getSaveFileName(self, str(title))
        return p or None

    def _apply_result(self, label, res):
        if not res.ok:
            if res.cancelled:
                self.status("Betik iptal edildi")
            else:
                self.log_view.appendPlainText(res.error)
                self.log_dock.show()
                QMessageBox.warning(self, APP, f"Betik hatası:\n{res.error[:600]}")
            return
        self.doc.info, self.doc.styles, self.doc.events = res.document.info, res.document.styles, res.document.events
        self.commit(label)
        self._fill_styles()
        if res.selection:
            self.select_rows(res.selection)
        else:
            self._load_edit(min(self.current_row(), len(self.doc.events) - 1))
        self.status(f"{label} tamamlandı")

    def _progress_begin(self, title):
        dlg = QProgressDialog(title, "İptal", 0, 100, self)
        dlg.setWindowModality(Qt.WindowModal)
        dlg.setMinimumDuration(400)
        dlg.canceled.connect(lambda: setattr(self.engine, "cancel_flag", True))
        self._progress = dlg

        def handler(kind, v):
            if kind == "set":
                dlg.setValue(int(max(0, min(100, v))))
            elif kind in ("task", "title"):
                dlg.setLabelText(str(v))
            QApplication.processEvents()
        self.engine.progress_handler = handler

    def _progress_end(self):
        if self._progress:
            self._progress.reset()
            self._progress = None
        self.engine.progress_handler = None

    def run_macro(self, mac):
        self._flush_edit()
        sel = self.selected_rows()
        self.engine.clipboard_text = QApplication.clipboard().text()
        if not self.engine.validate_macro(mac, self.doc, sel, self.current_row()):
            self.status("Bu makro mevcut seçim için geçerli değil"); return
        self._progress_begin(mac.name)
        try:
            res = self.engine.run_macro(mac, self.doc, sel, self.current_row())
        finally:
            self._progress_end()
        QApplication.clipboard().setText(self.engine.clipboard_text)
        self._apply_result(mac.name, res)

    def run_filter(self, flt):
        self._flush_edit()
        cfg = {}
        if flt.lua[b"config"] is not None:
            from ..automation.runtime import from_lua
            self.engine._doc = self.doc
        self._progress_begin(flt.name)
        try:
            res = self.engine.run_filter(flt, self.doc, cfg)
        finally:
            self._progress_end()
        self._apply_result(flt.name, res)

    # ------------------------------------------------------------ recent / dnd / goto
    def _rebuild_recent(self):
        self.recent_menu.clear()
        for p in self.recent[:10]:
            self._act(self.recent_menu, p, lambda p=p: self.open_path(p))

    def _remember(self, path):
        self.recent = [path] + [p for p in self.recent if p != path]
        self.settings.setValue("recent", self.recent[:10])
        self._rebuild_recent()

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dropEvent(self, e):
        for u in e.mimeData().urls():
            p = u.toLocalFile()
            if p.lower().endswith((".ass", ".ssa", ".srt", ".vtt")):
                self.open_path(p); return
            if p.lower().endswith((".mkv", ".mp4", ".avi", ".webm", ".mov", ".m2ts")):
                self.load_video(p); return
            if p.lower().endswith(".lua"):
                self.engine.load_script(p); self.rebuild_automation_menu(); return

    def join_file(self):
        p, _ = QFileDialog.getOpenFileName(self, "Eklenecek altyazı", self.settings.value("last_dir", ""), OPEN_FILTER)
        if not p:
            return
        other = load_any(p)
        off, ok = QInputDialog.getInt(self, "Birleştir", "Zaman kayması (ms, boş=sonuna ekle):",
                                      max((e.end for e in self.doc.events), default=0), 0, 10**9)
        if ok:
            n = [0]
            self.mutate("Dosya ekle", lambda d: n.__setitem__(0, tools.append_document(d, other, off)))
            self._fill_styles(); self.status(f"{n[0]} satır eklendi")

    def split_file(self):
        t, ok = QInputDialog.getText(self, "Böl", "Bölme zamanı (H:MM:SS.cc):", text=format_ass_time(self.wave.pos_ms))
        if not ok:
            return
        a, b = tools.split_document(self.doc, parse_ass_time(t))
        base = self.doc.filename or os.path.join(self.settings.value("last_dir", ""), "split.ass")
        root, _ = os.path.splitext(base)
        a.save(root + ".part1.ass", set_filename=False); b.save(root + ".part2.ass", set_filename=False)
        self.status(f"Yazıldı: {root}.part1.ass / .part2.ass")

    def goto_line(self):
        n, ok = QInputDialog.getInt(self, "Satıra git", "Satır no:", max(1, self.current_row() + 1), 1, max(1, len(self.doc.events)))
        if ok:
            self.select_rows([n - 1])

    # ------------------------------------------------------------ playback / pick / reference
    def play_line(self):
        row = self.current_row()
        if row < 0 or not self.video or not AudioPlayer.available():
            self.status("Çalmak için video açın (ffplay gerekir)"); return
        e = self.doc.events[row]
        self.player.play(self.video.path, e.start, e.end)
        self.play_timer.start()

    def stop_play(self):
        self.player.stop()
        self.play_timer.stop()
        if self.playback:
            self.playback.stop()
            self.playback = None

    def toggle_video(self):
        if self.playback or self.player.playing:
            self.stop_play(); return
        if not self.video or not media.available():
            self.status("Önce video açın"); return
        self._flush_edit()
        start = self.wave.pos_ms
        w = max(320, min(self.preview.width(), 960))
        self.playback = VideoPlayback(self.video, self.doc.dumps(), start, w, self._on_video_frame, self._on_video_done)
        if AudioPlayer.available():
            self.player.play(self.video.path, start, 0)
        self.play_timer.start()

    def _on_video_frame(self, img, ms):
        self.preview.show_image(img)

    def _on_video_done(self):
        self.playback = None
        self.player.stop()
        self.play_timer.stop()

    def step_frames(self, n):
        step = 1000.0 / (self.fps or 23.976)
        self.wave.pos_ms = max(0, int(round(self.wave.pos_ms + n * step)))
        self.wave.update()
        self.preview.request(self._frame_ass(self.wave.pos_ms), self.wave.pos_ms, self.doc.res)

    def end_and_next(self):
        row = self.current_row()
        if row < 0:
            return
        e = self.doc.events[row]
        e.end = max(self.wave.pos_ms, e.start)
        self.commit("Bitiş")
        nxt = row + 1
        if nxt < len(self.doc.events):
            self.doc.events[nxt].start = e.end
            self.commit("Başlangıç")
            self.select_rows([nxt])
        else:
            self._load_edit(row)

    def _play_tick(self):
        if not self.player.playing:
            self.play_timer.stop(); return
        ms = self.player.position()
        self.wave.pos_ms = ms
        self.wave.center_on(ms)
        self._update_reference(ms)

    def toggle_pick(self):
        self.preview.pick_mode = self.pick_action.isChecked()
        self.preview.setCursor(Qt.CrossCursor if self.preview.pick_mode else Qt.ArrowCursor)

    def _set_ts_mode(self, mode):
        self.ts_mode = mode
        self._move_start = None
        self.preview.pick_mode = True
        self.pick_action.setChecked(True)
        self.toggle_pick()

    def _set_tag(self, row, name, value):
        """Insert or replace an override tag like \\pos(...) in the first block of the line."""
        import re
        e = self.doc.events[row]
        tag = "\\" + name + "(" + value + ")"
        rx = re.compile(r"\\" + name + r"\([^)]*\)")
        if name in ("clip", "iclip"):
            e.text = re.sub(r"\\i?clip\([^)]*\)", "", e.text)
        if rx.search(e.text):
            e.text = rx.sub(lambda m: tag, e.text, count=1)
        elif e.text.startswith("{"):
            e.text = "{" + tag + e.text[1:]
        else:
            e.text = "{" + tag + "}" + e.text
        self.commit(name)
        self._load_edit(row)

    def _preview_clicked(self, x, y):
        row = self.current_row()
        if row < 0:
            return
        if self.ts_mode == "org":
            self._set_tag(row, "org", "%d,%d" % (round(x), round(y)))
        elif self.ts_mode == "move":
            if self._move_start is None:
                self._move_start = (x, y)
                self.status("\\move: bitiş noktasına tıkla")
            else:
                x0, y0 = self._move_start
                self._move_start = None
                import re
                e = self.doc.events[row]
                e.text = re.sub(r"\\pos\([^)]*\)", "", e.text)
                self._set_tag(row, "move", "%d,%d,%d,%d" % (round(x0), round(y0), round(x), round(y)))
        elif self.ts_mode in ("pos",):
            self._set_tag(row, "pos", "%d,%d" % (round(x), round(y)))

    def _preview_dragged(self, x0, y0, x1, y1):
        row = self.current_row()
        if row < 0 or self.ts_mode not in ("clip", "iclip"):
            if self.ts_mode == "pos":
                self._preview_clicked(x1, y1)
            return
        xa, xb = sorted((round(x0), round(x1)))
        ya, yb = sorted((round(y0), round(y1)))
        self._set_tag(row, self.ts_mode, "%d,%d,%d,%d" % (xa, ya, xb, yb))

    def open_reference(self):
        p, _ = QFileDialog.getOpenFileName(self, "Referans altyazı", self.settings.value("last_dir", ""), OPEN_FILTER)
        if p:
            self.ref_doc = load_any(p)
            self.ref_dock.show()
            self._update_reference()

    def _update_reference(self, ms=None):
        if self.ref_doc is None:
            return
        row = self.current_row()
        if ms is None:
            if row < 0:
                return
            e = self.doc.events[row]
            lo, hi = e.start, e.end
        else:
            lo = hi = ms
        from ..core.tags import strip_tags
        hits = [strip_tags(r.text) for r in self.ref_doc.events if not r.comment and r.start < max(hi, lo + 1) and r.end > lo]
        self.ref_view.setPlainText("\n---\n".join(hits))

    # ------------------------------------------------------------ table menu
    def _table_menu(self, pos):
        m = QMenu(self)
        for text, slot in (("Satır ekle", self.insert_line), ("Çoğalt", self.duplicate), ("Birleştir", self.merge_lines),
                           ("Sil", self.delete_lines)):
            self._act(m, text, slot)
        m.exec(self.table.viewport().mapToGlobal(pos))


def main(argv=None):
    app = QApplication(argv or sys.argv)
    app.setApplicationName(APP)
    w = MainWindow()
    w.show()
    for a in (argv or sys.argv)[1:]:
        if os.path.isfile(a):
            w.open_path(a)
            break
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
