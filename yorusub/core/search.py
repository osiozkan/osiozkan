"""Bul/Değiştir (düz metin veya regex, Türkçe büyük/küçük harf duyarlı)."""
import re
from .ass import AssFile


def tr_lower(s: str) -> str:
    return s.replace("I", "ı").replace("İ", "i").lower()


def tr_upper(s: str) -> str:
    return s.replace("i", "İ").replace("ı", "I").upper()


def find(a: AssFile, pattern: str, *, regex=False, case=False, field="text") -> list[int]:
    flags = 0 if case else re.I
    rx = re.compile(pattern if regex else re.escape(pattern), flags)
    return [i for i, e in enumerate(a.events) if rx.search(getattr(e, field))]


def replace(a: AssFile, pattern: str, repl: str, *, regex=False, case=False, field="text") -> int:
    flags = 0 if case else re.I
    rx = re.compile(pattern if regex else re.escape(pattern), flags)
    n = 0
    for e in a.events:
        new, k = rx.subn(repl if regex else repl.replace("\\", "\\\\"), getattr(e, field))
        if k:
            setattr(e, field, new); n += k
    return n
