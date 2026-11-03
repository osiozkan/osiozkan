from __future__ import annotations
import os
import re
import time
import traceback
from typing import Optional

import numpy as np
from PySide6.QtCore import Qt, QTimer, QSettings, QThread, Signal, QObject, QUrl, QSize
from PySide6.QtGui import QAction, QKeySequence, QFont, QTextCursor, QActionGroup, QIcon, QShortcut
from PySide6.QtWidgets import (QMainWindow, QWidget, QSplitter, QTableView, QVBoxLayout, QHBoxLayout, QPlainTextEdit,
                               QLabel, QComboBox, QSpinBox, QCheckBox, QLineEdit, QPushButton, QFileDialog, QMessageBox,
                               QAbstractItemView, QHeaderView, QMenu, QToolButton, QSlider, QStatusBar, QApplication,
                               QInputDialog, QFormLayout, QGridLayout, QFrame)

from ..core import formats, tools
from ..core.ass import Document, Event, Style, save as ass_save
from ..core.history import History
from ..core.tags import strip_tags, char_count
from ..core.timeutil import FrameRate, ass_time
from ..lua.engine import Engine, AutomationError, Cancelled
from ..media import probe
from ..media.assrender import AssRenderer, available as libass_available
from .dialogs import (ShiftDialog, FindReplaceDialog, StylesManager, PropertiesDialog, TimingDialog, QCDialog,
                      AutomationDialog, TextDialog, FixCommonDialog, SettingsDialog)
from .host import GuiHost
from .model import SubsModel, COLS
from .widgets import TimeEdit, WaveformWidget, VideoView
from .visual import VisualTools, TOOLS

try:
    from PySide6.QtMultimedia import QMediaPlayer, QAudioOutput
    from PySide6.QtMultimediaWidgets import QGraphicsVideoItem
    HAVE_MM = True
except Exception:  # pragma: no cover
    HAVE_MM = False

APP = "Kaede"
SUPPORTED = "Altyazılar (*.ass *.ssa *.srt *.vtt *.sbv *.txt);;Tüm dosyalar (*)"
VIDEO_FILTER = "Medya (*.mkv *.mp4 *.avi *.webm *.mov *.m2ts *.ts *.mka *.flac *.wav *.mp3 *.ogg *.m4a *.aac);;Tüm dosyalar (*)"


class AudioWorker(QObject):
    done = Signal(object, int, str)

    def __init__(self, path): super().__init__(); self.path = path

    def run(self):
        try:
            rate = 8000
            samples = probe.decode_audio(self.path, rate)
            self.done.emit(probe.peaks(samples, rate, 100), rate, "")
            self.samples = samples
        except Exception as e:
            self.done.emit(None, 0, str(e))


class MainWindow(QMainWindow):
    def __init__(self, path: Optional[str] = None):
        super().__init__()
        self.settings = QSettings("Kaede", "Kaede")
        self.doc = Document(); self.doc.events = []
        self.path: Optional[str] = None
        self.dirty = False
        self.history = History()
        self.fr = FrameRate(float(self.settings.value("default_fps", 23.976023976023978)))
        self.video_path = ""; self.audio_samples = None
        self.keyframes: list[int] = []
        self.qc_cfg = tools.QCConfig(max_cps=float(self.settings.value("qc_cps", 20)))
        self.style_lib: list[Style] = []
        self.current_script_title = "Otomasyon"
        self._last_edit_key = (None, 0.0)
        self._updating = False
        self.renderer: Optional[AssRenderer] = None
        self.dark = self.settings.value("dark", "true") == "true"

        self.model = SubsModel(self.doc)
        self.host = GuiHost(self)
        self.engine = Engine(self.host)

        self._build_ui()
        self._build_menus()
        self._build_shortcuts()
        self.setAcceptDrops(True)
        self.resize(1400, 900)
        self.restoreGeometry(self.settings.value("geometry", b"") or b"")

        self.autosave = QTimer(self); self.autosave.timeout.connect(self._autosave)
        _m = int(self.settings.value('autosave_min', 2))
        if _m: self.autosave.start(_m * 60000)
        self.overlay_timer = QTimer(self); self.overlay_timer.setSingleShot(True)
        self.overlay_timer.timeout.connect(self._refresh_overlay)

        self.reload_scripts()
        if path:
            self.open_file(path)
        else:
            self.new_file()
        self._refresh_title()

    # ------------------------------------------------------------------ UI
    def _build_ui(self):
        self.setWindowTitle(APP)
        # grid
        self.grid = QTableView()
        self.grid.setModel(self.model)
        self.grid.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.grid.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.grid.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.grid.verticalHeader().setVisible(False)
        self.grid.verticalHeader().setDefaultSectionSize(22)
        self.grid.setAlternatingRowColors(True)
        self.grid.setWordWrap(False)
        h = self.grid.horizontalHeader()
        for i, w in enumerate((50, 55, 95, 95, 55, 50, 110, 90)):
            self.grid.setColumnWidth(i, w)
        h.setSectionResizeMode(8, QHeaderView.Stretch)
        self.grid.selectionModel().currentRowChanged.connect(self._on_current_changed)
        self.grid.selectionModel().selectionChanged.connect(lambda *_: self._update_waveform_spans())
        self.grid.setContextMenuPolicy(Qt.CustomContextMenu)
        self.grid.customContextMenuRequested.connect(self._grid_menu)
        self.grid.doubleClicked.connect(lambda idx: self.seek_to_line(idx.row()))

        # video
        self.video = VideoView()
        self.player = None
        if HAVE_MM:
            try:
                self.player = QMediaPlayer(self); self.audio_out = QAudioOutput(self); self.player.setAudioOutput(self.audio_out)
                self.vitem = QGraphicsVideoItem(); self.video.scene_.addItem(self.vitem); self.vitem.setZValue(1)
                self.player.setVideoOutput(self.vitem)
                self.vitem.nativeSizeChanged.connect(self._native_size)
                self.player.positionChanged.connect(self._on_position)
                self.player.playbackStateChanged.connect(lambda *_: self._sync_play_btn())
            except Exception:
                self.player = None
        self.pos_label = QLabel("0:00:00.00  (kare 0)")
        self.play_btn = QPushButton("▶"); self.play_btn.setFixedWidth(34); self.play_btn.clicked.connect(self.toggle_play)
        self.seek_slider = QSlider(Qt.Horizontal); self.seek_slider.setRange(0, 1000)
        self.seek_slider.sliderMoved.connect(lambda v: self.seek_ms(int(v / 1000 * self._duration_ms())))
        step_b = QPushButton("◀|"); step_f = QPushButton("|▶")
        step_b.setFixedWidth(34); step_f.setFixedWidth(34)
        step_b.clicked.connect(lambda: self.step_frame(-1)); step_f.clicked.connect(lambda: self.step_frame(1))
        vc = QHBoxLayout(); vc.setContentsMargins(0, 0, 0, 0)
        for w in (step_b, self.play_btn, step_f): vc.addWidget(w)
        vc.addWidget(self.seek_slider, 1); vc.addWidget(self.pos_label)
        self.visual = VisualTools(self)
        self.video.toolPress.connect(self.visual.press); self.video.toolMove.connect(self.visual.move)
        self.video.toolRelease.connect(self.visual.release)
        tb = QHBoxLayout(); tb.setContentsMargins(0, 0, 0, 0)
        from PySide6.QtWidgets import QButtonGroup
        self.tool_group = QButtonGroup(self)
        for key, label in TOOLS:
            b = QPushButton(label); b.setCheckable(True); b.setChecked(key == "none")
            b.setToolTip("Videoda sürükleyerek yazı dizgisi (Shift: kısıtla)")
            b.clicked.connect(lambda _=False, k=key: self.visual.set_mode(k)); self.tool_group.addButton(b); tb.addWidget(b)
        tb.addStretch(1)
        vbox = QWidget(); vl = QVBoxLayout(vbox); vl.setContentsMargins(0, 0, 0, 0); vl.addLayout(tb); vl.addWidget(self.video, 1); vl.addLayout(vc)

        # edit panel
        self.edit_text = QPlainTextEdit(); self.edit_text.setFont(QFont("Sans", 12))
        self.edit_text.setPlaceholderText("Altyazı metni… (Ctrl+Enter: uygula ve sonrakine geç)")
        self.edit_text.textChanged.connect(self._text_edited)
        self.style_box = QComboBox(); self.style_box.setEditable(False)
        self.style_box.activated.connect(lambda: self._field_edited("style", self.style_box.currentText()))
        self.actor_edit = QLineEdit(); self.actor_edit.editingFinished.connect(lambda: self._field_edited("actor", self.actor_edit.text()))
        self.effect_edit = QLineEdit(); self.effect_edit.editingFinished.connect(lambda: self._field_edited("effect", self.effect_edit.text()))
        self.layer_spin = QSpinBox(); self.layer_spin.setRange(0, 9999); self.layer_spin.valueChanged.connect(lambda v: self._field_edited("layer", v))
        self.ml, self.mr, self.mv = QSpinBox(), QSpinBox(), QSpinBox()
        for w, k in ((self.ml, "margin_l"), (self.mr, "margin_r"), (self.mv, "margin_t")):
            w.setRange(0, 9999); w.setMaximumWidth(64); w.valueChanged.connect(lambda v, k=k: self._field_edited(k, v))
        self.comment_chk = QCheckBox("Yorum"); self.comment_chk.toggled.connect(lambda v: self._field_edited("comment", v))
        self.t_start, self.t_end, self.t_dur = TimeEdit(), TimeEdit(), TimeEdit()
        self.t_start.timeChanged.connect(lambda ms: self._time_edited("start", ms))
        self.t_end.timeChanged.connect(lambda ms: self._time_edited("end", ms))
        self.t_dur.timeChanged.connect(lambda ms: self._time_edited("dur", ms))
        self.stats = QLabel("")
        tagbar = QHBoxLayout()
        for label, tag, tip in (("B", "b", "Kalın (Ctrl+B)"), ("I", "i", "İtalik (Ctrl+I)"), ("U", "u", "Altı çizili (Ctrl+U)"), ("S", "s", "Üstü çizili")):
            b = QToolButton(); b.setText(label); b.setToolTip(tip); b.setFixedWidth(28)
            f = b.font(); f.setBold(label == "B"); f.setItalic(label == "I"); f.setUnderline(label == "U"); f.setStrikeOut(label == "S"); b.setFont(f)
            b.clicked.connect(lambda _=False, t=tag: self.toggle_tag(t)); tagbar.addWidget(b)
        nb = QPushButton("\\N"); nb.setFixedWidth(34); nb.setToolTip("Satır sonu ekle (Shift+Enter)")
        nb.clicked.connect(lambda: self.edit_text.insertPlainText("\\N")); tagbar.addWidget(nb)
        tagbar.addStretch(1); tagbar.addWidget(self.stats)
        ep = QWidget(); el = QVBoxLayout(ep); el.setContentsMargins(4, 2, 4, 2)
        row1 = QHBoxLayout()
        for lab, w in (("Stil", self.style_box), ("Aktör", self.actor_edit), ("Katman", self.layer_spin), ("Efekt", self.effect_edit)):
            row1.addWidget(QLabel(lab)); row1.addWidget(w, 1 if isinstance(w, (QComboBox, QLineEdit)) else 0)
        row1.addWidget(self.comment_chk)
        row2 = QHBoxLayout()
        for lab, w in (("Başl.", self.t_start), ("Bitiş", self.t_end), ("Süre", self.t_dur), ("Kenar L", self.ml), ("R", self.mr), ("V", self.mv)):
            row2.addWidget(QLabel(lab)); row2.addWidget(w)
        row2.addStretch(1)
        self.ref_label = QLabel(""); self.ref_label.setWordWrap(True); self.ref_label.setVisible(False)
        self.ref_label.setStyleSheet("color:#8fb4ff; padding:2px 4px; border-left:3px solid #3d7eff")
        self.ref_doc = None
        el.addLayout(row1); el.addLayout(row2); el.addWidget(self.ref_label); el.addLayout(tagbar); el.addWidget(self.edit_text, 1)

        self.wave = WaveformWidget(); self.wave.dark = self.dark
        self.wave.seek.connect(self.seek_ms)
        self.wave.activeRetimed.connect(self._wave_retimed)
        self.wave.createRange.connect(self._wave_create)

        top = QSplitter(Qt.Horizontal); top.addWidget(vbox); top.addWidget(ep); top.setSizes([620, 780])
        mid = QSplitter(Qt.Vertical); mid.addWidget(top); mid.addWidget(self.wave); mid.addWidget(self.grid)
        mid.setSizes([380, 130, 380]); mid.setChildrenCollapsible(False)
        self.setCentralWidget(mid)
        self.setStatusBar(QStatusBar())
        self.find_dlg = None
        self.theme()

    def theme(self):
        app = QApplication.instance()
        if not app: return
        from .app import apply_theme
        apply_theme(app, self.dark)
        self.model.dark = self.dark
        self.wave.dark = self.dark; self.wave.update(); self.model.refresh()

    def _act(self, menu, text, fn, shortcut=None, checkable=False):
        a = QAction(text, self)
        if shortcut: a.setShortcut(QKeySequence(shortcut))
        a.setCheckable(checkable)
        a.triggered.connect(lambda _=False: fn())
        menu.addAction(a); return a

    def _build_menus(self):
        mb = self.menuBar()
        m = mb.addMenu("&Dosya")
        self._act(m, "Yeni", self.new_file, "Ctrl+N")
        self._act(m, "Aç…", self.open_dialog, "Ctrl+O")
        self.recent_menu = m.addMenu("Son açılanlar"); self._fill_recent()
        m.addSeparator()
        self._act(m, "Kaydet", self.save, "Ctrl+S")
        self._act(m, "Farklı Kaydet…", self.save_as, "Ctrl+Shift+S")
        ex = m.addMenu("Dışa Aktar")
        for ext, lab in (("srt", "SubRip (.srt)"), ("vtt", "WebVTT (.vtt)"), ("sbv", "YouTube (.sbv)"), ("txt", "Düz metin (.txt)")):
            self._act(ex, lab, lambda e=ext: self.export(e))
        self._act(m, "Etiketsiz SRT dışa aktar", lambda: self.export("srt", False))
        m.addSeparator(); self._act(m, "Çıkış", self.close, "Ctrl+Q")

        m = mb.addMenu("D&üzen")
        self.undo_act = self._act(m, "Geri Al", self.undo, "Ctrl+Z")
        self.redo_act = self._act(m, "Yinele", self.redo, "Ctrl+Shift+Z")
        m.addSeparator()
        self._act(m, "Önüne satır ekle", lambda: self.insert_line(False), "Ctrl+Insert")
        self._act(m, "Sonrasına satır ekle", lambda: self.insert_line(True), "Ctrl+Shift+Insert")
        self._act(m, "Çoğalt", self.duplicate, "Ctrl+D")
        self._act(m, "Sil", self.delete_lines, "Ctrl+Delete")
        self._act(m, "Birleştir (\\N ile)", lambda: self.join_lines("\\N"), "Ctrl+J")
        self._act(m, "Birleştir (boşlukla)", lambda: self.join_lines(" "))
        self._act(m, "İmleçten böl", self.split_at_cursor, "Ctrl+Shift+Return")
        self._act(m, "Yorum satırı yap/kaldır", self.toggle_comment, "Ctrl+/")
        m.addSeparator()
        self._act(m, "Bul / Değiştir…", self.show_find, "Ctrl+F")
        self._act(m, "Tümünü seç", self.grid.selectAll, "Ctrl+A")
        sm = m.addMenu("Sırala")
        for k, lab in (("start", "Başlangıca göre"), ("end", "Bitişe göre"), ("style", "Stile göre"), ("actor", "Aktöre göre"), ("layer", "Katmana göre")):
            self._act(sm, lab, lambda k=k: self.mutate(f"Sırala ({k})", lambda d, s: tools.sort_events(d, k)))
        cm = m.addMenu("Harf büyüklüğü (Türkçe)")
        for mode, lab in (("upper", "BÜYÜK HARF"), ("lower", "küçük harf"), ("title", "Her Sözcük Baş Harfi"), ("sentence", "Cümle düzeni")):
            self._act(cm, lab, lambda md=mode: self.change_case(md))

        m = mb.addMenu("&Zamanlama")
        self._act(m, "Zamanları Kaydır…", self.shift_dialog, "Ctrl+Shift+T")
        self._act(m, "Zamanlama Düzeltmeleri…", self.timing_dialog)
        self._act(m, "Karelere hizala", lambda: self.mutate("Karelere hizala", lambda d, s: tools.snap_to_frames(d, self.fr, s or None)))
        self._act(m, "Keyframe'e yapıştır", self.snap_kf)
        self._act(m, "Çakışmaları düzelt", lambda: self.mutate("Çakışmaları düzelt", lambda d, s: tools.fix_overlaps(d, s or None)))
        self._act(m, "Başlangıcı videoya ayarla", lambda: self.set_from_video("start"), "Ctrl+Shift+Left")
        self._act(m, "Bitişi videoya ayarla", lambda: self.set_from_video("end"), "Ctrl+Shift+Right")
        self._act(m, "Sesten konuşma tespiti (satır oluştur)", self.auto_vad)
        m.addSeparator()
        self._act(m, "İki noktalı senkron…", self.point_sync_dialog)
        self._act(m, "FPS dönüştür (zamanları ölçekle)…", self.change_fps_dialog)
        self._act(m, "Zaman kodu dosyası (VFR) yükle…", self.load_timecodes)
        self._act(m, "Keyframe dosyası yükle…", self.load_keyframes)
        self._act(m, "Keyframe dosyası kaydet…", self.save_keyframes)

        m = mb.addMenu("&Video")
        self._act(m, "Video/ses aç…", self.open_video_dialog, "Ctrl+Shift+O")
        self._act(m, "Videoyu kapat", self.close_video)
        self._act(m, "Kare hızını ayarla…", self.set_fps_dialog)
        self._act(m, "Oynat/Duraklat", self.toggle_play, "Ctrl+Space")
        self._act(m, "Satırı oynat", self.play_line, "Ctrl+L")
        self._act(m, "Önceki kare", lambda: self.step_frame(-1), "Alt+Left")
        self._act(m, "Sonraki kare", lambda: self.step_frame(1), "Alt+Right")
        self._act(m, "Önceki satır", lambda: self.goto_row(self.cur_row() - 1), "Alt+Up")
        self._act(m, "Sonraki satır", lambda: self.goto_row(self.cur_row() + 1), "Alt+Down")

        m = mb.addMenu("&Araçlar")
        self._act(m, "Kalite Kontrol (QC)…", self.qc_dialog, "F7")
        self._act(m, "Stil Yöneticisi…", self.styles_dialog, "Ctrl+Shift+S" if False else "F6")
        self._act(m, "Betik Özellikleri…", self.props_dialog, "F5")
        self._act(m, "Metni temizle (boşluk/noktalama)", lambda: self.mutate("Metni temizle", lambda d, s: tools.normalize_text(d, s or None)))
        self._act(m, "Boş satırları sil", lambda: self.mutate("Boş satırları sil", lambda d, s: tools.remove_empty(d)))
        self._act(m, "Etiketleri sil", lambda: self.mutate("Etiketleri sil", lambda d, s: tools.strip_all_tags(d, s or None)))
        m.addSeparator()
        self._act(m, "Satırları dengele (otomatik \\N)…", self.auto_balance_dialog)
        self._act(m, "Satır sonlarını kaldır", lambda: self.mutate("Satır sonlarını kaldır", lambda d, s: tools.unbreak(d, s or None)))
        self._act(m, "İşitme engelli metnini sil ([..] (..) AD:)", lambda: self.mutate("İE metni sil", lambda d, s: tools.remove_hi(d, s or None)))
        self._act(m, "Kısa ardışık satırları birleştir", lambda: self.mutate("Kısa satırları birleştir", lambda d, s: tools.merge_short(d, 250, 40, s or None), select_after=[]))
        self._act(m, "Yinelenen satırları sil", lambda: self.mutate("Yinelenenleri sil", lambda d, s: tools.remove_duplicates(d), select_after=[]))
        self._act(m, "İstatistikler…", self.show_stats)
        self._act(m, "Yazı tiplerini denetle / topla…", self.fonts_dialog)
        self._act(m, "Sık hataları düzelt…", self.fix_common_dialog)
        self._act(m, "Çeviri modu: referans altyazı yükle…", self.load_reference)
        self._act(m, "Çeviri modunu kapat", self.close_reference)
        self._act(m, "Satıra git…", self.goto_dialog, "Ctrl+G")
        m.addSeparator()
        self._act(m, "Ayarlar…", self.settings_dialog, "Ctrl+,")
        self._act(m, "Spektrogram göster/gizle", self.toggle_spectrogram, "Ctrl+Alt+S")

        self.auto_menu = mb.addMenu("&Otomasyon")
        self.auto_menu.aboutToShow.connect(self._fill_auto_menu)

        m = mb.addMenu("&Görünüm")
        a = self._act(m, "Koyu tema", self.toggle_theme, checkable=True); a.setChecked(self.dark)
        a = self._act(m, "Etiketleri tabloda göster", self.toggle_tags, checkable=True); a.setChecked(True)

        m = mb.addMenu("Y&ardım")
        self._act(m, "Klavye kısayolları", self.show_shortcuts, "F1")
        self._act(m, "Kaede hakkında", self.about)

    def _build_shortcuts(self):
        QShortcut(QKeySequence("Ctrl+Return"), self.edit_text, activated=self.commit_next)
        QShortcut(QKeySequence("Ctrl+B"), self.edit_text, activated=lambda: self.toggle_tag("b"))
        QShortcut(QKeySequence("Ctrl+I"), self.edit_text, activated=lambda: self.toggle_tag("i"))
        QShortcut(QKeySequence("Ctrl+U"), self.edit_text, activated=lambda: self.toggle_tag("u"))
        QShortcut(QKeySequence("Shift+Return"), self.edit_text, activated=lambda: self.edit_text.insertPlainText("\\N"))

    # ------------------------------------------------------------------ state helpers
    def cur_row(self) -> int:
        i = self.grid.currentIndex()
        return i.row() if i.isValid() else -1

    def sel_rows(self) -> list[int]:
        return sorted({i.row() for i in self.grid.selectionModel().selectedRows()})

    def _refresh_title(self):
        n = os.path.basename(self.path) if self.path else "Adsız"
        self.setWindowTitle(f"{'● ' if self.dirty else ''}{n} — {APP}")

    def set_doc(self, doc: Document, keep_sel=False):
        self.doc = doc; self.model.set_doc(doc)
        self._refresh_styles_box()
        self._setup_renderer_script()
        self._update_waveform_spans()

    def mark_dirty(self):
        self.dirty = True; self._refresh_title()

    def mutate(self, label: str, fn, select_after=None, merge_key=None):
        """Apply fn(doc, selection) with undo. fn may return None."""
        sel = self.sel_rows()
        self.history.push(label, self.doc, sel)
        r = fn(self.doc, sel)
        self.after_change(select_after if select_after is not None else sel)
        return r

    def after_change(self, sel=None, keep_current=True):
        row = self.cur_row()
        self.model.refresh()
        self._refresh_styles_box()
        if sel is not None:
            self.select_rows(sel, current=row if keep_current else None)
        self.mark_dirty()
        self._load_edit_panel()
        self._update_waveform_spans()
        self._schedule_overlay()

    def select_rows(self, rows, current=None):
        from PySide6.QtCore import QItemSelection, QItemSelectionModel
        n = len(self.doc.events)
        rows = [r for r in rows if 0 <= r < n]
        sm = self.grid.selectionModel(); sm.clearSelection()
        if not rows: return
        sel = QItemSelection()
        for r in rows:
            sel.select(self.model.index(r, 0), self.model.index(r, len(COLS) - 1))
        sm.select(sel, QItemSelectionModel.Select)
        cur = current if current is not None and 0 <= current < n else rows[0]
        sm.setCurrentIndex(self.model.index(cur, 0), QItemSelectionModel.NoUpdate)
        self.grid.scrollTo(self.model.index(cur, 0))

    def goto_row(self, r):
        if 0 <= r < len(self.doc.events):
            self.select_rows([r], current=r); self.seek_to_line(r, only_if_playing=False)

    def undo(self):
        res = self.history.undo_step(self.doc, self.sel_rows())
        if res:
            label, doc, sel = res; self.set_doc(doc); self.after_change(sel); self.statusBar().showMessage(f"Geri alındı: {label}", 3000)

    def redo(self):
        res = self.history.redo_step(self.doc, self.sel_rows())
        if res:
            label, doc, sel = res; self.set_doc(doc); self.after_change(sel); self.statusBar().showMessage(f"Yinelendi: {label}", 3000)

    # ------------------------------------------------------------------ files
    def maybe_save(self) -> bool:
        if not self.dirty: return True
        r = QMessageBox.question(self, APP, "Değişiklikler kaydedilsin mi?", QMessageBox.Save | QMessageBox.Discard | QMessageBox.Cancel)
        if r == QMessageBox.Save: return self.save()
        return r == QMessageBox.Discard

    def new_file(self):
        if not self.maybe_save(): return
        d = Document(); d.events = []
        self.path = None; self.dirty = False; self.history = History(); self.set_doc(d)
        self.after_change([]); self.dirty = False; self._refresh_title()

    def open_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Altyazı aç", self.settings.value("last_dir", ""), SUPPORTED)
        if p: self.open_file(p)

    def open_file(self, p: str, encoding=None):
        if self.dirty and not self.maybe_save(): return
        try:
            d = formats.load_document(p, encoding)
        except Exception as e:
            QMessageBox.critical(self, APP, f"Açılamadı:\n{e}"); return
        ext = os.path.splitext(p)[1].lower()
        self.path = p if ext in (".ass", ".ssa") else None
        self.history = History(); self.set_doc(d); self.dirty = ext not in (".ass", ".ssa")
        self.settings.setValue("last_dir", os.path.dirname(p)); self._add_recent(p)
        self.after_change([0] if d.events else []); self.dirty = ext not in (".ass", ".ssa"); self._refresh_title()
        self.statusBar().showMessage(f"{len(d.events)} satır yüklendi — {os.path.basename(p)}", 5000)
        if d.events: self.goto_row(0)

    def save(self) -> bool:
        if not self.path: return self.save_as()
        return self._write(self.path)

    def save_as(self) -> bool:
        p, _ = QFileDialog.getSaveFileName(self, "Farklı kaydet", self.path or "", "Advanced SubStation Alpha (*.ass)")
        if not p: return False
        if not p.lower().endswith((".ass", ".ssa")): p += ".ass"
        if self._write(p): self.path = p; self._add_recent(p); self._refresh_title(); return True
        return False

    def _write(self, p) -> bool:
        try:
            self._commit_pending()
            ass_save(self.doc, p, bom=self.settings.value('bom', 'true') == 'true'); self.dirty = False; self._refresh_title()
            self.statusBar().showMessage(f"Kaydedildi: {p}", 3000); return True
        except Exception as e:
            QMessageBox.critical(self, APP, f"Kaydedilemedi:\n{e}"); return False

    def export(self, ext, keep_tags=True):
        base = os.path.splitext(self.path)[0] if self.path else "altyazi"
        p, _ = QFileDialog.getSaveFileName(self, "Dışa aktar", base + "." + ext, f"*.{ext}")
        if p:
            formats.save_document(self.doc, p, keep_tags=keep_tags)
            self.statusBar().showMessage(f"Dışa aktarıldı: {p}", 4000)

    def _add_recent(self, p):
        r = [x for x in (self.settings.value("recent", []) or []) if x != p]
        r.insert(0, p); self.settings.setValue("recent", r[:10]); self._fill_recent()

    def _fill_recent(self):
        self.recent_menu.clear()
        for p in self.settings.value("recent", []) or []:
            a = self.recent_menu.addAction(p); a.triggered.connect(lambda _=False, p=p: self.open_file(p))

    def _autosave(self):
        if not self.dirty: return
        try:
            d = os.path.expanduser("~/.kaede/autosave"); os.makedirs(d, exist_ok=True)
            n = os.path.basename(self.path) if self.path else "untitled.ass"
            with open(os.path.join(d, n + ".bak"), "w", encoding="utf-8") as f: f.write(self.doc.dump())
        except OSError:
            pass

    def closeEvent(self, e):
        if self.maybe_save():
            self.settings.setValue("geometry", self.saveGeometry())
            if self.player: self.player.stop()
            e.accept()
        else:
            e.ignore()

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls(): e.acceptProposedAction()

    def dropEvent(self, e):
        for u in e.mimeData().urls():
            p = u.toLocalFile()
            ext = os.path.splitext(p)[1].lower()
            if ext in (".ass", ".ssa", ".srt", ".vtt", ".sbv", ".txt"): self.open_file(p)
            elif ext in (".lua",): self.add_script(p)
            else: self.open_video(p)

    # ------------------------------------------------------------------ edit panel
    def _refresh_styles_box(self):
        cur = self.style_box.currentText()
        self.style_box.blockSignals(True); self.style_box.clear()
        self.style_box.addItems([s.name for s in self.doc.styles]); self.style_box.setCurrentText(cur)
        self.style_box.blockSignals(False)

    def _on_current_changed(self, cur, prev):
        self._load_edit_panel()
        r = cur.row()
        if 0 <= r < len(self.doc.events):
            e = self.doc.events[r]
            self.wave.set_active(e.start, e.end)
            self._update_waveform_spans()
            self.visual.redraw()

    def _load_edit_panel(self):
        r = self.cur_row()
        self._updating = True
        try:
            if not (0 <= r < len(self.doc.events)):
                self.edit_text.setPlainText(""); self.stats.setText(""); return
            e = self.doc.events[r]
            if self.edit_text.toPlainText() != e.text: self.edit_text.setPlainText(e.text)
            if e.style not in [self.style_box.itemText(i) for i in range(self.style_box.count())]: self.style_box.addItem(e.style)
            self.style_box.setCurrentText(e.style); self.actor_edit.setText(e.actor); self.effect_edit.setText(e.effect)
            self.layer_spin.setValue(e.layer); self.ml.setValue(e.margin_l); self.mr.setValue(e.margin_r); self.mv.setValue(e.margin_t)
            self.comment_chk.setChecked(e.comment)
            self.t_start.set_ms(e.start); self.t_end.set_ms(e.end); self.t_dur.set_ms(e.duration)
            self._update_stats(e)
            if self.model.ref is not None and r < len(self.model.ref): self.ref_label.setText("Referans: " + self.model.ref[r])
        finally:
            self._updating = False

    def _update_stats(self, e: Event):
        n = char_count(e.text); d = max(1, e.duration) / 1000
        lines = re.split(r"\\N|\\n", strip_tags(e.text))
        cpl = max(len(l.strip()) for l in lines) if lines else 0
        self.stats.setText(f"{n} kr · {n / d:.1f} cps · {len(lines)} satır · en uzun {cpl}")

    def _history_for(self, label, key):
        """Coalesce rapid same-field edits into one undo step."""
        now = time.time()
        if self._last_edit_key[0] == key and now - self._last_edit_key[1] < 1.5:
            self._last_edit_key = (key, now); return
        self.history.push(label, self.doc, self.sel_rows())
        self._last_edit_key = (key, now)

    def _text_edited(self):
        if self._updating: return
        r = self.cur_row()
        if not (0 <= r < len(self.doc.events)): return
        e = self.doc.events[r]; t = self.edit_text.toPlainText().replace("\n", "\\N")
        if t == e.text: return
        self._history_for("Metin düzenle", ("text", r))
        e.text = t
        self.model.refresh_rows(r, r); self.mark_dirty(); self._update_stats(e); self._schedule_overlay()

    def _field_edited(self, name, value):
        if self._updating: return
        rows = self.sel_rows() or [self.cur_row()]
        rows = [r for r in rows if 0 <= r < len(self.doc.events)]
        if not rows: return
        self._history_for(f"{name} düzenle", (name, tuple(rows)))
        for r in rows: setattr(self.doc.events[r], name, value)
        self.model.refresh_rows(min(rows), max(rows)); self.mark_dirty(); self._schedule_overlay()
        self._load_edit_panel()

    def _time_edited(self, which, ms):
        if self._updating: return
        r = self.cur_row()
        if not (0 <= r < len(self.doc.events)): return
        e = self.doc.events[r]
        self._history_for("Zamanı düzenle", ("time", r))
        if which == "start": e.start = ms; e.end = max(e.end, ms)
        elif which == "end": e.end = max(ms, e.start)
        else: e.end = e.start + ms
        self.model.refresh_rows(r, r); self.mark_dirty(); self._load_edit_panel(); self.wave.set_active(e.start, e.end)
        self._update_waveform_spans(); self._schedule_overlay()

    def _commit_pending(self):
        pass

    def commit_next(self):
        r = self.cur_row()
        self.goto_row(r + 1 if r + 1 < len(self.doc.events) else r)
        self.edit_text.setFocus()

    def toggle_tag(self, tag):
        c = self.edit_text.textCursor()
        if c.hasSelection():
            s = c.selectedText()
            c.insertText("{\\%s1}%s{\\%s0}" % (tag, s, tag))
        else:
            c.insertText("{\\%s1}" % tag)

    # ------------------------------------------------------------------ line ops
    def insert_line(self, after: bool):
        def f(d, sel):
            r = self.cur_row()
            base = d.events[r] if 0 <= r < len(d.events) else None
            n = Event(style=base.style if base else (d.styles[0].name if d.styles else "Default"))
            if base:
                n.start = base.end if after else max(0, base.start - 2000)
                n.end = n.start + 2000
            at = (r + 1 if after else r) if base else len(d.events)
            d.events.insert(at, n); self._new_row = at
        self.mutate("Satır ekle", f, select_after=[])
        self.select_rows([self._new_row], current=self._new_row); self.edit_text.setFocus()

    def duplicate(self):
        sel = self.sel_rows()
        if sel: self.mutate("Çoğalt", lambda d, s: tools.duplicate_events(d, s), select_after=[])

    def delete_lines(self):
        sel = self.sel_rows()
        if not sel: return
        def f(d, s):
            for i in reversed(s): del d.events[i]
        self.mutate("Sil", f, select_after=[])
        self.select_rows([min(sel[0], len(self.doc.events) - 1)])

    def join_lines(self, sep):
        sel = self.sel_rows()
        if len(sel) < 2: return
        self.mutate("Birleştir", lambda d, s: tools.merge_events(d, s, sep), select_after=[sel[0]])

    def split_at_cursor(self):
        r = self.cur_row()
        if r < 0: return
        pos = self.edit_text.textCursor().position()
        raw = self.edit_text.toPlainText()[:pos]
        vis = len(strip_tags(raw.replace("\n", "\\N")))
        self.mutate("Böl", lambda d, s: tools.split_at(d, r, vis), select_after=[r, r + 1])

    def toggle_comment(self):
        def f(d, sel):
            for i in sel: d.events[i].comment = not d.events[i].comment
        self.mutate("Yorum aç/kapa", f)

    def change_case(self, mode):
        def f(d, sel):
            for i in sel or range(len(d.events)):
                d.events[i].text = tools.apply_case(d.events[i].text, mode)
        self.mutate("Harf büyüklüğü", f)

    def toggle_theme(self):
        self.dark = not self.dark; self.settings.setValue("dark", "true" if self.dark else "false"); self.theme()

    def toggle_tags(self):
        self.model.show_tags = not self.model.show_tags; self.model.refresh()

    def _grid_menu(self, pos):
        m = QMenu(self)
        for lab, fn in (("Önüne ekle", lambda: self.insert_line(False)), ("Sonrasına ekle", lambda: self.insert_line(True)),
                        ("Çoğalt", self.duplicate), ("Sil", self.delete_lines), ("Birleştir", lambda: self.join_lines("\\N")),
                        ("Yorum aç/kapa", self.toggle_comment), ("Satıra git (video)", lambda: self.seek_to_line(self.cur_row()))):
            m.addAction(lab, fn)
        m.exec(self.grid.viewport().mapToGlobal(pos))

    # ------------------------------------------------------------------ find
    def show_find(self):
        if not self.find_dlg:
            self.find_dlg = FindReplaceDialog(self)
            self.find_dlg.findNext.connect(self._find_next)
            self.find_dlg.replaceOne.connect(lambda o: self._replace(o, False))
            self.find_dlg.replaceAll.connect(lambda o: self._replace(o, True))
        self.find_dlg.show(); self.find_dlg.raise_(); self.find_dlg.find.setFocus()

    def _rx(self, o):
        pat = o["find"] if o["regex"] else re.escape(o["find"])
        if o["tr"] and not o["regex"] and not o["case"]:
            pat = re.sub(r"[iİıI]", lambda m: {"i": "[iİ]", "İ": "[iİ]", "ı": "[ıI]", "I": "[ıI]"}[m.group(0)], pat)
        if o["word"]: pat = r"(?<!\w)" + pat + r"(?!\w)"
        return re.compile(pat, 0 if o["case"] else re.IGNORECASE)

    def _find_next(self, o):
        if not o["find"]: return
        try: rx = self._rx(o)
        except re.error as e: self.find_dlg.status.setText(f"Hatalı ifade: {e}"); return
        n = len(self.doc.events); start = self.cur_row() + 1
        rows = self.sel_rows() if o["sel"] and len(self.sel_rows()) > 1 else range(n)
        order = [r for r in list(range(start, n)) + list(range(0, start)) if r in rows]
        for r in order:
            if rx.search(getattr(self.doc.events[r], o["field"])):
                self.select_rows([r], current=r); self.find_dlg.status.setText(f"Satır {r + 1}"); return
        self.find_dlg.status.setText("Bulunamadı")

    def _replace(self, o, all_):
        if not o["find"]: return
        try: rx = self._rx(o)
        except re.error as e: self.find_dlg.status.setText(f"Hatalı ifade: {e}"); return
        repl = o["repl"] if o["regex"] else o["repl"].replace("\\", "\\\\")
        count = [0]
        def f(d, sel):
            rows = (sel if o["sel"] and sel else range(len(d.events))) if all_ else [self.cur_row()]
            for r in rows:
                if not 0 <= r < len(d.events): continue
                old = getattr(d.events[r], o["field"]); new, c = rx.subn(repl, old)
                if c: setattr(d.events[r], o["field"], new); count[0] += c
        self.mutate("Değiştir", f)
        self.find_dlg.status.setText(f"{count[0]} değişiklik")
        if not all_: self._find_next(o)

    # ------------------------------------------------------------------ timing tools
    def shift_dialog(self):
        dlg = ShiftDialog(self, bool(self.video_path))
        if not dlg.exec(): return
        ms, frames, scope, what = dlg.values()
        def f(d, sel):
            rows = None if scope == 1 else (sel if scope == 0 else list(range(sel[0], len(d.events))) if sel else None)
            if frames: tools.shift_frames(d, self.fr, frames, rows)
            else: tools.shift_times(d, ms, rows, start=what in (0, 1), end=what in (0, 2))
        self.mutate("Zamanları kaydır", f)

    def timing_dialog(self):
        dlg = TimingDialog(self)
        if not dlg.exec(): return
        def f(d, sel):
            rows = sel if dlg.sel.isChecked() and sel else None
            if dlg.overlap.isChecked(): tools.fix_overlaps(d, rows, dlg.gap.value())
            if dlg.chain.isChecked(): tools.chain_gaps(d, dlg.chain_ms.value(), dlg.gap.value(), rows)
            if dlg.dur.isChecked() or dlg.cps.isChecked():
                tools.enforce_duration(d, dlg.min_ms.value() if dlg.dur.isChecked() else 0, 9000,
                                       dlg.cps_v.value() if dlg.cps.isChecked() else 0, rows)
        self.mutate("Zamanlama düzeltmeleri", f)

    def snap_kf(self):
        if not self.keyframes: self.statusBar().showMessage("Keyframe yok (video açın)", 3000); return
        self.mutate("Keyframe'e yapıştır", lambda d, s: tools.snap_to_keyframes(d, self.keyframes, 250, 250, s or None))

    def set_from_video(self, which):
        r = self.cur_row()
        if r < 0: return
        t = self._cur_ms()
        def f(d, s):
            e = d.events[r]
            if which == "start": e.start = t; e.end = max(e.end, t + 100)
            else: e.end = max(t, e.start + 1)
        self.mutate("Zamanı videodan ayarla", f)

    def _wave_retimed(self, s, e):
        r = self.cur_row()
        if r < 0: return
        def f(d, sel): d.events[r].start, d.events[r].end = s, e
        self.mutate("Dalga biçiminde zamanla", f)

    def _wave_create(self, s, e):
        def f(d, sel):
            st = d.events[self.cur_row()].style if 0 <= self.cur_row() < len(d.events) else d.styles[0].name
            ev = Event(start=s, end=e, style=st); d.events.append(ev)
            tools.sort_events(d, "start"); self._new_row = d.events.index(ev)
        self.mutate("Satır oluştur", f, select_after=[])
        self.select_rows([self._new_row], current=self._new_row); self.edit_text.setFocus()

    def auto_vad(self):
        if self.audio_samples is None: self.statusBar().showMessage("Önce ses yükleyin", 3000); return
        segs = probe.detect_speech(self.audio_samples, 8000)
        st = self.doc.styles[0].name
        def f(d, sel):
            for a, b in segs: d.events.append(Event(start=a, end=b, style=st, text=""))
            tools.sort_events(d, "start")
        self.mutate("Konuşma tespiti", f, select_after=[])
        self.statusBar().showMessage(f"{len(segs)} konuşma bölgesi eklendi (boş metin)", 5000)

    # ------------------------------------------------------------------ dialogs
    def qc_dialog(self):
        dlg = QCDialog(self, self.doc, self.qc_cfg, self.fr)
        dlg.jump.connect(lambda r: self.goto_row(r))
        def mark():
            self.model.issues = {i.index: i.message for i in dlg.issues}; self.model.refresh()
        dlg.finished.connect(lambda *_: mark()); dlg.table.itemSelectionChanged.connect(mark)
        dlg.show(); self._qc = dlg

    def styles_dialog(self):
        self.history.push("Stil yöneticisi", self.doc, self.sel_rows())
        StylesManager(self, self.doc, self.style_lib).exec()
        self.after_change(self.sel_rows())

    def props_dialog(self):
        dlg = PropertiesDialog(self, self.doc, getattr(self, "_vsize", None))
        if dlg.exec():
            self.history.push("Betik özellikleri", self.doc, self.sel_rows()); dlg.apply(); self.after_change(self.sel_rows())
            self._setup_renderer_script()

    def set_fps_dialog(self):
        v, ok = QInputDialog.getDouble(self, "Kare hızı", "FPS:", self.fr.fps, 1, 240, 3)
        if ok: self.fr = FrameRate(v); self.host.fps = self.fr

    def set_default_fps(self):
        v, ok = QInputDialog.getDouble(self, "Varsayılan FPS", "FPS:", self.fr.fps, 1, 240, 3)
        if ok: self.settings.setValue("default_fps", v); self.fr = FrameRate(v); self.host.fps = self.fr

    # ---- extra tools
    def point_sync_dialog(self):
        sel = self.sel_rows()
        if len(sel) < 2:
            QMessageBox.information(self, APP, "İki noktalı senkron için tablo içinde iki satır seçin (ilk ve son referans satır)."); return
        a, b = sel[0], sel[-1]
        ea, eb = self.doc.events[a], self.doc.events[b]
        t1, ok = QInputDialog.getText(self, "Senkron", f"Satır {a + 1} yeni başlangıç (h:mm:ss.cc):", text=ass_time(ea.start))
        if not ok: return
        t2, ok = QInputDialog.getText(self, "Senkron", f"Satır {b + 1} yeni başlangıç (h:mm:ss.cc):", text=ass_time(eb.start))
        if not ok: return
        from ..core.timeutil import parse_ass_time
        try:
            n1, n2 = parse_ass_time(t1), parse_ass_time(t2)
        except ValueError:
            QMessageBox.warning(self, APP, "Geçersiz zaman"); return
        self.mutate("İki noktalı senkron", lambda d, s: tools.point_sync_lines(d, a, n1, b, n2))

    def change_fps_dialog(self):
        src, ok = QInputDialog.getDouble(self, "FPS dönüştür", "Kaynak FPS:", 23.976, 1, 240, 3)
        if not ok: return
        dst, ok = QInputDialog.getDouble(self, "FPS dönüştür", "Hedef FPS:", 25.0, 1, 240, 3)
        if ok: self.mutate("FPS dönüştür", lambda d, s: tools.change_fps(d, src, dst, s if len(s) > 1 else None))

    def auto_balance_dialog(self):
        v, ok = QInputDialog.getInt(self, "Satırları dengele", "Satır başına en çok karakter:", 42, 10, 100)
        if ok: self.mutate("Satırları dengele", lambda d, s: tools.auto_balance(d, v, s or None))

    def show_stats(self):
        st = tools.statistics(self.doc)
        if not st.get("lines"): QMessageBox.information(self, APP, "Satır yok"); return
        txt = (f"Satır: {st['lines']}  (yorum: {st['comments']})\nSözcük: {st['words']}  Karakter: {st['chars']}\n"
               f"Ort. CPS: {st['avg_cps']:.1f}  Maks CPS: {st['max_cps']:.1f}  >20 cps: {st['over_20cps']}\n"
               f"Süre ort/min/maks: {st['avg_duration_ms']:.0f} / {st['min_duration_ms']} / {st['max_duration_ms']} ms\n"
               f"Toplam: {ass_time(st['total_ms'])}\n\nStiller:\n" + "\n".join(f"  {k}: {v}" for k, v in st["styles"].items()))
        TextDialog(self, "İstatistikler", txt).exec()

    def fonts_dialog(self):
        from ..core import fonts
        uses = fonts.check(self.doc)
        lines = [f"{'✔' if u.installed else '✘ EKSİK'}  {u.family}{' (kalın)' if u.bold else ''}{' (italik)' if u.italic else ''}  — {u.lines} satır  {u.path or ''}" for u in uses]
        miss = [u for u in uses if not u.installed]
        txt = "\n".join(lines) + (f"\n\n{len(miss)} yazı tipi sistemde yok." if miss else "\n\nTüm yazı tipleri kurulu.")
        TextDialog(self, "Yazı tipleri", txt).exec()
        if uses and QMessageBox.question(self, APP, "Kurulu yazı tiplerini bir klasöre/zip'e toplayayım mı?") == QMessageBox.Yes:
            p, _ = QFileDialog.getSaveFileName(self, "Zip olarak kaydet", "yazitipleri.zip", "*.zip")
            if p:
                copied, missing = fonts.collect(self.doc, p, as_zip=True)
                self.statusBar().showMessage(f"{len(copied)} dosya toplandı, {len(missing)} eksik", 6000)

    def fix_common_dialog(self):
        dlg = FixCommonDialog(self)
        if not dlg.exec(): return
        opts = dlg.options(); cnt = [0]
        self.mutate("Sık hataları düzelt", lambda d, s: cnt.__setitem__(0, tools.fix_common(d, opts, s if dlg.sel.isChecked() and s else None)))
        self.statusBar().showMessage(f"{cnt[0]} satır düzeltildi", 5000)

    def load_reference(self):
        p, _ = QFileDialog.getOpenFileName(self, "Referans (orijinal) altyazı", self.settings.value("last_dir", ""), SUPPORTED)
        if not p: return
        try: self.ref_doc = formats.load_document(p)
        except Exception as e: QMessageBox.warning(self, APP, f"Açılamadı: {e}"); return
        self._update_reference(); self.statusBar().showMessage(f"Referans: {os.path.basename(p)} ({len(self.ref_doc.events)} satır)", 5000)

    def close_reference(self):
        self.ref_doc = None; self._update_reference()

    def _update_reference(self):
        rd = getattr(self, "ref_doc", None)
        self.model.beginResetModel()
        self.model.ref = tools.match_reference(self.doc, rd) if rd else None
        self.model.endResetModel()
        if rd: self.grid.horizontalHeader().setSectionResizeMode(len(COLS), QHeaderView.Stretch); self.grid.horizontalHeader().setSectionResizeMode(8, QHeaderView.Interactive); self.grid.setColumnWidth(8, 420)
        else: self.grid.horizontalHeader().setSectionResizeMode(8, QHeaderView.Stretch)
        self.ref_label.setVisible(bool(rd)); self._load_edit_panel()

    def settings_dialog(self):
        dlg = SettingsDialog(self, self.settings)
        if dlg.exec():
            dlg.save()
            self.qc_cfg.max_cps = float(self.settings.value("qc_cps", 20)); self.qc_cfg.max_cpl = int(self.settings.value("qc_cpl", 42))
            self.dark = self.settings.value("dark", "true") == "true"; self.theme()
            m = int(self.settings.value("autosave_min", 2)); self.autosave.stop()
            if m: self.autosave.start(m * 60000)
            self.reload_scripts()

    def toggle_spectrogram(self):
        if self.audio_samples is None: self.statusBar().showMessage("Önce ses yükleyin", 3000); return
        if self.wave.spec_img is None:
            self.statusBar().showMessage("Spektrogram hesaplanıyor…"); QApplication.processEvents()
            spec = probe.spectrogram(self.audio_samples, 8000, 512, 128)
            self.wave.set_spectrogram(spec, 128 / 8000 * 1000)
        self.wave.show_spec = not self.wave.show_spec; self.wave.update(); self.statusBar().clearMessage()

    def goto_dialog(self):
        v, ok = QInputDialog.getInt(self, "Satıra git", "Satır no:", self.cur_row() + 1, 1, max(1, len(self.doc.events)))
        if ok: self.goto_row(v - 1)

    def load_timecodes(self):
        from ..core import timecodes
        p, _ = QFileDialog.getOpenFileName(self, "Zaman kodu dosyası", "", "Zaman kodları (*.txt *.tc *.tcv2);;Hepsi (*)")
        if not p: return
        try:
            self.fr = timecodes.parse_timecodes(formats.read_text(p)); self.host.fps = self.fr
            self.statusBar().showMessage("VFR zaman kodları yüklendi", 4000)
        except Exception as e:
            QMessageBox.warning(self, APP, f"Geçersiz zaman kodu dosyası: {e}")

    def load_keyframes(self):
        from ..core import timecodes
        p, _ = QFileDialog.getOpenFileName(self, "Keyframe dosyası", "", "Hepsi (*)")
        if not p: return
        frames = timecodes.parse_keyframes(formats.read_text(p))
        self.keyframes = [self.fr.time_at(f) for f in frames]; self.host.keyframes = self.keyframes
        self.wave.keyframes = self.keyframes; self.wave.update()
        self.statusBar().showMessage(f"{len(frames)} keyframe yüklendi", 4000)

    def save_keyframes(self):
        from ..core import timecodes
        if not self.keyframes: return
        p, _ = QFileDialog.getSaveFileName(self, "Keyframe kaydet", "keyframes.txt", "*.txt")
        if p:
            open(p, "w").write(timecodes.write_keyframes([self.fr.frame_at(k) for k in self.keyframes], self.fr.fps))

    def show_shortcuts(self):
        TextDialog(self, "Klavye kısayolları", SHORTCUTS).exec()

    def about(self):
        QMessageBox.about(self, "Kaede", "<b>Kaede</b> 0.1<br>Anime altyazı editörü — Aegisub uyumlu Lua otomasyonu (LuaJIT), "
                          "libass önizleme, Subtitle Edit tarzı araçlar.<br><br>Kaynak: github.com/osiozkan/osiozkan (kaede/)")

    # ------------------------------------------------------------------ video / audio
    def open_video_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Video/ses aç", self.settings.value("last_dir", ""), VIDEO_FILTER)
        if p: self.open_video(p)

    def open_video(self, p):
        self.video_path = p; self.host.video_path = p; self.host.audio_path = p
        if probe.have_ffmpeg():
            try:
                mi = probe.probe(p)
                if mi.fps > 1: self.fr = FrameRate(mi.fps); self.host.fps = self.fr
                if mi.width: self._vsize = (mi.width, mi.height); self.host.video_size = self._vsize; self.video.set_video_size(mi.width, mi.height)
            except Exception as e:
                self.statusBar().showMessage(f"ffprobe hatası: {e}", 5000)
            self._start_audio(p)
            try:
                self.keyframes = probe.keyframes(p); self.host.keyframes = self.keyframes; self.wave.keyframes = self.keyframes
            except Exception: pass
        if self.player:
            self.player.setSource(QUrl.fromLocalFile(p))
            self.player.pause()
        self._setup_renderer_script(); self._schedule_overlay()

    def close_video(self):
        self.video_path = ""; self.host.video_size = None; self._vsize = None
        if self.player: self.player.setSource(QUrl())
        self.wave.set_peaks(None); self.audio_samples = None; self._schedule_overlay()

    def _start_audio(self, p):
        self.statusBar().showMessage("Ses çözülüyor…")
        self._aw = AudioWorker(p); self._at = QThread(self)
        self._aw.moveToThread(self._at); self._at.started.connect(self._aw.run)
        self._aw.done.connect(self._audio_done); self._at.start()

    def _audio_done(self, peaks, rate, err):
        self._at.quit()
        if peaks is None: self.statusBar().showMessage(f"Ses çözülemedi: {err}", 6000); return
        self.audio_samples = getattr(self._aw, "samples", None)
        self.wave.set_peaks(peaks, 100); self.statusBar().showMessage("Ses hazır", 3000)

    def _duration_ms(self):
        return self.player.duration() if self.player and self.player.duration() > 0 else max([e.end for e in self.doc.events] + [60000])

    def _cur_ms(self):
        return int(self.player.position()) if self.player and self.video_path else self.wave.playhead

    def seek_ms(self, ms):
        ms = max(0, int(ms))
        if self.player and self.video_path: self.player.setPosition(ms)
        self.wave.set_playhead(ms); self._pos_changed(ms)

    def seek_to_line(self, r, only_if_playing=False):
        if 0 <= r < len(self.doc.events): self.seek_ms(self.doc.events[r].start)

    def step_frame(self, n):
        f = self.fr.frame_at(self._cur_ms()) + n
        self.seek_ms(self.fr.start_ms(max(0, f)) + 1)

    def toggle_play(self):
        if not self.player or not self.video_path: return
        if self.player.playbackState() == QMediaPlayer.PlayingState: self.player.pause()
        else: self.player.play()

    def play_line(self):
        r = self.cur_row()
        if r < 0 or not self.player: return
        e = self.doc.events[r]; self.seek_ms(e.start); self.player.play()
        self._stop_at = e.end

    def _sync_play_btn(self):
        playing = self.player and self.player.playbackState() == QMediaPlayer.PlayingState
        self.play_btn.setText("⏸" if playing else "▶")

    def _native_size(self, size):
        if size.isValid() and size.width() > 0:
            self.vitem.setSize(size); self._vsize = (int(size.width()), int(size.height()))
            self.video.set_video_size(*self._vsize); self._setup_renderer_script()

    def _on_position(self, ms):
        stop = getattr(self, "_stop_at", None)
        if stop is not None and ms >= stop and self.player.playbackState() == QMediaPlayer.PlayingState:
            self.player.pause(); self._stop_at = None
        self.wave.set_playhead(ms); self._pos_changed(ms)

    def _pos_changed(self, ms):
        self.pos_label.setText(f"{ass_time(ms)}  (kare {self.fr.frame_at(ms)})")
        d = self._duration_ms()
        if d and not self.seek_slider.isSliderDown(): self.seek_slider.setValue(int(ms / d * 1000))
        self._schedule_overlay()

    def _update_waveform_spans(self):
        sel = set(self.sel_rows()); cur = self.cur_row()
        self.wave.spans = [(e.start, e.end, i in sel or i == cur) for i, e in enumerate(self.doc.events) if not e.comment]
        if 0 <= cur < len(self.doc.events):
            e = self.doc.events[cur]; self.wave.active = (e.start, e.end)
        self.wave.update()

    # ------------------------------------------------------------------ libass overlay
    def _setup_renderer_script(self):
        if not libass_available(): return
        w, h = getattr(self, "_vsize", None) or self.doc.res
        sc = min(1.0, 1280 / max(w, 1))
        size = (max(2, int(w * sc)), max(2, int(h * sc)))
        try:
            if self.renderer is None: self.renderer = AssRenderer(*size)
            self.renderer.set_size(*size)
            self._overlay_size = size
        except Exception as ex:
            self.renderer = None; self.statusBar().showMessage(f"libass başlatılamadı: {ex}", 4000)
        self._script_dirty = True
        if not getattr(self, "_vsize", None):
            self.video.set_video_size(*size)

    def _schedule_overlay(self):
        self._script_dirty = True
        self.overlay_timer.start(40)

    def _refresh_overlay(self):
        if not self.renderer: return
        try:
            t = self._cur_ms() if self.video_path else self._preview_ms()
            self.renderer.set_script(self.doc.dump_window(t - 500, t + 500) if len(self.doc.events) > 400 else self.doc.dump())
            bg = None
            if not self.video_path:
                w, h = self._overlay_size
                bg = np.zeros((h, w, 4), np.uint8); bg[..., :3] = 24; bg[..., 3] = 255
            frame = self.renderer.render(t, bg)
            self.video.set_overlay(frame)
            if self.video_path:
                self.video.sub_pix.setScale(self.video.vw / frame.shape[1])
            else:
                self.video.sub_pix.setScale(1.0)
        except Exception:
            traceback.print_exc()

    def _preview_ms(self):
        r = self.cur_row()
        if 0 <= r < len(self.doc.events):
            e = self.doc.events[r]; return e.start + min(e.duration // 2, 500)
        return 0

    # ------------------------------------------------------------------ automation
    def script_dirs(self):
        home = os.path.expanduser("~")
        dirs = [os.path.join(home, ".kaede", "automation", "autoload"),
                os.path.join(os.path.dirname(__file__), "..", "..", "scripts"),
                os.path.join(home, ".aegisub", "automation", "autoload"),
                os.path.join(home, "Library", "Application Support", "Aegisub", "automation", "autoload")]
        if os.environ.get("APPDATA"):
            dirs.append(os.path.join(os.environ["APPDATA"], "Aegisub", "automation", "autoload"))
        dirs += list(self.settings.value("script_dirs", []) or [])
        return [d for d in dirs if os.path.isdir(os.path.abspath(d))] or dirs[:2]

    def reload_scripts(self):
        extra = [s.path for s in self.engine.scripts if not any(s.path.startswith(os.path.abspath(d)) for d in self.script_dirs())]
        self.engine.scripts.clear()
        for d in self.script_dirs(): self.engine.load_dir(os.path.abspath(d))
        for p in extra + list(self.settings.value("scripts", []) or []):
            if p and os.path.exists(p) and not any(s.path == os.path.abspath(p) for s in self.engine.scripts): self.engine.load_script(p)
        bad = [s for s in self.engine.scripts if s.error]
        if bad: self.statusBar().showMessage(f"{len(bad)} betik yüklenemedi (Otomasyon → Yönetici)", 6000)

    def add_script(self, p=None):
        if not p:
            p, _ = QFileDialog.getOpenFileName(self, "Lua betiği ekle", "", "Lua (*.lua);;Hepsi (*)")
        if p:
            s = self.engine.load_script(p)
            lst = list(self.settings.value("scripts", []) or []); lst.append(p); self.settings.setValue("scripts", lst)
            if s.error: QMessageBox.warning(self, APP, s.error)

    def _fill_auto_menu(self):
        m = self.auto_menu; m.clear()
        self._act(m, "Yönetici…", self.auto_manager)
        self._act(m, "Betikleri yeniden yükle", self.reload_scripts, "Ctrl+Alt+R")
        m.addSeparator()
        sel, act = self.sel_rows(), max(0, self.cur_row())
        for mac in self.engine.all_macros():
            parts = [p for p in re.split(r"/", mac.name) if p]
            if parts and parts[0].lower() == "automation": parts = parts[1:]
            menu = m
            for p in parts[:-1]:
                sub = next((a.menu() for a in menu.actions() if a.menu() and a.text() == p), None)
                menu = sub or menu.addMenu(p)
            a = QAction(parts[-1] if parts else mac.name, self); a.setToolTip(mac.description); a.setStatusTip(mac.description)
            try: a.setEnabled(self.engine.validate(mac, self.doc, sel, act))
            except Exception: pass
            a.triggered.connect(lambda _=False, mac=mac: self.run_macro(mac)); menu.addAction(a)
        filt = self.engine.all_filters()
        if filt:
            m.addSeparator(); fm = m.addMenu("Dışa aktarma filtreleri")
            for f in filt:
                fm.addAction(f.name, lambda f=f: self.run_filter(f))

    def run_macro(self, mac):
        self.current_script_title = mac.name
        sel = self.sel_rows(); act = max(0, self.cur_row())
        self.host.fps = self.fr
        self.host.script_path = mac.script.path
        QApplication.setOverrideCursor(Qt.WaitCursor)
        try:
            res = self.engine.run_macro(mac, self.doc, sel, act)
        except Cancelled:
            self.statusBar().showMessage("İptal edildi", 3000); return
        except AutomationError as e:
            TextDialog(self, f"Betik hatası — {mac.name}", str(e)).exec(); return
        finally:
            QApplication.restoreOverrideCursor()
        self.history.push(mac.name, self.doc, sel)
        self.set_doc(res.doc)
        self.after_change(res.selection if res.selection is not None else [r for r in sel if r < len(res.doc.events)], keep_current=False)
        if res.active is not None: self.select_rows(res.selection or [res.active], current=res.active)
        if res.logs: TextDialog(self, f"Günlük — {mac.name}", "".join(res.logs)).exec()

    def run_filter(self, f):
        self.current_script_title = f.name
        try:
            res = self.engine.run_filter(f, self.doc)
        except (AutomationError, Cancelled) as e:
            TextDialog(self, "Filtre hatası", str(e)).exec(); return
        p, _ = QFileDialog.getSaveFileName(self, "Filtrelenmiş çıktıyı kaydet", "", "*.ass")
        if p: ass_save(res.doc, p)

    def auto_manager(self):
        AutomationDialog(self, self.engine, [os.path.abspath(d) for d in self.script_dirs()], self.reload_scripts, self.add_script).exec()


SHORTCUTS = """Dosya: Ctrl+N Yeni · Ctrl+O Aç · Ctrl+S Kaydet · Ctrl+Shift+S Farklı kaydet
Düzen: Ctrl+Z/Shift+Z Geri al/yinele · Ctrl+Ins satır ekle · Ctrl+D çoğalt · Ctrl+Del sil
       Ctrl+J birleştir · Ctrl+Shift+Enter böl · Ctrl+/ yorum · Ctrl+F bul/değiştir
Metin: Ctrl+Enter uygula+sonraki · Shift+Enter \\N · Ctrl+B/I/U etiketler
Video: Ctrl+Space oynat · Ctrl+L satırı oynat · Alt+←/→ kare · Alt+↑/↓ satır
Zaman: Ctrl+Shift+T kaydır · Ctrl+Shift+←/→ başlangıç/bitişi videodan ata
Dalga formu: tıkla=atla · kenarları sürükle=zamanla · Ctrl+sürükle=yeni satır · Ctrl+tekerlek=yakınlaştır
Araçlar: F5 özellikler · F6 stiller · F7 QC · Ctrl+Alt+R betikleri yenile
"""
