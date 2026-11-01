"""Main window and application entry point."""
from __future__ import annotations

import os
import sys
import time

from PySide6.QtCore import QSettings, QThread, QTimer, Qt, Signal, QObject
from PySide6.QtGui import QAction, QActionGroup, QFont, QGuiApplication, QKeySequence, QShortcut
from PySide6.QtWidgets import (QAbstractItemView, QApplication, QDockWidget, QFileDialog, QHeaderView, QInputDialog,
                               QLabel, QMainWindow, QMenu, QMessageBox, QStatusBar, QTableView, QWidget, QVBoxLayout)

from .. import __version__, APP_NAME, media
from ..automation import (AutomationScript, CancelledError, Host, ProjectInfo, ScriptError, load_scripts,
                          DEFAULT_INCLUDE_DIR)
from ..core import checks, tools
from ..core.ass import Event, Style
from ..core.formats import export_text
from ..core.tags import plain_text, set_override, get_override
from ..core.timecode import ms_to_ass
from ..project import Project
from .dialogs import (FindReplaceDialog, FpsDialog, PropertiesDialog, QCDialog, ShiftDialog, StylesDialog, SyncDialog,
                      TimingDialog)
from .editpanel import EditPanel
from .lua_ui import LogWindow, QtUI
from .model import EventModel, COLS
from .video import VideoPanel
from .misc_dialogs import FontsDialog, AttachmentsDialog, StatsDialog, ShortcutsDialog, ExportFiltersDialog
from .tools_dialogs import KaraokeDialog, SelectDialog, ReferencePanel
from .waveform import WaveformWidget
from .widgets import apply_theme

OPEN_FILTER = "Altyazılar (*.ass *.ssa *.srt *.vtt *.sub *.lrc *.txt);;Tümü (*)"
VIDEO_FILTER = "Medya (*.mkv *.mp4 *.avi *.webm *.mov *.m4v *.ts *.m2ts *.flv *.wmv *.mp3 *.flac *.wav *.ogg *.m4a *.aac *.opus);;Tümü (*)"
USER_DIR = os.path.expanduser("~/.subforge")
REPO_SCRIPTS = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(__file__))), "scripts")


class Worker(QThread):
    done = Signal(object, object)   # result, error

    def __init__(self, fn, parent=None):
        super().__init__(parent)
        self.fn = fn

    def run(self):
        try:
            self.done.emit(self.fn(), None)
        except Exception as e:  # noqa
            self.done.emit(None, e)


class MainWindow(QMainWindow):
    def __init__(self, path: str | None = None):
        super().__init__()
        self.settings = QSettings("SubForge", "SubForge")
        self.project = Project()
        self.model = EventModel(self.project)
        self.dark = self.settings.value("dark", True, type=bool)
        self.scripts: list[AutomationScript] = []
        self._workers: list[Worker] = []
        self._pending_range = None
        self._drag_snapshot = None
        self._find_dlg = None
        self._qc_dlg = None
        self._clip_events: list[str] = []
        self._qc_timer = QTimer(self); self._qc_timer.setSingleShot(True); self._qc_timer.setInterval(600)
        self._qc_timer.timeout.connect(self._live_qc)

        self.grid = QTableView()
        self.grid.setModel(self.model)
        self.grid.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.grid.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.grid.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self.grid.setAlternatingRowColors(True)
        self.grid.verticalHeader().setVisible(False)
        self.grid.verticalHeader().setDefaultSectionSize(22)
        hh = self.grid.horizontalHeader()
        hh.setStretchLastSection(True)
        for i, w in enumerate((46, 30, 92, 92, 52, 44, 110, 90, 90)):
            self.grid.setColumnWidth(i, w)
        self.grid.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.grid.customContextMenuRequested.connect(self._grid_menu)
        self.setCentralWidget(self.grid)
        self.setAcceptDrops(True)

        self.video = VideoPanel(self.project)
        self.wave = WaveformWidget(self.project)
        self.wave.model = self.model
        self.edit = EditPanel()
        self._dock("Video", self.video, Qt.DockWidgetArea.TopDockWidgetArea, "dock_video")
        self._dock("Düzenleme", self.edit, Qt.DockWidgetArea.TopDockWidgetArea, "dock_edit")
        self.reference = ReferencePanel(self)
        self._dock("Referans (çeviri)", self.reference, Qt.DockWidgetArea.RightDockWidgetArea, "dock_ref")
        self.docks["dock_ref"].hide()
        self._dock("Ses", self.wave, Qt.DockWidgetArea.BottomDockWidgetArea, "dock_audio")
        self.splitDockWidget(self.docks["dock_video"], self.docks["dock_edit"], Qt.Orientation.Horizontal)

        self.setStatusBar(QStatusBar())
        self.lbl_info = QLabel(""); self.lbl_media = QLabel(""); self.lbl_qc = QLabel("")
        self.statusBar().addWidget(self.lbl_info, 1)
        self.statusBar().addPermanentWidget(self.lbl_qc)
        self.statusBar().addPermanentWidget(self.lbl_media)

        self._build_actions()
        self._init_shortcuts()
        self._build_menus()
        self._wire()
        apply_theme(QApplication.instance(), self.dark)
        self._apply_dark_to_widgets()
        self.resize(1500, 950)
        self._restore()
        self.reload_scripts()
        self._refresh_all()
        self.autosave = QTimer(self); self.autosave.setInterval(60_000)
        self.autosave.timeout.connect(self._autosave); self.autosave.start()
        if path:
            self.open_path(path)

    # ------------------------------------------------------------------ layout helpers
    docks: dict

    def _dock(self, title, widget, area, name):
        if not hasattr(self, "docks"):
            self.docks = {}
        d = QDockWidget(title, self)
        d.setObjectName(name)
        d.setWidget(widget)
        self.addDockWidget(area, d)
        self.docks[name] = d
        return d

    def _restore(self):
        g = self.settings.value("geometry")
        s = self.settings.value("state")
        if g:
            self.restoreGeometry(g)
        if s:
            self.restoreState(s)
        self.resizeDocks([self.docks["dock_video"], self.docks["dock_edit"]], [680, 820], Qt.Orientation.Horizontal)
        self.resizeDocks([self.docks["dock_video"], self.docks["dock_audio"]], [420, 190], Qt.Orientation.Vertical)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dropEvent(self, e):
        for url in e.mimeData().urls():
            f = url.toLocalFile()
            ext = os.path.splitext(f)[1].lower()
            if ext in (".ass", ".ssa", ".srt", ".vtt", ".sub", ".lrc"):
                self.open_path(f)
            elif ext == ".lua":
                import shutil
                os.makedirs(os.path.join(USER_DIR, "automation", "autoload"), exist_ok=True)
                shutil.copy2(f, os.path.join(USER_DIR, "automation", "autoload"))
                self.reload_scripts()
                self.statusBar().showMessage(f"Betik yüklendi: {os.path.basename(f)}", 5000)
            elif ext:
                self.open_video(f)

    def closeEvent(self, e):
        if not self._confirm_discard():
            e.ignore()
            return
        for w in list(self._workers):
            w.wait(3000)
        self.settings.setValue("geometry", self.saveGeometry())
        self.settings.setValue("state", self.saveState())
        e.accept()

    # ------------------------------------------------------------------ actions / menus
    def act(self, text, slot, shortcut=None, tip=None, checkable=False, checked=False):
        a = QAction(text, self)
        if shortcut:
            a.setShortcut(QKeySequence(shortcut))
            a.setShortcutContext(Qt.ShortcutContext.WindowShortcut)
        if tip:
            a.setStatusTip(tip)
        a.setCheckable(checkable)
        if checkable:
            a.setChecked(checked)
            a.toggled.connect(slot)
        else:
            a.triggered.connect(lambda _=False, s=slot: s())
        return a

    def _build_actions(self):
        A = self.act
        self.a_new = A("&Yeni", self.new_file, "Ctrl+N")
        self.a_open = A("&Aç…", self.open_dialog, "Ctrl+O")
        self.a_save = A("&Kaydet", self.save, "Ctrl+S")
        self.a_saveas = A("&Farklı kaydet…", self.save_as, "Ctrl+Shift+S")
        self.a_export = A("Dışa aktar (SRT/VTT/TXT)…", self.export_dialog, "Ctrl+E")
        self.a_quit = A("Çı&kış", self.close, "Ctrl+Q")
        self.a_undo = A("&Geri al", self.project.undo, "Ctrl+Z")
        self.a_redo = A("&Yinele", self.project.redo, "Ctrl+Y")
        self.a_cut = A("Satırları kes", self.cut_lines, "Ctrl+Shift+X")
        self.a_copy = A("Satırları kopyala", self.copy_lines, "Ctrl+Shift+C")
        self.a_paste = A("Satırları yapıştır", self.paste_lines, "Ctrl+Shift+V")
        self.a_selall = A("Tümünü seç", lambda: self.grid.selectAll(), "Ctrl+Shift+A")
        self.a_ins_b = A("Önüne satır ekle", lambda: self.insert_line(False), "Ctrl+Shift+Insert")
        self.a_ins_a = A("Arkasına satır ekle", lambda: self.insert_line(True), "Ctrl+Insert")
        self.a_dup = A("Çoğalt", self.duplicate_lines, "Ctrl+D")
        self.a_del = A("Sil", self.delete_lines, "Ctrl+Delete")
        self.a_join = A("Sonrakiyle birleştir", self.join_next, "Ctrl+J")
        self.a_split = A("İmleçten böl", self.split_at_cursor, "Ctrl+B")
        self.a_comment = A("Yorum yap/geri al", self.toggle_comment, "Ctrl+/")
        self.a_find = A("Bul/Değiştir…", self.show_find, "Ctrl+F")
        self.a_sort = A("Zamana göre sırala", lambda: self._do("Sırala", lambda d: d.sort_events("start")))
        self.a_styles = A("Stil yöneticisi…", self.show_styles, "Ctrl+Shift+T")
        self.a_props = A("Betik özellikleri…", self.show_props)
        self.a_shift = A("Zamanları kaydır…", self.show_shift, "Ctrl+Shift+S" if False else "Ctrl+H")
        self.a_timing = A("Zamanlama son işlemcisi…", self.show_timing, "Ctrl+Shift+P")
        self.a_sync = A("İki noktalı senkron…", self.show_sync)
        self.a_fpschg = A("FPS değiştir…", self.show_fps_change)
        self.a_bridge = A("Boşlukları kapat (≤250 ms)", lambda: self._tool("Boşluk kapat", lambda ev: tools.bridge_gaps(ev, 250)))
        self.a_overlap = A("Çakışmaları düzelt", lambda: self._tool("Çakışma düzelt", lambda ev: tools.fix_overlaps(ev)))
        self.a_snap = A("Anahtar karelere yapıştır", lambda: self._tool("Anahtar kare", lambda ev: tools.snap_to_keyframes(ev, self.project.keyframes)))
        self.a_set_start = A("Başlangıcı video konumuna ayarla", lambda: self._set_time("start"), "Ctrl+[")
        self.a_set_end = A("Bitişi video konumuna ayarla", lambda: self._set_time("end"), "Ctrl+]")
        self.a_apply_range = A("Ses aralığını aktif satıra uygula", self.apply_range)
        self.a_new_range = A("Ses aralığından yeni satır", self.new_from_range)
        self.a_karaoke = A("Karaoke zamanlayıcı…", self.show_karaoke, "Ctrl+K")
        self.a_select = A("Satır seç…", self.show_select, "Ctrl+Shift+F")
        self.a_qc = A("Kalite kontrol…", self.show_qc, "Ctrl+Shift+Q")
        self.a_moji = A("Bozuk karakterleri onar (Ã§ → ç)", lambda: self._tool("Bozuk karakter onar", tools.repair_mojibake))
        self.a_fix = A("Yaygın hataları düzelt", lambda: self._tool("Yaygın hatalar", tools.fix_common_errors))
        self.a_balance = A("Satırları dengele (42)", lambda: self._tool("Dengele", lambda ev: tools.balance_lines(ev, 42)))
        self.a_hi = A("İşitme engelli notlarını sil", lambda: self._do("İE sil", lambda d: tools.remove_hearing_impaired(d)))
        self.a_dups = A("Yinelenen satırları birleştir", lambda: self._do("Yinelenenleri birleştir", lambda d: tools.merge_duplicates(d)))
        self.a_up = A("BÜYÜK HARF (TR)", lambda: self._tool("Büyük harf", lambda ev: tools.change_case(ev, "upper")))
        self.a_low = A("küçük harf (TR)", lambda: self._tool("Küçük harf", lambda ev: tools.change_case(ev, "lower")))
        self.a_sent = A("Cümle düzeni (TR)", lambda: self._tool("Cümle", lambda ev: tools.change_case(ev, "sentence")))
        self.a_open_video = A("Video aç…", self.open_video_dialog, "Ctrl+Shift+O")
        self.a_open_audio = A("Ses aç (dosyadan)…", self.open_audio_dialog)
        self.a_audio_from_video = A("Videodan sesi yükle", self.load_audio_from_video)
        self.a_load_kf = A("Anahtar kareleri yükle (dosya)…", self.load_keyframe_file)
        self.a_scan_kf = A("Videodan anahtar kareleri tara", self.scan_keyframes)
        self.a_fps = A("FPS ayarla…", self.set_fps_dialog)
        self.a_tc = A("Zaman kodlarını yükle (VFR v1/v2)…", self.load_timecodes_file)
        self.a_play = A("Oynat/Duraklat", self.video.toggle, "Ctrl+Space")
        self.a_play_line = A("Aktif satırı oynat", self.play_line, "Ctrl+L")
        self.a_prev_frame = A("Önceki kare", lambda: self.video.step(-1), "Alt+Left")
        self.a_next_frame = A("Sonraki kare", lambda: self.video.step(1), "Alt+Right")
        self.a_prev_line = A("Önceki satır", lambda: self.move_row(-1), "Alt+Up")
        self.a_next_line = A("Sonraki satır", lambda: self.move_row(1), "Alt+Down")
        self.a_spectrum = A("Spektrogram", self._toggle_spectrum, "Ctrl+Shift+W", checkable=True)
        self.a_snap_opt = A("Yapışma (anahtar kare/satır kenarı)", lambda v: setattr(self.wave, "snap", v), checkable=True, checked=True)
        self.a_follow = A("Oynatmayı takip et", lambda v: setattr(self.wave, "follow", v), checkable=True, checked=True)
        self.a_plain = A("Metin sütununda etiketleri gizle", self._toggle_plain, checkable=True)
        self.a_dark = A("Koyu tema", self._toggle_theme, checkable=True, checked=self.dark)
        self.a_reload = A("Betikleri yeniden yükle", self.reload_scripts, "Ctrl+Shift+R")
        self.a_manager = A("Otomasyon yöneticisi…", self.show_manager)
        self.a_adddir = A("Betik klasörü ekle…", self.add_script_dir)
        self.a_about = A("SubForge hakkında", self.about)
        self.a_fonts = A("Yazı tipleri / topla / göm…", lambda: FontsDialog(self).exec())
        self.a_attach = A("Ekler…", lambda: AttachmentsDialog(self).exec())
        self.a_stats = A("İstatistikler…", lambda: StatsDialog(self.project, self).exec())
        self.a_shortcuts = A("Kısayol tuşları…", lambda: ShortcutsDialog(self).exec())
        self.a_export_filters = A("Filtreli dışa aktar (Auto4)…", self.export_with_filters)
        self.a_goto = A("Satıra git…", self.goto_dialog, "Ctrl+G")
        # global typing-safe shortcuts
        for key, fn in (("Space", self._space), (",", lambda: self._if_not_typing(lambda: self.video.step(-1))),
                        (".", lambda: self._if_not_typing(lambda: self.video.step(1)))):
            sc = QShortcut(QKeySequence(key), self)
            sc.activated.connect(fn)
        sc = QShortcut(QKeySequence("Return"), self.wave, context=Qt.ShortcutContext.WidgetShortcut)
        sc.activated.connect(self.apply_range)
        sc = QShortcut(QKeySequence("Shift+Return"), self.wave, context=Qt.ShortcutContext.WidgetShortcut)
        sc.activated.connect(self.new_from_range)

    def shortcut_actions(self) -> dict:
        return {k: a for k, a in self._named_actions.items()}

    def _init_shortcuts(self):
        self._named_actions = {}
        for k, a in vars(self).items():
            if k.startswith("a_") and isinstance(a, QAction):
                a.setObjectName(k)
                self._named_actions[k] = a
        self.default_shortcuts = {k: a.shortcut().toString() for k, a in self._named_actions.items()}
        for k, a in self._named_actions.items():
            v = self.settings.value("shortcut/" + k, None)
            if v is not None:
                a.setShortcut(QKeySequence(v))

    def set_shortcut(self, key, seq):
        a = self._named_actions.get(key)
        if a is not None:
            a.setShortcut(QKeySequence(seq))
            self.settings.setValue("shortcut/" + key, seq)

    def export_with_filters(self):
        d = ExportFiltersDialog(self)
        if not d.exec():
            return
        chosen = d.chosen()
        p, _ = QFileDialog.getSaveFileName(self, "Filtreli dışa aktar", (os.path.splitext(self.project.path or "out")[0]) + "_export.ass",
                                           "ASS (*.ass)")
        if not p:
            return
        import copy, dataclasses
        from ..core.ass import ASSFile
        doc = ASSFile(lines=[dataclasses.replace(l) for l in self.project.doc.lines])
        pr = self.project
        info = ProjectInfo(fps=pr.fps, timecodes=pr.timecodes, video_size=pr.video_size, video_file=pr.video_path,
                           audio_file=pr.audio_path, script_path=pr.path or "")
        ui = QtUI(self, "Dışa aktar")
        host = Host(doc, [], 0, ui, info)
        try:
            for f in chosen:
                cfg = f.get_config(host)
                if cfg is None:
                    return
                f.run(host, cfg)
        except (ScriptError, CancelledError) as e:
            QMessageBox.warning(self, "Filtre", str(e) or "İptal edildi")
            return
        finally:
            ui.finish()
        with open(p, "w", encoding="utf-8-sig", newline="") as fh:
            fh.write(doc.to_string())
        self.statusBar().showMessage(f"Dışa aktarıldı: {p}", 5000)

    def _typing(self):
        w = QApplication.focusWidget()
        from PySide6.QtWidgets import QLineEdit, QPlainTextEdit, QSpinBox, QComboBox
        return isinstance(w, (QLineEdit, QPlainTextEdit, QSpinBox)) or (isinstance(w, QComboBox) and w.isEditable())

    def _if_not_typing(self, fn):
        if not self._typing():
            fn()

    def _space(self):
        self._if_not_typing(self.video.toggle)

    def _build_menus(self):
        mb = self.menuBar()
        m = mb.addMenu("&Dosya")
        for a in (self.a_new, self.a_open):
            m.addAction(a)
        self.recent_menu = m.addMenu("Son açılanlar")
        m.addSeparator()
        for a in (self.a_save, self.a_saveas, self.a_export):
            m.addAction(a)
        m.addAction(self.a_export_filters)
        m.addSeparator(); m.addAction(self.a_quit)
        m = mb.addMenu("D&üzen")
        for a in (self.a_undo, self.a_redo, None, self.a_cut, self.a_copy, self.a_paste, self.a_selall, None, self.a_ins_b,
                  self.a_ins_a, self.a_dup, self.a_del, None, self.a_join, self.a_split, self.a_comment, None, self.a_goto,
                  self.a_find, self.a_sort):
            m.addAction(a) if a else m.addSeparator()
        m = mb.addMenu("&Altyazı")
        for a in (self.a_styles, self.a_props, self.a_fonts, self.a_attach):
            m.addAction(a)
        m = mb.addMenu("&Zamanlama")
        for a in (self.a_shift, self.a_timing, self.a_sync, self.a_fpschg, None, self.a_bridge, self.a_overlap, self.a_snap,
                  None, self.a_set_start, self.a_set_end, self.a_apply_range, self.a_new_range):
            m.addAction(a) if a else m.addSeparator()
        m = mb.addMenu("A&raçlar")
        for a in (self.a_qc, self.a_stats, self.a_karaoke, self.a_select, self.a_moji, self.a_fix, self.a_balance, self.a_hi, self.a_dups):
            m.addAction(a)
        mc = m.addMenu("Harf büyüklüğü")
        for a in (self.a_up, self.a_low, self.a_sent):
            mc.addAction(a)
        m = mb.addMenu("&Medya")
        for a in (self.a_open_video, self.a_open_audio, self.a_audio_from_video, None, self.a_load_kf, self.a_scan_kf,
                  self.a_fps, self.a_tc, None, self.a_play, self.a_play_line, self.a_prev_frame, self.a_next_frame, self.a_prev_line,
                  self.a_next_line):
            m.addAction(a) if a else m.addSeparator()
        self.auto_menu = mb.addMenu("&Otomasyon")
        m = mb.addMenu("&Görünüm")
        for a in (self.a_spectrum, self.a_snap_opt, self.a_follow, self.a_plain, self.a_dark, self.a_shortcuts):
            m.addAction(a)
        m.addSeparator()
        for d in self.docks.values():
            m.addAction(d.toggleViewAction())
        m = mb.addMenu("&Yardım")
        m.addAction(self.a_about)
        self._refresh_recent()

    # ------------------------------------------------------------------ wiring
    def _wire(self):
        p = self.project
        sm = self.grid.selectionModel()
        sm.currentRowChanged.connect(self._current_changed)
        sm.selectionChanged.connect(lambda *_: self._selection_changed())
        self.model.modelAboutToBeReset.connect(self._save_sel)
        self.model.modelReset.connect(self._restore_sel)
        p.reset.connect(self._after_reset)
        p.dirtyChanged.connect(lambda _: self._update_title())
        p.undoChanged.connect(self._update_undo_actions)
        p.stylesChanged.connect(self._refresh_styles)
        p.reset.connect(lambda: self._qc_timer.start())
        p.rowsChanged.connect(lambda a, b: self._qc_timer.start())
        self.edit.textChanged.connect(self._text_edited)
        self.edit.fieldChanged.connect(self._field_edited)
        self.edit.timeEdited.connect(self._time_edited)
        self.edit.nextLine.connect(lambda: self.move_row(1, focus_text=True))
        self.video.tagEdit.connect(self._tag_edit)
        self.video.tagEditDone.connect(self._load_active)
        self.video.anchor_provider = self._anchor
        self.video.positionChanged.connect(self.wave.set_play_ms)
        self.video.positionChanged.connect(lambda ms: self.lbl_info.setText(self._info_text(ms)))
        self.video.loaded.connect(lambda path: None)
        self.wave.seekRequested.connect(self.video.set_position)
        self.wave.rowClicked.connect(self.select_row)
        self.wave.boundaryDragging.connect(self._boundary_drag)
        self.wave.boundaryCommitted.connect(self._boundary_commit)
        self.wave.rangeCreated.connect(self._range_created)
        self._update_undo_actions()

    def _apply_dark_to_widgets(self):
        self.wave.dark = self.dark
        self.model.dark = self.dark
        self.edit.hl.set_dark(self.dark)
        self.wave.update(); self.grid.viewport().update()

    # ------------------------------------------------------------------ state helpers
    def selected_rows(self) -> list[int]:
        return sorted({i.row() for i in self.grid.selectionModel().selectedRows()})

    def active_row(self) -> int:
        i = self.grid.currentIndex()
        return i.row() if i.isValid() else -1

    def active_event(self):
        r = self.active_row()
        return self.project.events[r] if 0 <= r < len(self.project.events) else None

    def select_row(self, row: int, seek=False, center=True):
        if not (0 <= row < self.model.rowCount()):
            return
        idx = self.model.index(row, 0)
        self.grid.selectionModel().setCurrentIndex(idx, self.grid.selectionModel().SelectionFlag.ClearAndSelect
                                                   | self.grid.selectionModel().SelectionFlag.Rows)
        self.grid.scrollTo(idx, QAbstractItemView.ScrollHint.EnsureVisible)

    def goto_row(self, row: int):
        self.select_row(row)

    def set_selection(self, rows):
        from PySide6.QtCore import QItemSelection, QItemSelectionModel
        sel = QItemSelection()
        for r in rows:
            if 0 <= r < self.model.rowCount():
                sel.select(self.model.index(r, 0), self.model.index(r, len(COLS) - 1))
        self.grid.selectionModel().select(sel, QItemSelectionModel.SelectionFlag.ClearAndSelect)
        if rows:
            r0 = min(max(rows[0], 0), self.model.rowCount() - 1)
            self.grid.selectionModel().setCurrentIndex(self.model.index(r0, 0), QItemSelectionModel.SelectionFlag.NoUpdate)

    def _refresh_all(self):
        self._refresh_styles()
        self._update_title()
        self._selection_changed()
        self._update_media_label()

    def _refresh_styles(self):
        self.edit.set_styles([s.name for s in self.project.doc.styles])

    def _save_sel(self):
        self._saved_sel = (self.selected_rows(), self.active_row())

    def _restore_sel(self):
        rows, cur = getattr(self, "_saved_sel", ([], -1))
        n = self.model.rowCount()
        rows = [r for r in rows if r < n]
        if rows:
            self.set_selection(rows)
        if 0 <= cur < n:
            from PySide6.QtCore import QItemSelectionModel
            self.grid.selectionModel().setCurrentIndex(self.model.index(cur, 0), QItemSelectionModel.SelectionFlag.NoUpdate)

    def _after_reset(self):
        self._refresh_styles()
        self._load_active()
        self.wave.selected_rows = set(self.selected_rows())
        self.wave.active_row = self.active_row()
        self.wave.update()
        self.lbl_info.setText(self._info_text(self.video.position()))

    def _update_title(self):
        self.setWindowTitle(f"{'*' if self.project.dirty else ''}{self.project.title} — {APP_NAME} {__version__}")

    def _update_undo_actions(self):
        p = self.project
        self.a_undo.setEnabled(p.can_undo()); self.a_redo.setEnabled(p.can_redo())
        self.a_undo.setText("&Geri al" + (f" ({p.undo_label()})" if p.can_undo() else ""))
        self.a_redo.setText("&Yinele" + (f" ({p.redo_label()})" if p.can_redo() else ""))

    def _info_text(self, ms: int) -> str:
        n = len(self.project.events)
        frame = f" · kare {self.project.frame_at(ms)}" if self.project.fps else ""
        return f"{n} satır · {len(self.selected_rows())} seçili · {ms_to_ass(ms)}{frame}"

    def _update_media_label(self):
        p = self.project
        bits = []
        if p.video_path:
            bits.append(os.path.basename(p.video_path))
        if p.fps:
            bits.append(f"{p.fps:.3f} fps" + (" VFR" if p.timecodes and p.timecodes.is_vfr else ""))
        if p.keyframes:
            bits.append(f"{len(p.keyframes)} anahtar kare")
        self.lbl_media.setText(" · ".join(bits))

    # ------------------------------------------------------------------ selection / edit panel
    def _load_active(self):
        e = self.active_event()
        self.edit.load(e, self.project.frame_ms)
        self.wave.active_row = self.active_row()

    def _current_changed(self, cur, prev):
        self._load_active()
        self.reference.refresh()
        e = self.active_event()
        if e is not None:
            self.wave.ensure_visible(e.start_time, e.end_time)
            if not self.video.is_playing():
                self.video.set_position(e.start_time)
        self.wave.update()

    def _selection_changed(self):
        self.wave.selected_rows = set(self.selected_rows())
        self.wave.update()
        self.lbl_info.setText(self._info_text(self.video.position()))

    def _text_edited(self, txt):
        r = self.active_row()
        ev = self.project.events
        if not (0 <= r < len(ev)) or ev[r].text == txt:
            return
        with self.project.edit("Metin düzenle", merge_key=("text", id(ev[r])), structural=False):
            ev[r].text = txt
        self.project.rowsChanged.emit(r, r)
        self.edit._update_info(ev[r])

    def _anchor(self):
        e = self.active_event()
        px, py = self.project.doc.play_res
        if e is not None:
            pos = get_override(e.text, "pos")
            frz = get_override(e.text, "frz") or get_override(e.text, "fr") or "0"
            try:
                x, y = [float(v) for v in pos.strip("()").split(",")[:2]] if pos else (px / 2, py * 0.9)
                return x, y, float(frz)
            except ValueError:
                pass
        return px / 2, py * 0.9, 0.0

    def _tag_edit(self, tag, value, final):
        rows = self.selected_rows() or ([self.active_row()] if self.active_row() >= 0 else [])
        ev = self.project.events
        rows = [r for r in rows if 0 <= r < len(ev)]
        if not rows:
            return
        with self.project.edit("Görsel dizgi", merge_key=("vis", tag, tuple(rows)), structural=False):
            for r in rows:
                ev[r].text = set_override(ev[r].text, tag, value)
        self.project.rowsChanged.emit(min(rows), max(rows))

    def _field_edited(self, name, value):
        rows = self.selected_rows() or ([self.active_row()] if self.active_row() >= 0 else [])
        ev = self.project.events
        rows = [r for r in rows if 0 <= r < len(ev)]
        if not rows:
            return
        with self.project.edit(f"{name} değiştir", merge_key=(name, tuple(rows)), structural=False):
            for r in rows:
                setattr(ev[r], name, value)
        self.project.rowsChanged.emit(min(rows), max(rows))

    def _time_edited(self, which, ms):
        r = self.active_row()
        ev = self.project.events
        if not (0 <= r < len(ev)):
            return
        e = ev[r]
        with self.project.edit("Zaman düzenle", structural=False):
            if which == "start":
                e.start_time = ms
                if e.end_time < ms:
                    e.end_time = ms
            elif which == "end":
                e.end_time = max(ms, e.start_time)
            else:
                e.end_time = e.start_time + ms
        self.edit.refresh_times(e)
        self.project.rowsChanged.emit(r, r)
        self.wave.update()

    # ------------------------------------------------------------------ waveform interactions
    def _boundary_drag(self, row, which, ms):
        ev = self.project.events
        if not (0 <= row < len(ev)):
            return
        if self._drag_snapshot is None:
            self._drag_snapshot = self.project.snapshot()
        e = ev[row]
        if which == "start":
            e.start_time = min(ms, e.end_time)
        else:
            e.end_time = max(ms, e.start_time)
        if row == self.active_row():
            self.edit.refresh_times(e)
        self.project.rowsChanged.emit(row, row)
        self.wave.update()

    def _boundary_commit(self, row, which, ms):
        self._boundary_drag(row, which, ms)
        if self._drag_snapshot is not None:
            self.project.commit_external("Zamanlama (sürükle)", self._drag_snapshot)
            self._drag_snapshot = None

    def _range_created(self, a, b):
        self._pending_range = (a, b)
        self.statusBar().showMessage(
            f"Aralık {ms_to_ass(a)} – {ms_to_ass(b)} : Enter = aktif satıra uygula, Shift+Enter = yeni satır", 8000)

    def apply_range(self):
        if not self._pending_range:
            return
        e = self.active_event()
        if e is None:
            return self.new_from_range()
        a, b = self._pending_range
        with self.project.edit("Aralığı uygula", structural=False):
            e.start_time, e.end_time = a, b
        self.project.rowsChanged.emit(self.active_row(), self.active_row())
        self.edit.refresh_times(e)
        self._pending_range = None

    def new_from_range(self):
        if not self._pending_range:
            return
        a, b = self._pending_range
        r = self.active_row()
        style = self.active_event().style if self.active_event() else "Default"
        ne = Event(start_time=a, end_time=b, style=style)
        self._insert_events([ne], after_row=r if r >= 0 else len(self.project.events) - 1, label="Yeni satır")
        self._pending_range = None
        self.edit.text.setFocus()

    # ------------------------------------------------------------------ document ops
    def _do(self, label, fn):
        with self.project.edit(label) as d:
            n = fn(d)
        self.statusBar().showMessage(f"{label}: {n if n is not None else ''}", 4000)

    def _tool(self, label, fn, rows=None):
        rows = rows if rows is not None else (self.selected_rows() if len(self.selected_rows()) > 1 else None)
        sel = self.project.events
        evs = [sel[r] for r in rows] if rows else sel
        keep = self.selected_rows()
        with self.project.edit(label):
            n = fn(evs)
        self.set_selection(keep)
        self.statusBar().showMessage(f"{label}: {n} satır değişti", 4000)

    def _insert_events(self, new_events, after_row, label="Satır ekle"):
        lines = self.project.doc.lines
        evs = self.project.events
        if after_row >= 0 and evs:
            idx = next(i for i, l in enumerate(lines) if l is evs[min(after_row, len(evs) - 1)]) + 1
        else:
            idx = len(lines)
        with self.project.edit(label):
            lines[idx:idx] = new_events
        evs = self.project.events
        first = next(i for i, e in enumerate(evs) if e is new_events[0])
        self.set_selection(list(range(first, first + len(new_events))))
        self.select_row(first)

    def insert_line(self, after=True):
        r = self.active_row()
        e = self.active_event()
        if e is None:
            ne = Event(start_time=0, end_time=2000)
            return self._insert_events([ne], -1)
        if after:
            ne = Event(start_time=e.end_time, end_time=e.end_time + 2000, style=e.style, layer=e.layer)
        else:
            ne = Event(start_time=max(0, e.start_time - 2000), end_time=e.start_time, style=e.style, layer=e.layer)
        self._insert_events([ne], r if after else r - 1, "Satır ekle")

    def duplicate_lines(self):
        rows = self.selected_rows()
        if not rows:
            return
        copies = [self.project.events[r].copy() for r in rows]
        self._insert_events(copies, rows[-1], "Çoğalt")

    def delete_lines(self):
        rows = self.selected_rows()
        if not rows:
            return
        ev = self.project.events
        kill = {id(ev[r]) for r in rows}
        with self.project.edit("Sil"):
            self.project.doc.lines = [l for l in self.project.doc.lines if id(l) not in kill]
        self.select_row(min(rows[0], self.model.rowCount() - 1))

    def join_next(self):
        r = self.active_row()
        ev = self.project.events
        if 0 <= r < len(ev) - 1:
            with self.project.edit("Birleştir"):
                tools.merge_events(self.project.doc, ev[r], ev[r + 1])
            self.select_row(r)

    def split_at_cursor(self):
        e = self.active_event()
        if e is None:
            return
        pos = self.edit.text.textCursor().position()
        raw = e.text
        # translate raw cursor offset to plain char offset (tags excluded)
        from ..core.tags import split_blocks
        plain_off, seen = 0, 0
        for is_tag, part in split_blocks(raw):
            if seen + len(part) >= pos:
                plain_off += 0 if is_tag else pos - seen
                break
            seen += len(part)
            plain_off += 0 if is_tag else len(part)
        r = self.active_row()
        with self.project.edit("Böl"):
            tools.split_event(self.project.doc, e, at_char=plain_off)
        self.select_row(r)

    def toggle_comment(self):
        rows = self.selected_rows()
        if rows:
            ev = self.project.events
            new = not ev[rows[0]].comment
            with self.project.edit("Yorum", structural=False):
                for r in rows:
                    ev[r].comment = new
            self.project.rowsChanged.emit(min(rows), max(rows))
            self._load_active()

    def copy_lines(self):
        rows = self.selected_rows()
        ev = self.project.events
        txt = "\n".join(ev[r].to_ass() for r in rows)
        QGuiApplication.clipboard().setText(txt)

    def cut_lines(self):
        self.copy_lines()
        self.delete_lines()

    def paste_lines(self):
        from ..core.ass import parse_ass
        txt = QGuiApplication.clipboard().text()
        lines = [l for l in txt.splitlines() if l.startswith(("Dialogue:", "Comment:"))]
        if not lines:
            return
        d = parse_ass("[Events]\n" + "\n".join(lines))
        r = self.active_row()
        self._insert_events(d.events, r if r >= 0 else len(self.project.events) - 1, "Yapıştır")

    def move_row(self, delta, focus_text=False):
        r = self.active_row()
        n = self.model.rowCount()
        if n == 0:
            return
        nr = min(max(r + delta, 0), n - 1) if r >= 0 else 0
        if nr == r and delta > 0 and focus_text:
            self.insert_line(True)
            nr = self.active_row()
        self.select_row(nr)
        if focus_text:
            self.edit.text.setFocus()

    def goto_dialog(self):
        n, ok = QInputDialog.getInt(self, "Satıra git", "Satır no:", self.active_row() + 1, 1, max(1, self.model.rowCount()))
        if ok:
            self.select_row(n - 1)

    def _grid_menu(self, pos):
        m = QMenu(self)
        for a in (self.a_ins_b, self.a_ins_a, self.a_dup, self.a_del, None, self.a_copy, self.a_cut, self.a_paste, None,
                  self.a_join, self.a_split, self.a_comment, None, self.a_shift):
            m.addAction(a) if a else m.addSeparator()
        m.exec(self.grid.viewport().mapToGlobal(pos))

    # ------------------------------------------------------------------ dialogs
    def show_find(self):
        if not self._find_dlg:
            self._find_dlg = FindReplaceDialog(self)
        self._find_dlg.show(); self._find_dlg.raise_(); self._find_dlg.find.setFocus()

    def show_styles(self):
        StylesDialog(self.project, self).exec()

    def show_props(self):
        d = PropertiesDialog(self.project, self)
        if d.exec():
            d.apply()

    def show_shift(self):
        d = ShiftDialog(self, self.project.fps, len(self.selected_rows()))
        if d.exec():
            ms, which, scope = d.values()
            ev = self.project.events
            rows = self.selected_rows()
            if scope == 1:
                sel = [ev[r] for r in rows]
            elif scope == 2 and rows:
                sel = ev[rows[0]:]
            else:
                sel = ev
            keep = rows
            with self.project.edit("Zamanları kaydır"):
                tools.shift(sel, ms, which)
            self.set_selection(keep)

    def show_timing(self):
        d = TimingDialog(self, len(self.selected_rows()), bool(self.project.keyframes))
        if d.exec():
            ev = self.project.events
            sel = [ev[r] for r in self.selected_rows()] if d.scope.currentIndex() == 1 else ev
            keep = self.selected_rows()
            with self.project.edit("Zamanlama son işlemcisi"):
                res = d.run(sel, self.project.keyframes)
            self.set_selection(keep)
            self.statusBar().showMessage("Zamanlama: " + ", ".join(f"{k}={v}" for k, v in res.items()), 6000)

    def show_sync(self):
        ev = sorted(self.project.events, key=lambda e: e.start_time)
        if len(ev) < 2:
            return
        d = SyncDialog(self, ev[0].start_time, ev[-1].start_time)
        if d.exec():
            try:
                with self.project.edit("İki noktalı senkron"):
                    tools.two_point_sync(self.project.events, *d.values())
            except ValueError as e:
                QMessageBox.warning(self, "Senkron", str(e))

    def show_fps_change(self):
        d = FpsDialog(self, "FPS değiştir (zamanları ölçekle)")
        if d.exec():
            a, b = d.values()
            with self.project.edit("FPS değiştir"):
                tools.change_framerate(self.project.events, a, b)

    def show_karaoke(self):
        if self.active_event() is None:
            return
        KaraokeDialog(self).exec()

    def show_select(self):
        d = SelectDialog(self)
        d.show(); d.raise_()
        self._select_dlg = d

    def show_qc(self):
        if not self._qc_dlg:
            self._qc_dlg = QCDialog(self.project, self)
            self._qc_dlg.jumpTo.connect(self.select_row)
        self._qc_dlg.show(); self._qc_dlg.raise_()
        self._qc_dlg.run()

    def _live_qc(self):
        if len(self.project.events) > 30000:
            return
        s = self._qc_dlg.settings() if self._qc_dlg else checks.QCSettings()
        issues = checks.check(self.project.doc, s)
        worst = {}
        for i in issues:
            if worst.get(i.index) != "error":
                worst[i.index] = i.severity if i.severity != "info" else worst.get(i.index, "info")
        self.model.issues = {k: v for k, v in worst.items() if v in ("error", "warning")}
        self.model.max_cps = s.max_cps
        self.model.dataChanged.emit(self.model.index(0, 0), self.model.index(max(0, self.model.rowCount() - 1), len(COLS) - 1))
        e = sum(i.severity == "error" for i in issues)
        w = sum(i.severity == "warning" for i in issues)
        self.lbl_qc.setText(f"⚠ {w}  ✖ {e}" if (e or w) else "✔ temiz")

    # ------------------------------------------------------------------ file ops
    def _confirm_discard(self) -> bool:
        if not self.project.dirty:
            return True
        r = QMessageBox.question(self, APP_NAME, "Kaydedilmemiş değişiklikler var. Kaydedilsin mi?",
                                 QMessageBox.StandardButton.Save | QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel)
        if r == QMessageBox.StandardButton.Save:
            return self.save()
        return r == QMessageBox.StandardButton.Discard

    def new_file(self):
        if self._confirm_discard():
            self.project.new()

    def open_dialog(self):
        if not self._confirm_discard():
            return
        p, _ = QFileDialog.getOpenFileName(self, "Altyazı aç", self.settings.value("last_dir", ""), OPEN_FILTER)
        if p:
            self.open_path(p, check=False)

    def open_path(self, path, check=True):
        if check and not self._confirm_discard():
            return
        try:
            fmt, enc, warns = self.project.open(path)
        except Exception as e:
            QMessageBox.critical(self, "Açılamadı", f"{path}\n\n{e}")
            return
        self.settings.setValue("last_dir", os.path.dirname(path))
        self._add_recent(path)
        msg = f"{os.path.basename(path)} açıldı ({fmt}, {enc}, {len(self.project.events)} satır)"
        if warns:
            msg += f" — {len(warns)} uyarı"
        self.statusBar().showMessage(msg, 6000)
        self.project.video_size = None
        self._refresh_all()
        if self.model.rowCount():
            self.select_row(0)
        # restore media hints from the file's own header (Aegisub project garbage)
        vf = self.project.doc.get_info("Video File") or self._garbage("Video File")
        if vf:
            vp = vf if os.path.isabs(vf) else os.path.join(os.path.dirname(path), vf)
            if vp.startswith("?"):
                vp = ""
            if vp and os.path.exists(vp) and not self.project.video_path:
                self.open_video(vp)

    def _garbage(self, key):
        from ..core.ass import Info, S_GARBAGE
        for l in self.project.doc.lines:
            if isinstance(l, Info) and l.section == S_GARBAGE and l.key.lower() == key.lower():
                return l.value
        return ""

    def save(self) -> bool:
        if not self.project.path:
            return self.save_as()
        return self._write(self.project.path)

    def save_as(self) -> bool:
        src = self.project.path or (os.path.splitext(self.project.source)[0] + ".ass" if self.project.source else "untitled.ass")
        p, _ = QFileDialog.getSaveFileName(self, "Farklı kaydet", src, "ASS (*.ass)")
        if not p:
            return False
        if not p.lower().endswith((".ass", ".ssa")):
            p += ".ass"
        return self._write(p)

    def _write(self, path) -> bool:
        try:
            self.project.save(path)
        except Exception as e:
            QMessageBox.critical(self, "Kaydedilemedi", str(e))
            return False
        self._add_recent(path)
        self._update_title()
        self.statusBar().showMessage("Kaydedildi: " + path, 4000)
        return True

    def export_dialog(self):
        base = os.path.splitext(self.project.path or "untitled")[0]
        p, flt = QFileDialog.getSaveFileName(self, "Dışa aktar", base + ".srt",
                                             "SubRip (*.srt);;WebVTT (*.vtt);;Düz metin (*.txt);;ASS (*.ass)")
        if p:
            try:
                self.project.save(p)
                self.statusBar().showMessage("Dışa aktarıldı: " + p, 4000)
            except Exception as e:
                QMessageBox.critical(self, "Hata", str(e))

    def _add_recent(self, p):
        rec = [x for x in self.settings.value("recent", [], type=list) if x != p][:9]
        self.settings.setValue("recent", [p] + rec)
        self._refresh_recent()

    def _refresh_recent(self):
        self.recent_menu.clear()
        for p in self.settings.value("recent", [], type=list):
            a = self.recent_menu.addAction(p)
            a.triggered.connect(lambda _=False, p=p: self.open_path(p))

    def _autosave(self):
        if self.project.dirty:
            try:
                os.makedirs(os.path.join(USER_DIR, "autosave"), exist_ok=True)
                name = os.path.splitext(self.project.title)[0] + ".autosave.ass"
                with open(os.path.join(USER_DIR, "autosave", name), "w", encoding="utf-8-sig", newline="") as fh:
                    fh.write(self.project.doc.to_string())
            except OSError:
                pass

    # ------------------------------------------------------------------ media
    def _run_bg(self, fn, done, busy_msg):
        self.statusBar().showMessage(busy_msg)
        w = Worker(fn, self)
        self._workers.append(w)

        def fin(res, err):
            self._workers.remove(w)
            self.statusBar().clearMessage()
            if err:
                QMessageBox.warning(self, APP_NAME, str(err))
            else:
                done(res)
        w.done.connect(fin)
        w.start()

    def open_video_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Video aç", self.settings.value("last_media_dir", ""), VIDEO_FILTER)
        if p:
            self.open_video(p)

    def open_video(self, path):
        self.settings.setValue("last_media_dir", os.path.dirname(path))
        pr = self.project
        pr.video_path = path
        if media.have_ffmpeg():
            try:
                info = media.probe(path)
                if info.has_video:
                    pr.video_size = (info.width, info.height)
                    pr.fps = info.fps or pr.fps
                pr.audio_path = path if info.has_audio else pr.audio_path
            except Exception as e:
                self.statusBar().showMessage(f"ffprobe hatası: {e}", 5000)
        self.video.load(path)
        self.edit.start.frame_ms = pr.frame_ms
        self._update_media_label()
        if pr.audio_path:
            self.load_audio(pr.audio_path)
        if media.have_ffmpeg() and pr.video_size:
            self.scan_keyframes()
            self._detect_vfr(path)
        # offer to match script resolution
        if pr.video_size and pr.doc.play_res != pr.video_size and not any(isinstance(l, Event) for l in pr.doc.lines):
            pr.doc.set_info("PlayResX", pr.video_size[0]); pr.doc.set_info("PlayResY", pr.video_size[1])

    def _detect_vfr(self, path):
        def work():
            ts = media.video_timecodes(path)
            return ts if media.is_vfr(ts) else None
        self._run_bg(work, self._vfr_ready, "Kare zamanları inceleniyor…")

    def _vfr_ready(self, ts):
        if ts:
            from ..core.timecodes import Timecodes
            self.project.set_timecodes(Timecodes.from_v2(ts))
            self.statusBar().showMessage("Değişken kare hızı (VFR) algılandı — zaman kodları kullanılıyor", 6000)
            self._update_media_label()

    def load_timecodes_file(self):
        p, _ = QFileDialog.getOpenFileName(self, "Zaman kodu dosyası (v1/v2)", "", "Tümü (*)")
        if p:
            from ..core.timecodes import Timecodes
            from ..core.io import read_text
            try:
                self.project.set_timecodes(Timecodes.from_text(read_text(p)[0]))
            except Exception as e:
                QMessageBox.warning(self, APP_NAME, f"Zaman kodları okunamadı: {e}")
                return
            self._update_media_label()

    def open_audio_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Ses aç", self.settings.value("last_media_dir", ""), VIDEO_FILTER)
        if p:
            self.project.audio_path = p
            self.load_audio(p)

    def load_audio_from_video(self):
        if self.project.video_path:
            self.load_audio(self.project.video_path)

    def load_audio(self, path):
        if not media.have_ffmpeg():
            QMessageBox.warning(self, APP_NAME, "ffmpeg/ffprobe bulunamadı — ses dalga formu için gerekli.")
            return
        self._run_bg(lambda: media.load_audio(path), self._audio_ready, "Ses çözülüyor…")

    def _audio_ready(self, audio):
        self.wave.set_audio(audio)
        self.statusBar().showMessage("Ses hazır", 3000)

    def scan_keyframes(self):
        if not self.project.video_path or not media.have_ffmpeg():
            return
        path = self.project.video_path
        self._run_bg(lambda: media.keyframes(path), self._kf_ready, "Anahtar kareler taranıyor…")

    def _kf_ready(self, kf):
        self.project.keyframes = kf
        self._update_media_label()
        self.wave.update()

    def load_keyframe_file(self):
        p, _ = QFileDialog.getOpenFileName(self, "Anahtar kare dosyası", "", "Tümü (*)")
        if p:
            self.project.keyframes = media.read_keyframe_file(p, self.project.fps)
            self._update_media_label(); self.wave.update()

    def set_fps_dialog(self):
        d = FpsDialog(self, "FPS", two=False, cur=self.project.fps)
        if d.exec():
            self.project.fps = d.values()[0]
            self._update_media_label()

    def play_line(self):
        e = self.active_event()
        if e:
            self.video.play_range(e.start_time, e.end_time)

    def _set_time(self, which):
        e = self.active_event()
        if e is None:
            return
        pos = self.video.position()
        tc = self.project.timecodes
        if tc is not None:
            f = tc.frame_at(pos)
            pos = tc.start_ms_for_frame(f) if which == "start" else tc.end_ms_for_frame(f)
        with self.project.edit("Zaman ayarla", structural=False):
            if which == "start":
                e.start_time = pos; e.end_time = max(e.end_time, pos)
            else:
                e.end_time = max(pos, e.start_time)
        self.edit.refresh_times(e)
        self.project.rowsChanged.emit(self.active_row(), self.active_row())

    def _toggle_spectrum(self, v):
        self.wave.spectrum_mode = v
        self.wave._spec_cache = (None, None)
        self.wave.update()

    def _toggle_plain(self, v):
        self.model.show_plain = v
        self.grid.viewport().update()

    def _toggle_theme(self, v):
        self.dark = v
        self.settings.setValue("dark", v)
        apply_theme(QApplication.instance(), v)
        self._apply_dark_to_widgets()

    # ------------------------------------------------------------------ automation
    def script_dirs(self) -> list[str]:
        bundled = os.path.join(os.path.dirname(os.path.dirname(__file__)), "automation", "autoload")
        dirs = [bundled, os.path.join(USER_DIR, "automation", "autoload"), REPO_SCRIPTS]
        dirs += [d for d in self.settings.value("script_dirs", [], type=list) if d]
        return dirs

    def reload_scripts(self):
        os.makedirs(os.path.join(USER_DIR, "automation", "autoload"), exist_ok=True)
        self.scripts = load_scripts(self.script_dirs(), [os.path.join(USER_DIR, "automation", "include")])
        self._build_automation_menu()

    def add_script_dir(self):
        d = QFileDialog.getExistingDirectory(self, "Lua betik klasörü")
        if d:
            dirs = self.settings.value("script_dirs", [], type=list)
            if d not in dirs:
                self.settings.setValue("script_dirs", dirs + [d])
            self.reload_scripts()

    def _build_automation_menu(self):
        m = self.auto_menu
        m.clear()
        m.addAction(self.a_manager); m.addAction(self.a_reload); m.addAction(self.a_adddir)
        m.addSeparator()
        n = 0
        for s in self.scripts:
            for mac in s.macros:
                parts = mac.name.split("/")
                menu = m
                for part in parts[:-1]:
                    sub = next((a.menu() for a in menu.actions() if a.menu() and a.text() == part), None)
                    menu = sub or menu.addMenu(part)
                a = menu.addAction(parts[-1])
                a.setStatusTip(mac.description)
                a.triggered.connect(lambda _=False, mac=mac: self.run_macro(mac))
                n += 1
        if not n:
            a = m.addAction("(yüklü makro yok)"); a.setEnabled(False)

    def _flat_selection(self):
        lines = self.project.doc.lines
        flat_of_row = [i + 1 for i, l in enumerate(lines) if isinstance(l, Event)]
        rows = self.selected_rows()
        sel = [flat_of_row[r] for r in rows if r < len(flat_of_row)]
        act = flat_of_row[self.active_row()] if 0 <= self.active_row() < len(flat_of_row) else (sel[0] if sel else 0)
        return sel, act

    def run_macro(self, macro, answers=None):
        sel, act = self._flat_selection()
        pr = self.project
        info = ProjectInfo(fps=pr.fps, timecodes=pr.timecodes, keyframes=[pr.frame_at(k) for k in pr.keyframes],
                           video_size=pr.video_size, video_file=pr.video_path, audio_file=pr.audio_path,
                           script_path=pr.path or "", video_position=pr.frame_at(self.video.position()))
        ui = QtUI(self, macro.name)
        host = Host(pr.doc, sel, act, ui, info)
        try:
            ok, msg = macro.validate(host)
        except (ScriptError, CancelledError) as e:
            ok, msg = False, str(e)
        if not ok:
            QMessageBox.information(self, macro.name, msg or "Makro bu seçimle çalıştırılamaz.")
            return
        snap = pr.snapshot()
        try:
            macro.run(host)
        except CancelledError:
            pr.doc.lines = snap
            ui.finish()
            self.statusBar().showMessage("Makro iptal edildi", 4000)
            self._show_log(ui, macro.name)
            return
        except ScriptError as e:
            pr.doc.lines = snap
            ui.finish()
            ui.log_lines.append(str(e) + "\n")
            self._show_log(ui, macro.name + " — HATA", force=True)
            return
        ui.finish()
        if host.modified:
            pr.commit_external(f"Makro: {macro.name}", snap)
        flat_rows, c = {}, 0
        for i, l in enumerate(pr.doc.lines, 1):
            if isinstance(l, Event):
                flat_rows[i] = c
                c += 1
        rows = [flat_rows[i] for i in host.selection if i in flat_rows]
        if rows:
            self.set_selection(rows)
        self.statusBar().showMessage(f"Makro tamamlandı: {macro.name}", 4000)
        self._show_log(ui, macro.name)

    def _show_log(self, ui, title, force=False):
        if ui.log_lines or force:
            d = LogWindow(self)
            d.setWindowTitle(f"Günlük — {title}")
            d.append("".join(ui.log_lines))
            d.exec()

    def show_manager(self):
        from .manager import ManagerDialog
        ManagerDialog(self).exec()

    def about(self):
        QMessageBox.about(self, APP_NAME, f"<b>{APP_NAME} {__version__}</b><br>Anime altyazı stüdyosu — Aegisub uyumlu "
                          "Lua (Auto4) otomasyonu, Subtitle Edit tarzı araçlar.<br>libass: "
                          f"{'var' if __import__('subforge.render', fromlist=['x']).available() else 'yok'}")


def run_gui(path=None) -> int:
    app = QApplication.instance() or QApplication(sys.argv)
    app.setApplicationName(APP_NAME)
    w = MainWindow(path)
    w.show()
    return app.exec()
