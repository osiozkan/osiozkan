"""Spell checking through Hunspell dictionaries using the pure-Python `spylls` port (optional dependency)."""
from __future__ import annotations
import os
import re
from typing import Iterator, Optional

from .tags import BLOCK

SEARCH_DIRS = [
    os.path.expanduser("~/.osisub/dicts"), "/usr/share/hunspell", "/usr/share/myspell", "/usr/share/myspell/dicts",
    "/Library/Spelling", "C:/Program Files/LibreOffice/share/extensions/dict-tr",
]
WORD = re.compile(r"[^\W\d_]+(?:['’][^\W\d_]+)*", re.UNICODE)


def find_dictionary(lang: str) -> Optional[str]:
    """lang like 'tr_TR' / 'en_US'. Returns base path without extension."""
    for d in SEARCH_DIRS:
        base = os.path.join(d, lang)
        if os.path.isfile(base + ".dic") and os.path.isfile(base + ".aff"):
            return base
    return None


class SpellChecker:
    def __init__(self, lang: str = "tr_TR", base: Optional[str] = None):
        from spylls.hunspell import Dictionary  # optional dependency
        base = base or find_dictionary(lang)
        if not base:
            raise FileNotFoundError(f"Hunspell sözlüğü bulunamadı: {lang} (aranan: {', '.join(SEARCH_DIRS)})")
        self.lang = lang
        self.dic = Dictionary.from_files(base)
        self.user_path = os.path.expanduser(f"~/.osisub/user_{lang}.txt")
        self.user: set[str] = set()
        self.ignored: set[str] = set()
        if os.path.isfile(self.user_path):
            self.user = {l.strip() for l in open(self.user_path, encoding="utf-8") if l.strip()}

    def is_ok(self, word: str) -> bool:
        if word in self.user or word.lower() in self.user or word in self.ignored:
            return True
        if len(word) < 2 or word.isupper():  # acronyms / single letters
            return True
        w = word.replace("’", "'")
        return bool(self.dic.lookup(w) or self.dic.lookup(w.lower()) or
                    self.dic.lookup(w.replace("I", "ı").replace("İ", "i").lower()))

    def suggest(self, word: str, limit: int = 8) -> list[str]:
        out = []
        for s in self.dic.suggest(word):
            if s not in out:
                out.append(s)
            if len(out) >= limit:
                break
        return out

    def add(self, word: str) -> None:
        self.user.add(word)
        os.makedirs(os.path.dirname(self.user_path), exist_ok=True)
        with open(self.user_path, "a", encoding="utf-8") as f:
            f.write(word + "\n")

    def errors(self, text: str) -> Iterator[tuple[int, int, str]]:
        """Yield (start, end, word) of misspelled words in an ASS text, skipping {tags} and \\N escapes."""
        masked = BLOCK.sub(lambda m: " " * len(m.group(0)), text)
        masked = masked.replace("\\N", "  ").replace("\\n", "  ").replace("\\h", "  ")
        for m in WORD.finditer(masked):
            if not self.is_ok(m.group(0)):
                yield m.start(), m.end(), m.group(0)
