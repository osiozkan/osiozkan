"""Yazım denetimi servisi (arayüz tarafı): arka planda yükleme, sözlük indirme."""

from __future__ import annotations

import os
import threading

from PySide6.QtCore import QObject, Signal

from ..core.spell import SpellChecker, download_dictionary, find_dictionary


class SpellService(QObject):
    ready_changed = Signal(bool)
    progress = Signal(str)
    download_finished = Signal(bool, str)
    _loaded = Signal(object)
    _dl_done = Signal(bool, str)

    def __init__(self, config):
        super().__init__()
        self.config = config
        self.checker: SpellChecker | None = None
        self.loading = False
        self._loaded.connect(self._on_loaded)
        self._dl_done.connect(self._on_download)

    @property
    def ready(self) -> bool:
        return self.checker is not None and self.checker.ready

    def check(self, word: str) -> bool:
        return self.checker.check(word) if self.ready else True

    def suggest(self, word: str) -> list[str]:
        return self.checker.suggest(word) if self.ready else []

    def add_word(self, word: str):
        if self.checker:
            self.checker.add_word(word)
            self.ready_changed.emit(True)

    def ignore(self, word: str):
        if self.checker:
            self.checker.ignore(word)
            self.ready_changed.emit(True)

    @staticmethod
    def user_words_path() -> str:
        from .. import config as cfgmod
        return os.path.join(cfgmod.user_dir(), "kullanici_sozlugu.txt")

    def start(self, lang: str | None = None, blocking: bool = False) -> bool:
        """Sözlük bulunursa yükler (varsayılan: arka planda). Bulunamazsa False."""
        if self.loading or self.ready:
            return True
        lang = lang or self.config["spell_lang"] or "tr_TR"
        base = find_dictionary(lang)
        if not base:
            return False
        self.loading = True
        self.progress.emit("Türkçe sözlük yükleniyor…")
        sc = SpellChecker(base, self.user_words_path())
        if blocking:
            sc.load()
            self._on_loaded(sc)
        else:
            sc.load_async(lambda c: self._loaded.emit(c))
        return True

    def _on_loaded(self, sc):
        self.loading = False
        if sc.ready:
            self.checker = sc
            self.progress.emit("Yazım denetimi hazır")
            self.ready_changed.emit(True)
        else:
            self.progress.emit(f"Yazım denetimi yüklenemedi: {sc.error}")
            self.ready_changed.emit(False)

    def download(self):
        """Sözlüğü indirir (arka planda), bitince yükler."""
        lang = self.config["spell_lang"] or "tr_TR"

        def work():
            try:
                download_dictionary(lang, progress=lambda p: self.progress.emit(f"Sözlük indiriliyor %{int(p * 100)}"))
                self._dl_done.emit(True, "")
            except Exception as e:  # noqa: BLE001
                self._dl_done.emit(False, str(e))

        threading.Thread(target=work, name="osisub-dict-dl", daemon=True).start()

    def _on_download(self, ok, err):
        self.download_finished.emit(ok, err)
        if ok:
            self.start()
        else:
            self.progress.emit(f"Sözlük indirilemedi: {err}")
