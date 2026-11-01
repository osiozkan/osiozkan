"""Robust file reading/detection (UTF-8/16 BOM, Turkish-friendly legacy fallbacks)."""
from __future__ import annotations

import codecs

_LEGACY = ("cp1254", "cp1252", "latin-1")


def decode_bytes(data: bytes, hint: str | None = None) -> tuple[str, str]:
    if hint:
        return data.decode(hint, errors="replace"), hint
    if data.startswith(codecs.BOM_UTF8):
        return data[3:].decode("utf-8", errors="replace"), "utf-8-sig"
    if data.startswith((codecs.BOM_UTF16_LE, codecs.BOM_UTF16_BE)):
        return data.decode("utf-16"), "utf-16"
    if data.startswith((codecs.BOM_UTF32_LE, codecs.BOM_UTF32_BE)):
        return data.decode("utf-32"), "utf-32"
    try:
        return data.decode("utf-8"), "utf-8"
    except UnicodeDecodeError:
        pass
    for enc in _LEGACY:
        try:
            return data.decode(enc), enc
        except UnicodeDecodeError:
            continue
    return data.decode("utf-8", errors="replace"), "utf-8"


def read_text(path: str, hint: str | None = None) -> tuple[str, str]:
    with open(path, "rb") as fh:
        return decode_bytes(fh.read(), hint)
