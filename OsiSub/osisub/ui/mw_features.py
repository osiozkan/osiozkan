"""Ana pencerenin 1.3 özellikleri (Aegisub araçları, satır işlemleri, anahtar kareler).

MainWindow bu sınıftan da türer; burada yalnız yöntemler vardır (eylemler ve menüler
mainwindow.py'de kurulur). Her yöntem headless (test) kipinde pencereyi açmadan döndürür.
"""

from __future__ import annotations

import os

from PySide6.QtCore import QByteArray
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QApplication, QFileDialog, QInputDialog, QMessageBox

from ..core import aegisub_tools as A


class FeaturesMixin:
    # Çalışma düzenleri: hangi bölmeler görünür, hangisi öne gelir
    LAYOUT_PRESETS = {
        "Çeviri": {"show": ("dock_ref", "dock_compare", "dock_gloss", "dock_cpl"), "raise": ("dock_ref", "dock_cpl"),
                   "hide": ("dock_audio", "dock_strip", "dock_motion", "dock_qc")},
        "Zamanlama": {"show": ("dock_audio", "dock_video", "dock_strip", "dock_cpl"), "raise": ("dock_cpl",),
                      "hide": ("dock_ref", "dock_compare", "dock_gloss", "dock_motion", "dock_qc")},
        "Tipografi": {"show": ("dock_video", "dock_motion", "dock_strip"), "raise": ("dock_motion",),
                      "hide": ("dock_audio", "dock_ref", "dock_compare", "dock_gloss", "dock_qc", "dock_cpl")},
        "Denetim": {"show": ("dock_qc", "dock_cpl", "dock_compare", "dock_gloss"), "raise": ("dock_qc", "dock_compare"),
                    "hide": ("dock_audio", "dock_strip", "dock_motion")},
    }

    # ------------------------------------------------------------ kısayollar / ayarlar / palet
    def apply_shortcuts(self):
        """config.json > shortcuts değişikliklerini uygula (varsayılanlar default_shortcuts'ta)."""
        over = self.ctx.config["shortcuts"] or {}
        for key, a in self.acts.items():
            if key in over:
                a.setShortcut(QKeySequence(over[key]))
            else:
                a.setShortcut(self.default_shortcuts.get(key, QKeySequence()))

    def show_palette(self):
        from .palette import CommandPalette
        dlg = CommandPalette(self)
        if self.headless:
            return dlg
        dlg.show()
        dlg.edit.setFocus()
        return dlg

    def show_prefs(self):
        from .prefs import PrefsDialog
        return self._show(PrefsDialog(self))

    def apply_prefs(self, theme_changed: bool = False):
        from ..core.textutil import set_cpl_limit
        cfg = self.ctx.config
        if theme_changed:
            self.set_theme(cfg["theme"])
        set_cpl_limit(cfg["cpl_limit"] or 42)
        self.editbox.set_font_size(cfg["editbox_font_size"] or 13)
        self.grid.set_font_size(cfg["grid_font_size"] or 10)
        self.grid.model_.marker = cfg["grid_tag_marker"] or "\u2600"
        self.grid.model_.reset()
        self.editbox.load()
        self.cpl_panel.refresh()
        self.autosave.set_interval(int(cfg["autosave_seconds"] or 0))
        self.apply_shortcuts()
        if cfg["spell_enabled"] and not (self.spell.ready or self.spell.loading) and not self.headless:
            self.spell.start()
        self.ctx.status.emit("Ayarlar uygulandı")

    # ------------------------------------------------------------ çalışma düzenleri
    def apply_layout(self, name: str) -> bool:
        p = self.LAYOUT_PRESETS.get(name)
        if p is not None:
            for d in p["hide"]:
                getattr(self, d).hide()
            for d in p["show"]:
                getattr(self, d).show()
            for d in p["raise"]:
                getattr(self, d).raise_()
            self.ctx.status.emit(f"Çalışma düzeni: {name}")
            return True
        saved = (self.ctx.config["layouts"] or {}).get(name)
        if saved:
            ok = self.restoreState(QByteArray.fromBase64(saved.encode()))
            self.ctx.status.emit(f"Çalışma düzeni: {name}")
            return bool(ok)
        if name == "Varsayılan" and getattr(self, "_default_state", None) is not None:
            return bool(self.restoreState(self._default_state))
        return False

    def save_layout(self, name: str | None = None):
        if name is None:
            name, ok = QInputDialog.getText(self, "Düzeni kaydet", "Ad:", text="Benim düzenim")
            if not ok or not name.strip():
                return None
        name = name.strip()
        lay = dict(self.ctx.config["layouts"] or {})
        lay[name] = bytes(self.saveState().toBase64()).decode()
        self.ctx.config["layouts"] = lay
        self.ctx.config.save()
        self.ctx.status.emit(f"Düzen kaydedildi: {name}")
        return name

    def delete_layout(self, name: str):
        lay = dict(self.ctx.config["layouts"] or {})
        if lay.pop(name, None) is not None:
            self.ctx.config["layouts"] = lay
            self.ctx.config.save()

    def _fill_layouts_menu(self):
        m = self.layouts_menu
        m.clear()
        for name in list(self.LAYOUT_PRESETS) + ["Varsayılan"]:
            a = m.addAction(name)
            a.triggered.connect(lambda _=False, n=name: self.apply_layout(n))
        saved = self.ctx.config["layouts"] or {}
        if saved:
            m.addSeparator()
            for name in saved:
                a = m.addAction(name)
                a.triggered.connect(lambda _=False, n=name: self.apply_layout(n))
            dm = m.addMenu("Kayıtlı düzeni sil")
            for name in saved:
                a = dm.addAction(name)
                a.triggered.connect(lambda _=False, n=name: self.delete_layout(n))
        m.addSeparator()
        a = m.addAction("Şimdiki düzeni kaydet…")
        a.triggered.connect(lambda: self.save_layout())

    def _update_line_status(self, *_):
        n = len(self.ctx.doc.events)
        sel = len(self.ctx.selection)
        txt = f"Satır {self.ctx.active + 1}/{n}" if n else ""
        if sel > 1:
            txt += f" · {sel} seçili"
        self.line_lbl.setText(txt)
    # ------------------------------------------------------------ pencereler
    def _show(self, dlg):
        if self.headless:
            return dlg
        dlg.exec()
        return dlg

    def show_select_lines(self):
        from .aegisub_dialogs import SelectLinesDialog
        return self._show(SelectLinesDialog(self.ctx, self))

    def show_fonts(self):
        from .aegisub_dialogs import FontsDialog
        return self._show(FontsDialog(self.ctx, self))

    def show_resample(self):
        from .aegisub_dialogs import ResampleDialog
        return self._show(ResampleDialog(self.ctx, self))

    def show_extract_subs(self, video: str | None = None):
        from .media_export import ExtractDialog
        return self._show(ExtractDialog(self, video))

    def show_burn(self):
        from .media_export import BurnDialog
        return self._show(BurnDialog(self))

    def show_mux(self):
        from .media_export import MuxDialog
        return self._show(MuxDialog(self))

    def paste_over(self, text: str | None = None):
        from .aegisub_dialogs import PasteOverDialog
        text = QApplication.clipboard().text() if text is None else text
        if not text.strip():
            self.ctx.status.emit("Pano boş")
            return None
        return self._show(PasteOverDialog(self.ctx, text, self))

    # ------------------------------------------------------------ satır işlemleri
    def split_at_cursor(self, mode: str = "estimate"):
        ctx = self.ctx
        ev = ctx.active_event()
        if ev is None:
            return None
        pos = self.editbox.text.textCursor().position()
        split_ms = None
        if mode == "video":
            fm = ctx.frame_map
            if fm is None:
                ctx.status.emit("Önce video açın")
                return None
            split_ms = fm.time_at_frame(self.media.frame, 1)
            if not (ev.start < split_ms < ev.end):
                ctx.status.emit("Videonun karesi satırın içinde değil")
                return None
        res = A.split_at(ev, pos, mode, split_ms, ctx.frame_map)
        if res is None:
            ctx.status.emit("Bölünecek metin yok")
            return None
        r = ctx.active
        evs = list(ctx.doc.events)
        evs[r:r + 1] = list(res)
        ctx.replace_all_events(evs, "İmleçten böl", selection={r, r + 1}, active=r + 1)
        return res

    def make_continuous(self, change: str = "start"):
        ch = A.make_continuous(self.ctx.doc.events, self.ctx.selected_rows(), change)
        n = len(self.ctx.update_events(ch, "Zamanları sürekli yap")) if ch else 0
        self.ctx.status.emit(f"{n} satırın zamanı birleştirildi" if n else "Değişiklik yok (en az iki satır seçin)")
        return n

    def duplicate_shift(self):
        fm = self.ctx.frame_map
        if fm is None:
            self.ctx.status.emit("1 kare kaydırmak için önce video açın")
            return 0
        rows = self.ctx.selected_rows()
        evs = list(self.ctx.doc.events)
        new_rows = []
        for pos, dup in A.duplicate_shift(evs, rows, fm):
            evs.insert(pos, dup)
            new_rows = [r + 1 if r >= pos else r for r in new_rows] + [pos]
        self.ctx.replace_all_events(evs, "1 kare kaydırarak çoğalt", selection=set(new_rows),
                                    active=min(new_rows) if new_rows else None)
        return len(new_rows)

    def swap_lines(self):
        rows = self.ctx.selected_rows()
        if len(rows) != 2:
            self.ctx.status.emit("Yer değiştirmek için tam iki satır seçin")
            return False
        a, b = rows
        evs = list(self.ctx.doc.events)
        evs[a], evs[b] = evs[b], evs[a]
        self.ctx.replace_all_events(evs, "Satırların yerini değiştir", selection={a, b}, active=self.ctx.active)
        return True

    # ------------------------------------------------------------ anahtar kareler
    def open_keyframes(self, path: str | None = None):
        if self.media.index is None:
            self.ctx.status.emit("Önce video açın")
            return 0
        if path is None:
            start = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
            path, _ = QFileDialog.getOpenFileName(self, "Anahtar kare dosyası", start,
                                                  "Anahtar kareler (*.txt *.log *.stats *.pass *.kf);;Tüm dosyalar (*)")
            if not path:
                return 0
        try:
            with open(path, encoding="utf-8", errors="replace") as f:
                kfs = A.load_keyframes(f.read())
        except (OSError, ValueError) as e:
            if self.headless:
                self.ctx.status.emit(str(e))
            else:
                QMessageBox.warning(self, "Anahtar kareler", str(e))
            return 0
        n = self.media.set_keyframes(kfs)
        self.ctx.status.emit(f"{n} anahtar kare yüklendi: {os.path.basename(path)}")
        return n

    def save_keyframes(self, path: str | None = None):
        idx = self.media.index
        if idx is None:
            self.ctx.status.emit("Önce video açın")
            return None
        if path is None:
            base = os.path.splitext(self.media.video_path or "video")[0]
            path, _ = QFileDialog.getSaveFileName(self, "Anahtar kareleri kaydet", base + "_keyframes.txt",
                                                  "Anahtar kareler (*.txt)")
            if not path:
                return None
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(A.save_keyframes(idx.keyframes, idx.avg_fps))
        self.ctx.status.emit(f"{len(idx.keyframes)} anahtar kare kaydedildi")
        return path

    def keyframes_from_scenes(self):
        scenes = list(getattr(self.media, "scenes", []) or [])
        if self.media.index is None or not scenes:
            self.ctx.status.emit("Önce sahne değişimlerini bulun (Video > Sahne değişimlerini bul)")
            return 0
        before = len(self.media.index.keyframes)
        n = self.media.set_keyframes(set(self.media.index.keyframes) | set(scenes))
        self.ctx.status.emit(f"{n - before} sahne değişimi anahtar karelere katıldı")
        return n - before

    def _keyframes_changed(self):
        self.video._kf_cache_for = None
        self.audio.update()
        self.audio.view.update()
        self.strip.update()
