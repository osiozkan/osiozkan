"""Ana pencere: menüler, araç çubuğu, bölmeler ve satır işlemleri."""

from __future__ import annotations

import os

from PySide6.QtCore import QByteArray, Qt, QTimer
from PySide6.QtGui import QAction, QIcon, QKeySequence
from PySide6.QtWidgets import (QApplication, QDockWidget, QFileDialog, QLabel, QMainWindow,
                               QMenu, QMessageBox, QSplitter, QToolBar)

from .. import APP_NAME, __version__
from ..automation.runtime import LUA_FLAVOR, ScriptManager
from ..core.assfile import AssDocument, Event
from ..core.formats import EXPORTERS, IMPORT_EXT, IMPORT_FILTER
from ..core.textutil import char_count, split_at_linebreak, strip_tags
from . import theme
from .automation_ui import (GuiHost, ScriptManagerDialog, init_invoker, run_macro_gui,
                            validate_macro_gui)
from .context import Context
from .dialogs import FindReplace, ScriptProps, ShiftTimes, StyleManager
from .editbox import EditBox
from .grid import SubsGrid
from .audio import AudioPanel
from .media_ctl import MediaController
from .panels import CplPanel, ToolRunDialog
from .framestrip import FrameStrip
from .reference import REF_FILTER, ReferenceController, ReferencePanel, TransferDialog
from .glossary_panel import GlossaryPanel
from .qc_panel import QcPanel
from .spell_ui import SpellService
from .timing_dialog import TimingDialog
from .mw_features import FeaturesMixin
from .video import VideoPanel, mpv_error

VIDEO_EXT = (".mkv", ".mp4", ".avi", ".m2ts", ".ts", ".webm", ".mov", ".wmv", ".flv", ".m4v", ".mpg", ".mpeg")
AUDIO_EXT = (".wav", ".flac", ".mp3", ".aac", ".m4a", ".opus", ".ogg", ".mka", ".ac3", ".eac3", ".dts", ".wv")

ASS_FILTER = "ASS altyazı (*.ass *.ssa);;Tüm dosyalar (*)"
OPEN_FILTER = IMPORT_FILTER


class MainWindow(FeaturesMixin, QMainWindow):
    def __init__(self, ctx: Context | None = None, headless: bool = False):
        super().__init__()
        init_invoker()
        self.headless = headless
        self.ctx = ctx or Context()
        self.gui_host = GuiHost(self.ctx, self)
        self.gui_host.doc = self.ctx.doc
        self.scripts = ScriptManager(self.ctx.config.autoload_dirs(), self.ctx.config.include_dirs(),
                                     self.gui_host)
        self.last_macro = None
        self.last_macro_log = ""
        self.setAcceptDrops(True)
        self.resize(1400, 900)

        self.acts: dict[str, QAction] = {}
        self._make_actions()
        self.editbox = EditBox(self.ctx)
        self.grid = SubsGrid(self.ctx, self.acts)
        self.grid.request_edit_focus.connect(self.editbox.focus_text)
        self.editbox.text.extra_actions = [self.acts[k] for k in ("split_cursor", "split_cursor_keep",
                                                                  "split_cursor_video", "break_lines")]
        split = QSplitter(Qt.Vertical)
        split.addWidget(self.editbox)
        split.addWidget(self.grid)
        split.setStretchFactor(0, 0)
        split.setStretchFactor(1, 1)
        split.setSizes([200, 600])
        self.setCentralWidget(split)

        self.media = MediaController(self.ctx)
        self.ctx.media = self.media
        use_mpv = (not headless) or bool(os.environ.get("OSISUB_MPV_VO"))
        self.video = VideoPanel(self.ctx, self.media, use_mpv=use_mpv)
        self.audio = AudioPanel(self.ctx, self.media)
        self.dock_video = self._dock("Video", self.video, Qt.LeftDockWidgetArea)
        self.dock_audio = self._dock("Ses", self.audio, Qt.TopDockWidgetArea)
        self.cpl_panel = CplPanel(self.ctx)
        self.dock_cpl = self._dock("CPL denetimi", self.cpl_panel, Qt.RightDockWidgetArea)
        self.strip = FrameStrip(self.ctx, self.media)
        self.dock_strip = self._dock("Kare şeridi", self.strip, Qt.BottomDockWidgetArea)
        self.strip.frame_clicked.connect(lambda f: self.video.seek_frame(f))
        self.dock_strip.hide()
        self.media.index_ready.connect(self.dock_strip.show)
        self.reference = ReferenceController(self.ctx)
        self.ctx.reference = self.reference
        self.ref_panel = ReferencePanel(self.ctx, self.reference, self)
        self.dock_ref = self._dock("Referans / aktarma", self.ref_panel, Qt.RightDockWidgetArea)
        self.dock_ref.hide()
        self.reference.changed.connect(self._reference_changed)
        from .compare_panel import ComparePanel
        self.compare_panel = ComparePanel(self.ctx, self)
        self.dock_compare = self._dock("Karşılaştırma", self.compare_panel, Qt.RightDockWidgetArea)
        self.tabifyDockWidget(self.dock_ref, self.dock_compare)
        self.dock_compare.hide()
        # yazım denetimi + denetim (QC)
        self.spell = SpellService(self.ctx.config)
        self.ctx.spell = self.spell
        self.editbox.text.highlighter.spell = self.spell
        self.spell.ready_changed.connect(lambda _ok: self.editbox.text.highlighter.rehighlight())
        self.spell.progress.connect(lambda m: self.statusBar().showMessage(m, 6000))
        self.qc_panel = QcPanel(self.ctx, self.spell)
        self.dock_qc = self._dock("Denetim (QC)", self.qc_panel, Qt.RightDockWidgetArea)
        self.tabifyDockWidget(self.dock_cpl, self.dock_qc)
        self.dock_qc.hide()
        self.glossary_panel = GlossaryPanel(self.ctx)
        self.dock_gloss = self._dock("Terim sözlüğü", self.glossary_panel, Qt.RightDockWidgetArea)
        self.tabifyDockWidget(self.dock_cpl, self.dock_gloss)
        self.dock_gloss.hide()
        from .motion_panel import MotionPanel
        self.motion_panel = MotionPanel(self.ctx, self.media, self.video, self)
        from PySide6.QtWidgets import QScrollArea
        motion_scroll = QScrollArea()
        motion_scroll.setWidgetResizable(True)
        motion_scroll.setWidget(self.motion_panel)
        self.dock_motion = self._dock("Hareket takibi", motion_scroll, Qt.RightDockWidgetArea)
        self.tabifyDockWidget(self.dock_cpl, self.dock_motion)
        self.dock_motion.hide()
        self.dock_video.hide()
        self.dock_audio.hide()
        self.media.video_opened.connect(lambda _p: self.dock_video.show())
        self.media.audio_ready.connect(self.dock_audio.show)
        self.media.progress.connect(lambda m: self.statusBar().showMessage(m, 8000))
        self.media.error.connect(self._media_error)
        self.media.index_ready.connect(self._update_counts)
        self.media.keyframes_changed.connect(self._keyframes_changed)
        self.audio.view.seek_request.connect(self.video.seek_ms)
        self.ctx.active_changed.connect(self._follow_line)

        from .recovery import Autosave
        self.autosave = Autosave(self.ctx, int(self.ctx.config["autosave_seconds"] or 0))
        from .icon import app_icon
        self.setWindowIcon(app_icon())
        self._crash_shown = 0
        self._make_menus()
        self._make_toolbar()
        self.status_lbl = QLabel("")
        self.statusBar().addPermanentWidget(self.status_lbl)
        self.ctx.status.connect(lambda m: self.statusBar().showMessage(m, 6000))
        self.ctx.undo_changed.connect(self._update_title)
        self.ctx.path_changed.connect(self._update_title)
        self.ctx.document_reset.connect(self._update_counts)
        self.ctx.undo_changed.connect(self._update_undo_actions)
        self.ctx.document_reset.connect(lambda: setattr(self.gui_host, "doc", self.ctx.doc))
        self._update_title()
        self._update_counts()
        self._update_undo_actions()
        self._restore_layout()
        if not headless:
            QTimer.singleShot(0, self.load_scripts)
            if self.ctx.config["spell_enabled"]:
                QTimer.singleShot(500, self.spell.start)

    # ================================================================ kurulum
    def _dock(self, title, widget, area):
        d = QDockWidget(title, self)
        d.setObjectName("dock_" + title)
        d.setWidget(widget)
        self.addDockWidget(area, d)
        return d

    def _act(self, key, text, slot, shortcut=None, tip=None, checkable=False):
        a = QAction(text, self)
        if shortcut:
            a.setShortcut(QKeySequence(shortcut))
            a.setShortcutContext(Qt.WindowShortcut)
        if tip:
            a.setToolTip(tip)
            a.setStatusTip(tip)
        a.setCheckable(checkable)
        a.triggered.connect(slot)
        self.acts[key] = a
        return a

    def _make_actions(self):
        A = self._act
        A("new", "Yeni", self.new_file, QKeySequence.New)
        A("open", "Aç…", self.open_file, QKeySequence.Open)
        A("save", "Kaydet", self.save_file, QKeySequence.Save)
        A("save_as", "Farklı kaydet…", self.save_file_as, "Ctrl+Shift+S")
        A("exit", "Çıkış", self.close, "Ctrl+Q")
        A("undo", "Geri al", self.ctx.do_undo, QKeySequence.Undo)
        A("redo", "Yinele", self.ctx.do_redo, "Ctrl+Y")
        A("find", "Bul…", lambda: self.show_find(False), QKeySequence.Find)
        A("replace", "Bul ve değiştir…", lambda: self.show_find(True), "Ctrl+H")
        A("multi_replace", "Çoklu değiştir…", self.show_multi_replace, "Ctrl+Alt+H",
          "Kural listeleriyle toplu değiştirme (Subtitle Edit Multiple replace); Türkçe doğru harf eşleşmesi")
        A("insert_before", "Önüne satır ekle", lambda: self.insert_line(False), "Ctrl+Shift+I")
        A("insert_after", "Arkasına satır ekle", lambda: self.insert_line(True), "Ctrl+I")
        A("duplicate", "Çoğalt", self.duplicate_lines, "Ctrl+D")
        A("delete_lines", "Satırları sil", self.delete_lines, "Ctrl+Delete")
        A("join_concat", "Birleştir (\\N ile)", lambda: self.join_lines("\\N"), "Ctrl+J")
        A("join_space", "Birleştir (boşlukla)", lambda: self.join_lines(" "))
        A("join_keep_first", "Birleştir (ilk metni koru)", lambda: self.join_lines(None))
        A("split_at_n", "\\N'den böl (zamanı orantıla)", self.split_at_n, "Ctrl+Shift+J")
        A("copy_lines", "Satırları kopyala", self.copy_lines, "Ctrl+Shift+C")
        A("cut_lines", "Satırları kes", self.cut_lines, "Ctrl+Shift+X")
        A("paste_lines", "Satırları yapıştır", self.paste_lines, "Ctrl+Shift+V")
        A("toggle_comment", "Yorum yap / kaldır", self.toggle_comment, "Ctrl+K")
        A("sort_time", "Başlangıç zamanına göre sırala", self.sort_by_time)
        A("break_lines", "Seçili satırları dengeli kır (CPL)", self.break_lines, "Ctrl+Shift+N",
          "Osi kuralı: 42'yi aşan satırı noktalama/bağlaç gözeterek iki dengeli satıra kır")
        A("unbreak_lines", "Satır kırmalarını kaldır", self.unbreak_lines)
        A("tool_sort", "Sırala…", lambda: self.show_tool("sort"))
        A("tool_case", "Büyük/küçük harf…", lambda: self.show_tool("case"))
        A("tool_hi", "İşitme engelli metnini kaldır…", lambda: self.show_tool("hi"))
        A("tool_merge", "Kısa satırları birleştir…", lambda: self.show_tool("merge"))
        A("tool_split", "Uzun satırları böl…", lambda: self.show_tool("split"))
        A("tool_stats", "İstatistikler…", self.show_stats)
        A("tool_durations", "Süreleri ayarla…", lambda: self.show_tool("durations"))
        A("tool_bridge", "Boşlukları köprüle…", lambda: self.show_tool("bridge"))
        A("tool_fps", "Kare hızını / hızı değiştir…", lambda: self.show_tool("fps"))
        A("tool_pointsync", "Noktalı senkron…", lambda: self.show_tool("pointsync"),
          tip="Seçili satırların doğru zamanlarını vererek tüm altyazıyı kaydır/esnet")
        # --- Aegisub araçları (1.3)
        A("select_lines", "Satır seç…", self.show_select_lines,
          tip="Metin/stil/aktör/efekte göre satır seç; seçime ekle, çıkar, kesiştir")
        A("fonts", "Yazı tipi toplayıcı…", self.show_fonts,
          tip="Kullanılan yazı tiplerini bul, eksik karakterleri (ğ ş ı İ) denetle, klasöre/zip'e topla")
        A("resample", "Çözünürlüğü yeniden örnekle…", self.show_resample,
          tip="PlayRes'i değiştir; stiller, konumlar, clip'ler, çizimler ve boyutlar ölçeklenir")
        A("split_cursor", "İmleçten böl (zamanı tahmin et)", lambda: self.split_at_cursor("estimate"))
        A("split_cursor_keep", "İmleçten böl (zamanı koru)", lambda: self.split_at_cursor("preserve"))
        A("split_cursor_video", "İmleçten böl (videodaki karede)", lambda: self.split_at_cursor("video"))
        A("continuous_start", "Zamanları sürekli yap (başlangıç değişir)", lambda: self.make_continuous("start"))
        A("continuous_end", "Zamanları sürekli yap (bitiş değişir)", lambda: self.make_continuous("end"))
        A("dup_shift", "1 kare kaydırarak çoğalt", self.duplicate_shift, "Ctrl+Shift+D",
          "Kare kare tipografi: seçili satırların bir kare sonrasına kopyası")
        A("swap_lines", "İki satırın yerini değiştir", self.swap_lines)
        A("paste_over", "Üstüne yapıştır…", lambda: self.paste_over(),
          tip="Panodaki satırları (ya da düz metni) seçili satırların üstüne, seçilen alanlarla yaz")
        A("kf_open", "Anahtar kare dosyası aç…", lambda: self.open_keyframes())
        A("kf_save", "Anahtar kareleri kaydet…", lambda: self.save_keyframes())
        A("kf_scenes", "Sahne değişimlerini anahtar karelere kat", self.keyframes_from_scenes)
        A("extract_subs", "Videodaki altyazıları aç…", lambda: self.show_extract_subs(),
          tip="MKV/MP4 içindeki ASS/SRT izini belge ya da referans olarak aç; ekli yazı tiplerini çıkar")
        A("burn", "Videoya yak (hardsub)…", self.show_burn,
          tip="libass ile altyazıyı videoya yak (x264/x265, CRF, anime ayarı, yazı tipleri toplanır)")
        A("mux", "MKV'ye göm (softsub)…", self.show_mux,
          tip="Yeniden kodlamadan MKV: video + ses + bu altyazı (dil/başlık/varsayılan) + yazı tipi ekleri")
        A("select_all", "Tüm satırları seç", self.select_all, "Ctrl+Shift+A")
        A("styles", "Stil yöneticisi…", self.show_styles, "Ctrl+Shift+Y")
        A("props", "Betik özellikleri…", self.show_props, "Ctrl+P")
        A("shift", "Zamanları kaydır…", self.show_shift, "Ctrl+Shift+H")
        A("snap_frames", "Seçili satırları kareye oturt", self.snap_to_frames)
        A("timing_pp", "Zamanlama son işlemcisi…", self.show_timing_pp, "Ctrl+Shift+T")
        A("sync", "Konuşmaya göre senkron (bsync)…", lambda: self.show_sync("audio"), "Ctrl+Shift+B",
          "Altyazıyı ses dalgasındaki konuşmaya hizala; kayma yalnız sahne aralarında değişir")
        A("sync_ref", "Referansa göre senkron (bsnap)…", lambda: self.show_sync("ref"), "Ctrl+Alt+B",
          "Altyazıyı doğru zamanlı referans altyazıya (ör. resmi EN) parçalı olarak hizala")
        # --- çeviri
        A("open_ref", "Referans altyazı aç…", self.open_reference, "Ctrl+Alt+O")
        A("close_ref", "Referansı kapat", lambda: self.reference.close())
        A("transfer", "Referanstan aktar (TR → bu dosya)…", self.transfer_from_reference)
        A("copy_ref", "Referans metnini satıra yaz", lambda: self.editbox.copy_reference(), "Ctrl+Alt+R")
        A("compare", "Kaynakla karşılaştır (aligncompare)…", self.show_compare, "Ctrl+Alt+K",
          "Referans (EN) ile bu dosyayı zamanla hizala: EKSİK/FAZLA/İNGİLİZCE?/UZUNLUK/BOŞ/ZAMAN/SAYI")
        A("export_srt", "SRT olarak dışa aktar…", self.export_srt)
        A("batch_convert", "Toplu dönüştür…", self.show_batch_convert,
          tip="Çok sayıda altyazıyı SRT/WebVTT/TTML-DFXP/SBV/MicroDVD/ASS/metin biçimine çevir")
        # --- video
        A("open_video", "Video aç…", self.open_video_dialog, "Ctrl+Shift+O")
        A("close_video", "Videoyu kapat", lambda: self.media.close_video())
        A("vid_line_start", "Videoyu satır başına götür", lambda: self._seek_line(False), "Ctrl+1")
        A("vid_line_end", "Videoyu satır sonuna götür", lambda: self._seek_line(True), "Ctrl+2")
        A("set_start_frame", "Başlangıcı geçerli kareye ayarla", lambda: self._set_from_frame(False), "Ctrl+3")
        A("set_end_frame", "Bitişi geçerli kareye ayarla", lambda: self._set_from_frame(True), "Ctrl+4")
        A("goto", "Kareye / zamana git…", self.goto_dialog, "Ctrl+G")
        A("play_pause", "Oynat / duraklat", lambda: self.video.toggle_play(), "Ctrl+Space")
        A("play_line", "Satırı oynat (video)", lambda: self.video.play_line(), "Ctrl+Shift+Space")
        A("prev_frame", "Önceki kare", lambda: self.video.step(-1), "Alt+Left")
        A("next_frame", "Sonraki kare", lambda: self.video.step(1), "Alt+Right")
        A("prev_kf", "Önceki anahtar kare", lambda: self.video.step_keyframe(-1), "Alt+Shift+Left")
        A("next_kf", "Sonraki anahtar kare", lambda: self.video.step_keyframe(1), "Alt+Shift+Right")
        A("save_frame", "Kareyi PNG kaydet (altyazılı)…", lambda: self.save_frame(True))
        A("scenes", "Sahne değişimlerini bul", self.find_scenes)
        A("qc_run", "Denetimi çalıştır (QC)", self.run_qc, "F7")
        A("dict_download", "Türkçe sözlüğü indir (~38 MB)…", self.download_dictionary)
        A("speech_lines", "Konuşmadan boş satırlar oluştur", self.lines_from_speech)
        A("asr", "Konuşmayı yazıya dök (Whisper)…", self.show_asr)
        A("glossary", "Terim sözlüğü", lambda: (self.dock_gloss.show(), self.dock_gloss.raise_()))
        A("add_term", "Seçili metni terim sözlüğüne ekle", self.add_term_from_selection)
        A("save_frame_raw", "Kareyi PNG kaydet (altyazısız)…", lambda: self.save_frame(False))
        A("motion", "Hareket takibi…", self.show_motion, "Ctrl+Alt+M",
          "Seçili satırlar için izleme: alanı çiz, izle, uygula (assmotion)")
        A("follow_line", "Satır değişince videoyu satıra götür", self._toggle_follow, checkable=True)
        # --- ses
        A("open_audio", "Ses aç…", self.open_audio_dialog)
        A("audio_from_video", "Sesi videodan aç", self.audio_from_video)
        A("close_audio", "Sesi kapat", lambda: (self.audio.stop(), self.media.close_audio()))
        A("play_sel", "Seçimi çal / durdur", lambda: self.audio.toggle_play_selection(), "Ctrl+Alt+Space")
        A("spectrum", "Spektrogram", lambda v=None: self.audio.spec.toggle(), checkable=False)
        A("prev_line", "Önceki satır", lambda: self.ctx.set_active(self.ctx.active - 1, True), "Ctrl+Up")
        A("next_line", "Sonraki satır", lambda: self.ctx.set_active(self.ctx.active + 1, True), "Ctrl+Down")
        A("scripts", "Automation betikleri…", self.show_script_manager, "Ctrl+Shift+M")
        A("reload_scripts", "Betikleri yeniden yükle", self.load_scripts)
        A("repeat_macro", "Son makroyu tekrarla", self.repeat_macro, "Ctrl+R")
        A("theme_dark", "Koyu tema", lambda: self.set_theme("dark"), checkable=True)
        A("theme_light", "Açık tema", lambda: self.set_theme("light"), checkable=True)
        A("about", "Hakkında", self.about)
        A("open_logs", "Günlük ve çökme raporları klasörünü aç", self.open_logs)
        A("recovery", "Kurtarma yedekleri…", lambda: self.check_recovery(always=True))
        A("shortcuts", "Kısayollar", self.show_shortcuts, "F1")

    def _make_menus(self):
        mb = self.menuBar()
        m = mb.addMenu("&Dosya")
        for k in ("new", "open"):
            m.addAction(self.acts[k])
        self.recent_menu = m.addMenu("Son açılanlar")
        self.recent_menu.aboutToShow.connect(self._fill_recent)
        m.addSeparator()
        for k in ("save", "save_as"):
            m.addAction(self.acts[k])
        em = m.addMenu("Dışa aktar")
        em.addAction(self.acts["export_srt"])
        for key, (label, ext) in EXPORTERS.items():
            if key in ("ass", "srt"):
                continue
            a = em.addAction(f"{label} ({ext})…")
            a.triggered.connect(lambda _=False, kk=key: self.export_as(kk))
        m.addAction(self.acts["batch_convert"])
        m.addAction(self.acts["fonts"])
        m.addSeparator()
        for k in ("extract_subs", "burn", "mux"):
            m.addAction(self.acts[k])
        m.addSeparator()
        m.addAction(self.acts["props"])
        m.addSeparator()
        m.addAction(self.acts["exit"])

        m = mb.addMenu("D&üzen")
        for k in ("undo", "redo", None, "find", "replace", "multi_replace", None, "copy_lines", "cut_lines",
                  "paste_lines", "paste_over", None, "select_all", "select_lines", "prev_line", "next_line"):
            m.addSeparator() if k is None else m.addAction(self.acts[k])

        m = mb.addMenu("&Altyazı")
        for k in ("insert_before", "insert_after", "duplicate", "dup_shift", None, "join_concat", "join_space",
                  "join_keep_first", "split_at_n", "split_cursor", "split_cursor_keep", "split_cursor_video", None,
                  "break_lines", "unbreak_lines", None, "toggle_comment", "delete_lines", "swap_lines", None,
                  "sort_time", "tool_sort", None, "tool_case", "tool_hi", "tool_merge", "tool_split", None,
                  "tool_stats", None, "styles", "resample"):
            m.addSeparator() if k is None else m.addAction(self.acts[k])

        m = mb.addMenu("&Zamanlama")
        m.addAction(self.acts["shift"])
        m.addAction(self.acts["snap_frames"])
        m.addAction(self.acts["timing_pp"])
        m.addAction(self.acts["sync"])
        m.addAction(self.acts["sync_ref"])
        m.addSeparator()
        for k in ("tool_durations", "tool_bridge", "tool_fps", "tool_pointsync", None, "continuous_start",
                  "continuous_end"):
            m.addSeparator() if k is None else m.addAction(self.acts[k])
        m.addSeparator()
        m.addAction(self.acts["set_start_frame"])
        m.addAction(self.acts["set_end_frame"])

        m = mb.addMenu("&Video")
        for k in ("open_video", "close_video", None, "play_pause", "play_line", None, "prev_frame",
                  "next_frame", "prev_kf", "next_kf", "goto", None, "vid_line_start", "vid_line_end",
                  "set_start_frame", "set_end_frame", None, "follow_line", "scenes", "kf_scenes", "kf_open",
                  "kf_save", None, "motion", None, "save_frame", "save_frame_raw", None, "extract_subs", "burn",
                  "mux"):
            m.addSeparator() if k is None else m.addAction(self.acts[k])
        from .visual import TOOLS
        vm = m.addMenu("Görsel tipografi aracı")
        for key, label, tip in [("video", "Video (normal görünüm)", "")] + TOOLS:
            a = vm.addAction(label)
            a.setStatusTip(tip)
            a.triggered.connect(lambda _=False, kk=key: (self.dock_video.show(), self.video.set_tool(kk)))
        self.acts["follow_line"].setChecked(bool(self.ctx.config["video_seek_on_line"]))

        m = mb.addMenu("Çe&viri")
        for k in ("open_ref", "close_ref", None, "copy_ref", "transfer", "compare", None, "glossary", "add_term"):
            m.addSeparator() if k is None else m.addAction(self.acts[k])

        m = mb.addMenu("&Ses")
        for k in ("open_audio", "audio_from_video", "close_audio", None, "play_sel", "spectrum", None,
                  "speech_lines", "asr"):
            m.addSeparator() if k is None else m.addAction(self.acts[k])

        self.automation_menu = mb.addMenu("A&utomation")
        self.automation_menu.aboutToShow.connect(self._validate_automation_menu)
        self.rebuild_automation_menu()

        self.tools_menu = mb.addMenu("A&raçlar")
        self.tools_menu.aboutToShow.connect(self._fill_tools)

        m = mb.addMenu("&Görünüm")
        for d in (self.dock_video, self.dock_audio, self.dock_strip, self.dock_cpl, self.dock_ref, self.dock_compare,
                  self.dock_qc, self.dock_gloss, self.dock_motion):
            m.addAction(d.toggleViewAction())
        m.addSeparator()
        m.addAction(self.acts["theme_dark"])
        m.addAction(self.acts["theme_light"])
        cur = self.ctx.config["theme"]
        self.acts["theme_dark"].setChecked(cur != "light")
        self.acts["theme_light"].setChecked(cur == "light")

        m = mb.addMenu("&Yardım")
        m.addAction(self.acts["shortcuts"])
        m.addSeparator()
        m.addAction(self.acts["recovery"])
        m.addAction(self.acts["open_logs"])
        m.addSeparator()
        m.addAction(self.acts["about"])

    def _make_toolbar(self):
        tb = QToolBar("Ana araçlar")
        tb.setObjectName("toolbar_main")
        tb.setMovable(False)
        for k in ("new", "open", "save", None, "undo", "redo", None, "find", "replace", None,
                  "styles", "props", "shift", None, "scripts", "repeat_macro"):
            tb.addSeparator() if k is None else tb.addAction(self.acts[k])
        self.addToolBar(tb)

    # ================================================================ durum
    def _update_title(self):
        self.setWindowTitle(f"{self.ctx.title} — {APP_NAME} {__version__}")

    def _update_counts(self):
        n = len(self.ctx.doc.events)
        parts = [f"{n} satır"]
        idx = getattr(self, "media", None) and self.media.index
        if idx:
            parts.append(f"video {len(idx)} kare {'VFR' if idx.is_vfr else 'CFR'} "
                         f"{idx.avg_fps:.3f} fps · {'libmpv' if self.video.mpv else 'kare gösterici'}")
        parts.append(LUA_FLAVOR)
        self.status_lbl.setText(" · ".join(parts))

    def _update_undo_actions(self):
        u = self.ctx.undo
        self.acts["undo"].setEnabled(u.can_undo)
        self.acts["redo"].setEnabled(u.can_redo)
        self.acts["undo"].setText(f"Geri al: {u.undo_desc()}" if u.can_undo else "Geri al")
        self.acts["redo"].setText(f"Yinele: {u.redo_desc()}" if u.can_redo else "Yinele")

    def set_theme(self, name):
        self.ctx.config["theme"] = name
        self.ctx.config.save()
        theme.apply(QApplication.instance(), name)
        self.acts["theme_dark"].setChecked(name != "light")
        self.acts["theme_light"].setChecked(name == "light")
        self.grid.viewport().update()
        self.editbox.load()
        self.cpl_panel.refresh()

    def _restore_layout(self):
        cfg = self.ctx.config
        try:
            if cfg["window_geometry"]:
                self.restoreGeometry(QByteArray.fromBase64(cfg["window_geometry"].encode()))
            if cfg["window_state"]:
                self.restoreState(QByteArray.fromBase64(cfg["window_state"].encode()))
        except Exception:  # noqa: BLE001
            pass

    # ================================================================ dosya
    def maybe_save(self) -> bool:
        if not self.ctx.undo.modified or self.headless:
            return True
        ans = QMessageBox.question(self, APP_NAME, "Değişiklikler kaydedilsin mi?",
                                   QMessageBox.Save | QMessageBox.Discard | QMessageBox.Cancel)
        if ans == QMessageBox.Save:
            return self.save_file()
        return ans == QMessageBox.Discard

    def new_file(self):
        if self.maybe_save():
            self.ctx.new_document()

    def open_file(self):
        if not self.maybe_save():
            return
        start = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
        path, _ = QFileDialog.getOpenFileName(self, "Altyazı aç", start, OPEN_FILTER)
        if path:
            self.open_path(path, force=True)

    def open_path(self, path, force=False):
        if not force and not self.maybe_save():
            return
        fps = None
        if path.lower().endswith(".sub") and self.media.index is not None:
            fps = self.media.index.avg_fps                  # MicroDVD: açık videonun kare hızı
        try:
            self.ctx.open_document(path, fps)
        except Exception as e:  # noqa: BLE001
            QMessageBox.critical(self, "Açılamadı", f"{path}\n\n{e}")
            return
        self.gui_host.doc = self.ctx.doc
        self.ctx.status.emit(f"Açıldı: {path}")
        if self.ctx.config["auto_load_media"]:
            self._load_project_media()

    def save_file(self) -> bool:
        if not self.ctx.doc.path:
            return self.save_file_as()
        self._store_media_paths()
        try:
            self.ctx.save_document()
        except Exception as e:  # noqa: BLE001
            QMessageBox.critical(self, "Kaydedilemedi", str(e))
            return False
        self.ctx.status.emit("Kaydedildi")
        return True

    def save_file_as(self) -> bool:
        start = self.ctx.doc.path or ""
        path, _ = QFileDialog.getSaveFileName(self, "Farklı kaydet", start, ASS_FILTER)
        if not path:
            return False
        if not os.path.splitext(path)[1]:
            path += ".ass"
        try:
            self.ctx.save_document(path)
        except Exception as e:  # noqa: BLE001
            QMessageBox.critical(self, "Kaydedilemedi", str(e))
            return False
        return True

    def _fill_recent(self):
        self.recent_menu.clear()
        for p in self.ctx.config["recent_files"] or []:
            a = self.recent_menu.addAction(p)
            a.triggered.connect(lambda _=False, pp=p: self.open_path(pp))
        if self.recent_menu.isEmpty():
            a = self.recent_menu.addAction("(boş)")
            a.setEnabled(False)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dropEvent(self, e):
        for u in e.mimeData().urls():
            p = u.toLocalFile()
            low = p.lower()
            if low.endswith(IMPORT_EXT):
                self.open_path(p)
                break
            if low.endswith(VIDEO_EXT):
                self.media.open_video(p)
                break
            if low.endswith(AUDIO_EXT):
                self.media.open_audio(p)
                break

    def closeEvent(self, e):
        if not self.maybe_save():
            e.ignore()
            return
        self.autosave.clear()          # düzgün kapanış: bu oturumun yedekleri gereksiz
        self.audio.stop()
        self.video.stop_backend()
        self.strip.shutdown()
        cfg = self.ctx.config
        cfg["window_geometry"] = bytes(self.saveGeometry().toBase64()).decode()
        cfg["window_state"] = bytes(self.saveState().toBase64()).decode()
        cfg.save()
        super().closeEvent(e)

    # ================================================================ satır işlemleri
    def insert_line(self, after: bool):
        ctx = self.ctx
        ev = ctx.active_event()
        if ev is None:
            new, idx = Event(), 0
        elif after:
            new = ev.fresh().replace(text="", effect="", comment=False, start=ev.end, end=ev.end + 5000)
            idx = ctx.active + 1
        else:
            new = ev.fresh().replace(text="", effect="", comment=False,
                                     start=max(0, ev.start - 5000), end=ev.start)
            idx = ctx.active
        ctx.insert_events(idx, [new], "Satır ekle")
        self.editbox.focus_text()

    def duplicate_lines(self):
        ctx = self.ctx
        rows = ctx.selected_rows()
        if not rows:
            return
        copies = [ctx.doc.events[r].fresh() for r in rows]
        ctx.insert_events(rows[-1] + 1, copies, "Çoğalt")

    def delete_lines(self):
        self.ctx.delete_rows(self.ctx.selected_rows())

    def toggle_comment(self):
        ctx = self.ctx
        rows = ctx.selected_rows()
        if not rows:
            return
        target = not ctx.doc.events[ctx.active if ctx.active in rows else rows[0]].comment
        ctx.update_events({r: {"comment": target} for r in rows}, "Yorum")

    def join_lines(self, sep):
        ctx = self.ctx
        rows = ctx.selected_rows()
        if len(rows) < 2:
            self.ctx.status.emit("Birleştirmek için en az iki satır seçin")
            return
        evs = ctx.doc.events
        first = evs[rows[0]]
        if sep is None:
            text = first.text
        else:
            text = sep.join(evs[r].text for r in rows)
        joined = first.replace(text=text, start=min(evs[r].start for r in rows),
                               end=max(evs[r].end for r in rows))
        new_events = [e for i, e in enumerate(evs) if i not in set(rows[1:])]
        new_events[rows[0]] = joined
        ctx.replace_all_events(new_events, "Birleştir", selection={rows[0]}, active=rows[0])

    def split_at_n(self):
        ctx = self.ctx
        ev = ctx.active_event()
        if ev is None:
            return
        parts = split_at_linebreak(ev.text)
        if not parts:
            self.ctx.status.emit("Satırda \\N yok")
            return
        a, b = parts
        la, lb = char_count(strip_tags(a)), char_count(strip_tags(b))
        total = max(1, la + lb)
        mid = ev.start + (ev.end - ev.start) * la // total
        if self.ctx.frame_map is not None:
            mid = self.ctx.frame_map.snap(mid)
        e1 = ev.replace(text=a, end=mid)
        e2 = ev.fresh().replace(text=b, start=mid)
        evs = list(ctx.doc.events)
        r = ctx.active
        evs[r:r + 1] = [e1, e2]
        ctx.replace_all_events(evs, "\\N'den böl", selection={r, r + 1}, active=r + 1)

    def copy_lines(self):
        ctx = self.ctx
        fmt = ctx.doc.ensure_section("events").format_fields
        lines = [ctx.doc.events[r].serialize(fmt) for r in ctx.selected_rows()]
        QApplication.clipboard().setText("\n".join(lines))
        ctx.status.emit(f"{len(lines)} satır kopyalandı")

    def cut_lines(self):
        self.copy_lines()
        self.delete_lines()

    def paste_lines(self):
        ctx = self.ctx
        txt = QApplication.clipboard().text().replace("\r\n", "\n")
        if not txt.strip():
            return
        fmt = ctx.doc.ensure_section("events").format_fields
        new = []
        base = ctx.active_event() or Event()
        for ln in txt.split("\n"):
            low = ln.lower()
            if low.startswith("dialogue:") or low.startswith("comment:"):
                new.append(Event.parse(ln, fmt).fresh())
            elif ln.strip():
                new.append(base.fresh().replace(text=ln, effect="", comment=False))
        if new:
            ctx.insert_events(ctx.active + 1, new, "Satırları yapıştır")

    def break_lines(self):
        from .tools_dialogs import break_changes
        ch = break_changes(self.ctx, self.ctx.selected_rows())
        n = len(self.ctx.update_events(ch, "Dengeli kır")) if ch else 0
        self.ctx.status.emit(f"{n} satır kırıldı" if n else "Kırılacak satır yok (hepsi sınır içinde)")
        return n

    def unbreak_lines(self):
        from .tools_dialogs import unbreak_changes
        ch = unbreak_changes(self.ctx, self.ctx.selected_rows())
        n = len(self.ctx.update_events(ch, "Kırmaları kaldır")) if ch else 0
        self.ctx.status.emit(f"{n} satırın kırması kaldırıldı")
        return n

    def show_multi_replace(self):
        from .multireplace_dialog import MultiReplaceDialog
        dlg = MultiReplaceDialog(self.ctx, self)
        if self.headless:
            return dlg
        dlg.exec()
        return dlg

    def show_tool(self, key: str):
        from .tools_dialogs import TOOL_DIALOGS
        dlg = TOOL_DIALOGS[key](self.ctx, self)
        if self.headless:
            return dlg
        dlg.exec()
        return dlg

    def show_stats(self):
        from .tools_dialogs import StatsDialog
        dlg = StatsDialog(self.ctx, self)
        if self.headless:
            return dlg
        dlg.exec()
        return dlg

    def sort_by_time(self):
        ctx = self.ctx
        evs = sorted(ctx.doc.events, key=lambda e: e.start)
        ctx.replace_all_events(evs, "Zamana göre sırala", selection={0}, active=0)

    def select_all(self):
        self.ctx.set_selection(range(len(self.ctx.doc.events)), self.ctx.active)

    # ================================================================ diyaloglar
    def show_find(self, replace):
        dlg = getattr(self, "_find_dlg", None)
        if dlg is None or dlg.windowTitle().startswith("Bul ve") != replace:
            dlg = FindReplace(self.ctx, self, replace)
            self._find_dlg = dlg
        dlg.show()
        dlg.raise_()
        dlg.find.setFocus()

    def show_styles(self):
        StyleManager(self.ctx, self).exec()

    def show_props(self):
        ScriptProps(self.ctx, self).exec()

    def show_shift(self):
        ShiftTimes(self.ctx, self).exec()

    def about(self):
        QMessageBox.about(self, APP_NAME, f"<b>{APP_NAME} {__version__}</b><br>"
                          "Aegisub yerine geçen altyazı düzenleyici.<br>"
                          f"Automation 4 uyumlu Lua motoru: {LUA_FLAVOR}<br><br>"
                          "Aegisub'ın automation/include dosyaları BSD/ISC lisanslıdır.")

    def show_shortcuts(self):
        rows = []
        for k, a in self.acts.items():
            if not a.shortcut().isEmpty():
                rows.append(f"<tr><td>{a.text().replace('&', '')}</td>"
                            f"<td><b>{a.shortcut().toString(QKeySequence.NativeText)}</b></td></tr>")
        rows.append("<tr><td>Metin kutusunda satır kırma (\\N)</td><td><b>Shift+Enter</b></td></tr>")
        rows.append("<tr><td>Onayla ve sonraki satıra geç</td><td><b>Enter</b></td></tr>")
        rows.append("<tr><td>Onayla, satırda kal</td><td><b>Ctrl+Enter</b></td></tr>")
        rows.append("<tr><td>Zaman kutusunda ±10 ms / ±1 sn</td><td><b>↑ ↓ / PgUp PgDn</b></td></tr>")
        QMessageBox.information(self, "Kısayollar", "<table cellpadding=3>" + "".join(rows) + "</table>")

    # ================================================================ Automation
    def load_scripts(self):
        self.scripts.autoload_dirs = self.ctx.config.autoload_dirs()
        self.scripts.include_dirs = self.ctx.config.include_dirs()
        self.scripts.load_all()
        self.rebuild_automation_menu()
        bad = [s for s in self.scripts.scripts if s.error]
        n = len(self.scripts.scripts)
        msg = f"{n} Automation betiği yüklendi"
        if bad:
            msg += f" · {len(bad)} betikte hata (Automation > Automation betikleri)"
        self.ctx.status.emit(msg)

    def rebuild_automation_menu(self):
        m = self.automation_menu
        m.clear()
        m.addAction(self.acts["scripts"])
        m.addAction(self.acts["reload_scripts"])
        m.addAction(self.acts["repeat_macro"])
        m.addSeparator()
        self._macro_actions = []
        submenus: dict[tuple, QMenu] = {}
        for macro in sorted(self.scripts.macros(), key=lambda x: x.name.lower()):
            path = macro.menu_path
            parent = m
            for depth in range(len(path) - 1):
                key = tuple(path[:depth + 1])
                if key not in submenus:
                    submenus[key] = parent.addMenu(path[depth])
                parent = submenus[key]
            a = parent.addAction(path[-1])
            a.setToolTip(macro.description)
            a.setStatusTip(macro.description)
            a.triggered.connect(lambda _=False, mm=macro: self.run_macro(mm))
            self._macro_actions.append((a, macro))

    def _validate_automation_menu(self):
        for a, macro in self._macro_actions:
            if macro.has_validate:
                ok, desc = validate_macro_gui(self, macro)
                a.setEnabled(bool(ok))
                if desc:
                    a.setStatusTip(desc)

    def run_macro(self, macro):
        self.last_macro = macro
        self.editbox.ctx.undo.break_coalesce()
        return run_macro_gui(self, macro)

    def repeat_macro(self):
        if self.last_macro is not None:
            self.run_macro(self.last_macro)

    def show_script_manager(self):
        ScriptManagerDialog(self, self.scripts).exec()

    # ================================================================ araçlar
    def _fill_tools(self):
        m = self.tools_menu
        m.clear()
        m.addAction(self.acts["qc_run"])
        m.addAction(self.dock_cpl.toggleViewAction())
        sp = m.addAction("Yazım denetimi: " + ("hazır" if self.spell.ready else
                                              "yükleniyor…" if self.spell.loading else "sözlük yok"))
        sp.setEnabled(False)
        m.addAction(self.acts["dict_download"])
        m.addSeparator()
        tools = self.ctx.config["external_tools"] or []
        for t in tools:
            a = m.addAction(t.get("name", "Araç"))
            a.setToolTip(f"{t.get('program', '')} {t.get('args', '')}")
            a.triggered.connect(lambda _=False, tt=t: self.run_tool(tt))
        if not tools:
            a = m.addAction("(harici araç tanımlı değil — config.json: external_tools)")
            a.setEnabled(False)

    def run_tool(self, tool):
        if tool.get("needs_saved", True):
            if self.ctx.undo.modified or not self.ctx.doc.path:
                if not self.save_file():
                    return
        dlg = ToolRunDialog(self, tool)
        dlg.start()
        dlg.exec()

    # ================================================================ video / ses
    def _media_error(self, msg):
        if self.headless:
            self.last_media_error = msg
            self.ctx.status.emit(msg)
        else:
            QMessageBox.warning(self, "Medya", msg)

    def open_video_dialog(self):
        start = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
        flt = "Video (" + " ".join("*" + e for e in VIDEO_EXT) + ");;Tüm dosyalar (*)"
        p, _ = QFileDialog.getOpenFileName(self, "Video aç", start, flt)
        if p:
            self.media.open_video(p)

    def open_audio_dialog(self):
        start = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
        flt = "Ses / video (" + " ".join("*" + e for e in AUDIO_EXT + VIDEO_EXT) + ");;Tüm dosyalar (*)"
        p, _ = QFileDialog.getOpenFileName(self, "Ses aç", start, flt)
        if p:
            self.media.open_audio(p)

    def audio_from_video(self):
        if self.media.video_path:
            self.media.open_audio(self.media.video_path)

    def _resolve_media(self, value: str) -> str | None:
        if not value:
            return None
        base = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
        p = value if os.path.isabs(value) else os.path.normpath(os.path.join(base, value))
        return p if os.path.exists(p) else None

    def _load_project_media(self):
        g = self.ctx.doc.garbage()
        v = self._resolve_media(g.get("Video File", ""))
        a_raw = g.get("Audio File", "")
        if v and v != self.media.video_path:
            a = self._resolve_media(a_raw) if a_raw and not a_raw.startswith("?video") else None
            self.media.open_video(v, with_audio=a is None)
            if a and a != v:
                self.media.open_audio(a)
        elif a_raw and not a_raw.startswith("?video"):
            a = self._resolve_media(a_raw)
            if a:
                self.media.open_audio(a)

    def _rel(self, p):
        base = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
        try:
            return os.path.relpath(p, base) if base else p
        except ValueError:          # farklı sürücü (Windows)
            return p

    def _store_media_paths(self):
        """Aegisub gibi video/ses yolunu [Aegisub Project Garbage]'a yazar —
        yalnız değiştiyse (bayt koruma)."""
        if not self.ctx.doc.path:
            return
        d = self.ctx.doc
        g = d.garbage()
        if self.media.video_path:
            rel = self._rel(self.media.video_path)
            if g.get("Video File") != rel:
                d.set_garbage("Video File", rel)
        if self.media.audio_path:
            val = "?video" if self.media.audio_path == self.media.video_path else self._rel(self.media.audio_path)
            if g.get("Audio File") != val:
                d.set_garbage("Audio File", val)

    def _follow_line(self, _row):
        if not self.ctx.config["video_seek_on_line"] or self.media.index is None or self.video.playing:
            return
        ev = self.ctx.active_event()
        if ev is not None:
            self.video.seek_ms(ev.start)

    def _toggle_follow(self):
        self.ctx.config["video_seek_on_line"] = self.acts["follow_line"].isChecked()
        self.ctx.config.save()

    def _seek_line(self, end: bool):
        ev = self.ctx.active_event()
        if ev is None or self.media.index is None:
            return
        fm = self.ctx.frame_map
        # bitiş: satırın göründüğü SON kare
        f = fm.frame_at_time(ev.end, 2) if end else fm.frame_at_time(ev.start, 1)
        self.video.seek_frame(f)

    def _set_from_frame(self, end: bool):
        fm = self.ctx.frame_map
        if fm is None:
            self.ctx.status.emit("Önce video açın")
            return
        f = self.media.frame
        changes = {}
        for r in self.ctx.selected_rows():
            ev = self.ctx.doc.events[r]
            if end:
                e = fm.time_at_frame(f, 2)
                changes[r] = {"end": e, "start": min(ev.start, e)}
            else:
                s = fm.time_at_frame(f, 1)
                changes[r] = {"start": s, "end": max(ev.end, s)}
        self.ctx.update_events(changes, "Bitiş = kare" if end else "Başlangıç = kare")

    def snap_to_frames(self):
        fm = self.ctx.frame_map
        if fm is None:
            self.ctx.status.emit("Önce video açın")
            return
        changes = {}
        for r in self.ctx.selected_rows():
            ev = self.ctx.doc.events[r]
            s = fm.time_at_frame(fm.frame_at_time(ev.start, 1), 1)
            e = fm.time_at_frame(fm.frame_at_time(ev.end, 2), 2)
            changes[r] = {"start": s, "end": max(s, e)}
        n = len(self.ctx.update_events(changes, "Kareye oturt"))
        self.ctx.status.emit(f"{n} satır kareye oturtuldu")

    def goto_dialog(self):
        from PySide6.QtWidgets import QInputDialog
        from ..core.asstime import parse_time
        if self.media.index is None:
            return
        txt, ok = QInputDialog.getText(self, "Git", "Kare numarası ya da zaman (0:01:23.45):",
                                       text=str(self.media.frame))
        if not ok or not txt.strip():
            return
        txt = txt.strip()
        if ":" in txt or "." in txt:
            self.video.seek_ms(parse_time(txt))
        elif txt.isdigit():
            self.video.seek_frame(int(txt))

    def save_frame(self, with_subs: bool):
        img = self.video.grab_frame(with_subs)
        if img is None:
            return
        from PySide6.QtGui import QImage
        base = os.path.splitext(self.media.video_path or "kare")[0]
        p, _ = QFileDialog.getSaveFileName(self, "Kareyi kaydet", f"{base}_{self.media.frame}.png", "PNG (*.png)")
        if p:
            h, w, _ = img.shape
            QImage(img.data, w, h, 3 * w, QImage.Format_RGB888).copy().save(p)

    # ================================================================ zamanlama / çeviri
    def show_timing_pp(self):
        TimingDialog(self.ctx, self).exec()

    def open_reference(self, path: str | None = None):
        if not path:
            start = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
            path, _ = QFileDialog.getOpenFileName(self, "Referans altyazı aç", start, REF_FILTER)
            if not path:
                return
        try:
            self.reference.load(path)
        except Exception as e:  # noqa: BLE001
            if self.headless:
                self.ctx.status.emit(f"Referans açılamadı: {e}")
            else:
                QMessageBox.warning(self, "Referans", f"{path}\n\n{e}")
            return
        self.dock_ref.show()
        self.ctx.status.emit(f"Referans: {os.path.basename(path)} · {len(self.reference.events)} satır")

    def _reference_changed(self):
        on = self.reference.loaded
        self.grid.set_reference_visible(on)
        self.editbox._update_ref()

    def transfer_from_reference(self):
        if not self.reference.loaded:
            self.open_reference()
            if not self.reference.loaded:
                return
        dlg = TransferDialog(self.ctx, self.reference, self)
        if self.headless:
            self._transfer_dlg = dlg
            return dlg
        if dlg.exec() and dlg.plan is not None:
            self.ref_panel.show_issues(dlg.plan)
            self.dock_ref.show()
        return dlg

    def show_compare(self):
        """Karşılaştırma bölmesini aç; kaynak yoksa önce referans altyazıyı sor, sonra karşılaştır."""
        self.dock_compare.show()
        self.dock_compare.raise_()
        if not self.reference.loaded and not self.headless:
            self.open_reference()
        if self.reference.loaded:
            return self.compare_panel.run()
        return None

    def export_srt(self):
        return self.export_as("srt")

    def export_as(self, key: str, path: str | None = None):
        """Açık belgeyi başka biçimde yazar (belge ve yolu değişmez)."""
        from ..core.formats import export_bytes
        label, ext = EXPORTERS[key]
        if path is None:
            base = os.path.splitext(self.ctx.doc.path or getattr(self.ctx.doc, "source_path", "") or "altyazi")[0]
            path, _ = QFileDialog.getSaveFileName(self, f"{label} olarak dışa aktar", base + ext, f"{label} (*{ext})")
            if not path:
                return None
        fps = self.media.index.avg_fps if self.media.index is not None else None
        try:
            data = export_bytes(self.ctx.doc, key, fps=fps, lang=self.ctx.config["export_lang"] or "tr",
                                ttml_time=self.ctx.config["ttml_time"] or "clock")
            with open(path, "wb") as f:
                f.write(data)
        except Exception as e:  # noqa: BLE001
            if self.headless:
                self.ctx.status.emit(f"Dışa aktarılamadı: {e}")
            else:
                QMessageBox.critical(self, "Dışa aktarılamadı", str(e))
            return None
        self.ctx.status.emit(f"Dışa aktarıldı: {path}")
        return path

    def show_batch_convert(self):
        from .batch_convert import BatchConvertDialog
        dlg = BatchConvertDialog(self.ctx, self)
        if self.headless:
            return dlg
        dlg.exec()
        return dlg

    # ================================================================ denetim / yazım / sahne
    def run_qc(self):
        self.dock_qc.show()
        self.dock_qc.raise_()
        issues = self.qc_panel.run()
        self.ctx.status.emit(f"Denetim: {len(issues)} sorun")
        return issues

    def download_dictionary(self):
        from ..core.spell import find_dictionary
        if find_dictionary(self.ctx.config["spell_lang"] or "tr_TR") and not self.headless:
            if QMessageBox.question(self, "Sözlük", "Sözlük zaten var. Yeniden indirilsin mi?") != QMessageBox.Yes:
                return
        self.spell.download()

    def find_scenes(self):
        if self.media.index is None:
            self.ctx.status.emit("Önce video açın")
            return
        self.media.detect_scenes(float(self.ctx.config["scene_threshold"] or 10.0))

    # ================================================================ konuşma / Whisper / terimler
    def lines_from_speech(self):
        if not self.media.speech:
            self.ctx.status.emit("Konuşma bölümü yok (önce ses açın)")
            return []
        from ..media.vad import lines_from_speech
        from .asr_dialog import insert_timed
        new = lines_from_speech(self.media.speech, self.ctx.doc.events)
        if not new:
            self.ctx.status.emit("Boş aralıkta konuşma bölümü yok")
            return []
        rows = insert_timed(self.ctx, new, "Konuşmadan satırlar")
        self.ctx.status.emit(f"{len(new)} boş satır oluşturuldu")
        return rows

    def show_asr(self):
        from .asr_dialog import AsrDialog
        dlg = AsrDialog(self)
        if self.headless:
            return dlg
        dlg.exec()
        return dlg

    def add_term_from_selection(self):
        sel = self.editbox.text.textCursor().selectedText().strip()
        self.dock_gloss.show()
        self.dock_gloss.raise_()
        self.glossary_panel.add_term(source="", target=sel)

    # ================================================================ hareket takibi
    def show_motion(self):
        """Bölmeyi aç, aralığı seçili satırlardan al, videoda İzleme aracını aç."""
        self.dock_motion.show()
        self.dock_motion.raise_()
        mp = self.motion_panel
        if self.media.index is None:
            self.statusBar().showMessage("Hareket takibi için önce video açın", 6000)
            return
        mp.range_from_lines()
        if not mp.roi and _has_rect_clip(self.ctx.active_event()):
            mp.roi_from_clip()
        self.dock_video.show()
        self.video.set_tool("track")


def _has_rect_clip(ev) -> bool:
    from ..core import tags
    c = tags.get_clip(ev.text) if ev is not None else None
    return bool(c and c[0] == "rect")


# ==================================================================== kurtarma / çökme
def _mw_check_recovery(self, always: bool = False) -> bool:
    """Açılışta: önceki oturumdan kalan yedek varsa kurtarma penceresi."""
    from .recovery import RecoveryDialog, find_recoverable, restore_into
    items = find_recoverable()
    if not items:
        if always:
            QMessageBox.information(self, APP_NAME, "Kurtarılacak yedek yok.")
        return False
    dlg = RecoveryDialog(items, self)
    if dlg.exec() and dlg.choice is not None:
        if not self.maybe_save():
            return False
        restore_into(self.ctx, dlg.choice)
        self.gui_host.doc = self.ctx.doc
        self.statusBar().showMessage("Yedek açıldı — kontrol edip Kaydet'e bas (özgün dosyaya henüz yazılmadı)", 15000)
        return True
    return False


def _mw_on_crash(self, report: str, summary: str):
    """Yakalanmamış hata (ana iş parçacığında, sinyal ile): acil yedek + bilgi."""
    backup = self.autosave.emergency()
    self._crash_shown += 1
    if self.headless or self._crash_shown > 3:      # hata döngüsünde pencere yağmuru olmasın
        self.statusBar().showMessage(f"Hata: {summary} — rapor: {report}", 15000)
        return
    msg = (f"Beklenmeyen bir hata oluştu ama program açık kalıyor.\n\n{summary}\n\n"
           f"Rapor: {report}\n" + (f"Acil yedek: {backup}\n" if backup else "") +
           "\nÇalışmanı kaydetmeni öneririm. Raporu bana gönderirsen düzeltirim.")
    QMessageBox.warning(self, f"{APP_NAME} — hata", msg)


def _mw_open_logs(self):
    from PySide6.QtCore import QUrl
    from PySide6.QtGui import QDesktopServices

    from ..diag import logs_dir
    QDesktopServices.openUrl(QUrl.fromLocalFile(logs_dir()))


MainWindow.check_recovery = _mw_check_recovery
MainWindow.on_crash = _mw_on_crash
MainWindow.open_logs = _mw_open_logs


def _mw_show_sync(self, source: str | None = None):
    has_ref, has_audio = self.reference.loaded, self.media.audio is not None
    msg = None
    if source == "ref" and not has_ref:
        msg = "Referansa göre senkron için önce referans altyazıyı açın (Çeviri > Referans altyazı aç)"
    elif source == "audio" and not has_audio:
        msg = "Konuşmaya göre senkron için önce ses açın"
    elif source is None and not (has_ref or has_audio):
        msg = "Senkron için önce ses ya da referans altyazı açın"
    if msg:
        self.statusBar().showMessage(msg, 6000)
        return None
    from .sync_dialog import SyncDialog
    dlg = SyncDialog(self.ctx, self.media, self, source=source)
    if self.headless:
        return dlg
    dlg.exec()
    return dlg


MainWindow.show_sync = _mw_show_sync
