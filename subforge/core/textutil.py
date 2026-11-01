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
