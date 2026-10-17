"""Ana pencerenin 1.3 özellikleri (Aegisub araçları, satır işlemleri, anahtar kareler).

MainWindow bu sınıftan da türer; burada yalnız yöntemler vardır (eylemler ve menüler
mainwindow.py'de kurulur). Her yöntem headless (test) kipinde pencereyi açmadan döndürür.
"""

from __future__ import annotations

import os

from PySide6.QtWidgets import QApplication, QFileDialog, QMessageBox

from ..core import aegisub_tools as A


class FeaturesMixin:
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
