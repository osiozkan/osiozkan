"""Türkçe yazım denetimi (Hunspell sözlüğü, saf Python `spylls` ile).

Sözlük arama sırası (ilk bulunan):
  %APPDATA%\\OsiSub\\dictionaries\\   (ya da ~/.config/osisub/dictionaries)
  <OsiSub>\\dictionaries\\
  %APPDATA%\\Aegisub\\dictionaries\\  (Aegisub'a kurulu sözlük varsa o kullanılır)
  /usr/share/hunspell, /usr/share/myspell
Dosyalar: tr_TR.aff + tr_TR.dic. Yoksa uygulama içinden indirilebilir
(LibreOffice sözlük deposu, ~38 MB). Yükleme ~7 sn sürer; arka planda yapılır.

Türkçe kurallar:
  * Özel isim + kesme işareti: "Ali'nin", "İstanbul'da" -> kök ("Ali", "İstanbul") denetlenir;
    kök büyük harfle başlıyorsa sözlükte olmasa da kabul edilir (özel isim).
  * Tireli birleşikler parça parça denetlenir.
  * Kullanıcı sözlüğü: <kullanıcı klasörü>/kullanici_sozlugu.txt (satır başına bir kelime).
"""

from __future__ import annotations

import os
import sys
import threading
import unicodedata

DICT_URLS = {
    "tr_TR": ("https://raw.githubusercontent.com/LibreOffice/dictionaries/master/tr_TR/tr_TR.aff",
              "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/tr_TR/tr_TR.dic"),
}


def dictionary_dirs() -> list[str]:
    from .. import config
    dirs = [os.path.join(config.user_dir(), "dictionaries"),
            os.path.join(os.path.dirname(config.app_dir()), "dictionaries")]
    if sys.platform.startswith("win"):
        ap = os.environ.get("APPDATA", "")
        if ap:
            dirs.append(os.path.join(ap, "Aegisub", "dictionaries"))
    else:
        dirs += [os.path.expanduser("~/.aegisub/dictionaries"), "/usr/share/hunspell", "/usr/share/myspell"]
    return dirs


def find_dictionary(lang: str = "tr_TR") -> str | None:
    """'.../tr_TR' (uzantısız taban yol) ya da None."""
    for d in dictionary_dirs():
        base = os.path.join(d, lang)
        if os.path.isfile(base + ".aff") and os.path.isfile(base + ".dic"):
            return base
    return None


def download_dictionary(lang: str = "tr_TR", dest: str | None = None, progress=None) -> str:
    """Sözlüğü indirir; taban yolu döndürür."""
    import urllib.request
    from .. import config
    dest = dest or os.path.join(config.user_dir(), "dictionaries")
    os.makedirs(dest, exist_ok=True)
    urls = DICT_URLS[lang]
    for k, url in enumerate(urls):
        out = os.path.join(dest, os.path.basename(url))
        tmp = out + ".part"
        with urllib.request.urlopen(url, timeout=60) as r, open(tmp, "wb") as f:
            total = int(r.headers.get("Content-Length") or 0)
            got = 0
            while True:
                chunk = r.read(1 << 16)
                if not chunk:
                    break
                f.write(chunk)
                got += len(chunk)
                if progress and total:
                    progress((k + got / total) / len(urls))
        os.replace(tmp, out)
    return os.path.join(dest, lang)


class SpellChecker:
    def __init__(self, base: str | None = None, user_words: str | None = None):
        self.base = base
        self.dic = None
        self.error = ""
        self.ready = False
        self._cache: dict[str, bool] = {}
        self.ignored: set[str] = set()
        self.user_path = user_words
        self.user_words: set[str] = set()
        if user_words and os.path.exists(user_words):
            with open(user_words, encoding="utf-8") as f:
                self.user_words = {ln.strip() for ln in f if ln.strip()}

    def load(self) -> bool:
        if not self.base:
            self.error = "Sözlük bulunamadı"
            return False
        try:
            from spylls.hunspell import Dictionary
        except ImportError:
            self.error = "spylls kurulu değil (pip install spylls)"
            return False
        try:
            self.dic = Dictionary.from_files(self.base)
        except Exception as e:  # noqa: BLE001
            self.error = f"Sözlük okunamadı: {e}"
            return False
        self.ready = True
        return True

    def load_async(self, done=None):
        def work():
            self.load()
            if done:
                done(self)
        threading.Thread(target=work, name="osisub-spell", daemon=True).start()

    # ------------------------------------------------------------ denetim
    def _lookup(self, w: str) -> bool:
        try:
            return bool(self.dic.lookup(w))
        except Exception:  # noqa: BLE001
            return True

    def check(self, word: str) -> bool:
        if not self.ready:
            return True
        w = unicodedata.normalize("NFC", word).replace("’", "'")
        hit = self._cache.get(w)
        if hit is not None:
            return hit
        ok = self._check(w)
        if len(self._cache) > 50000:
            self._cache.clear()
        self._cache[w] = ok
        return ok

    def _check(self, w: str) -> bool:
        if len(w) < 2 or w in self.user_words or w in self.ignored:
            return True
        if self._lookup(w):
            return True
        if "'" in w:
            stem = w.split("'", 1)[0]
            if stem and (stem[0].isupper() or self._lookup(stem) or stem in self.user_words):
                return True
        if "-" in w:
            parts = [p for p in w.split("-") if p]
            if parts and all(self._check(p) for p in parts):
                return True
        if w.isupper() and len(w) <= 4:        # kısaltma (TV, NHK ...)
            return True
        return False

    def suggest(self, word: str, n: int = 8) -> list[str]:
        if not self.ready:
            return []
        w = unicodedata.normalize("NFC", word).replace("’", "'")
        stem, sep, rest = w.partition("'")
        try:
            out = []
            for s in self.dic.suggest(stem if sep else w):
                out.append(s + (sep + rest if sep else ""))
                if len(out) >= n:
                    break
            return out
        except Exception:  # noqa: BLE001
            return []

    def add_word(self, word: str):
        w = unicodedata.normalize("NFC", word)
        self.user_words.add(w)
        self._cache.pop(w, None)
        if self.user_path:
            os.makedirs(os.path.dirname(self.user_path), exist_ok=True)
            with open(self.user_path, "a", encoding="utf-8") as f:
                f.write(w + "\n")

    def ignore(self, word: str):
        w = unicodedata.normalize("NFC", word)
        self.ignored.add(w)
        self._cache.pop(w, None)
