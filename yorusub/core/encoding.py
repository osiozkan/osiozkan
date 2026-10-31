"""Kodlama tespiti (UTF-8/16, BOM, Türkçe için cp1254 yedeği)."""
import codecs


def decode_bytes(data: bytes) -> str:
    if data.startswith(codecs.BOM_UTF8):
        return data[3:].decode("utf-8")
    if data.startswith((codecs.BOM_UTF16_LE, codecs.BOM_UTF16_BE)):
        return data.decode("utf-16")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        pass
    for enc in ("cp1254", "cp1252", "latin-1"):
        try:
            return data.decode(enc)
        except UnicodeDecodeError:
            continue
    return data.decode("utf-8", errors="replace")


def read_text(path: str) -> str:
    with open(path, "rb") as f:
        return decode_bytes(f.read())
