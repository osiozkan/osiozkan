"""Text helpers incl. Turkish-aware casing (i/İ, ı/I)."""
from __future__ import annotations

_TR_UP = str.maketrans({"i": "İ", "ı": "I"})
_TR_LOW = str.maketrans({"İ": "i", "I": "ı"})


def tr_upper(s: str) -> str:
    return s.translate(_TR_UP).upper()


def tr_lower(s: str) -> str:
    return s.translate(_TR_LOW).lower()


def tr_capitalize(s: str) -> str:
    return tr_upper(s[:1]) + tr_lower(s[1:]) if s else s


def tr_title(s: str) -> str:
    return " ".join(tr_capitalize(w) for w in s.split(" "))


_MOJI = __import__("re").compile(r"[ÃÄÅÂ][\u0080-¿ŒœŠšŸŽžƒˆ˜–-›€™ -ÿ]")


def looks_mojibake(s: str) -> bool:
    """True for UTF-8 text that was mis-decoded as a single-byte code page (e.g. 'Ã§' for 'ç', 'ÅŸ' for 'ş')."""
    return bool(_MOJI.search(s))


def fix_mojibake(s: str) -> str:
    """Repair UTF-8 mis-decoded as cp1252/cp1254/latin-1. Returns s unchanged if it can't be repaired cleanly."""
    if not looks_mojibake(s):
        return s
    for enc in ("cp1252", "cp1254", "latin-1"):
        try:
            fixed = s.encode(enc).decode("utf-8")
        except (UnicodeEncodeError, UnicodeDecodeError):
            continue
        if fixed != s:
            return fixed
    # partial: fix only the offending runs
    import re

    def run(m):
        t = m.group(0)
        for enc in ("cp1252", "cp1254", "latin-1"):
            try:
                return t.encode(enc).decode("utf-8")
            except (UnicodeEncodeError, UnicodeDecodeError):
                continue
        return t
    return re.sub(r"(?:[ÃÄÅÂ][^\x00-\x7f]\s?)+", run, s)
