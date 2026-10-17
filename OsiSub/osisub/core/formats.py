"""Diğer altyazı biçimleri: SRT / WebVTT okuma, SRT yazma.

Okuma sonucu her zaman bir AssDocument'tır (tek "Default" stil), böylece
referans altyazı, içe aktarma ve aktarma aracı aynı modelle çalışır.
SRT etiketleri: <i> <b> <u> <s> -> {\\i1}… ; <font color="#RRGGBB"> -> {\\c&HBBGGRR&};
satır sonları -> \\N. Kodlama: UTF-8 (BOM'lu/BOM'suz), olmazsa cp1254 (Türkçe Windows).
"""

from __future__ import annotations

import re

from .assfile import AssDocument, Event
from .asstime import format_time, round_ms

_TIME = re.compile(r"(\d+):(\d{1,2}):(\d{1,2})[,.](\d{1,3})")
_ARROW = re.compile(r"(\d+:\d{1,2}:\d{1,2}[,.]\d{1,3})\s*-->\s*(\d+:\d{1,2}:\d{1,2}[,.]\d{1,3})")
_VTT_ARROW = re.compile(r"((?:\d+:)?\d{1,2}:\d{1,2}\.\d{1,3})\s*-->\s*((?:\d+:)?\d{1,2}:\d{1,2}\.\d{1,3})")


def decode_text(data: bytes) -> str:
    if data.startswith(b"\xef\xbb\xbf"):
        return data[3:].decode("utf-8", errors="replace")
    if data.startswith((b"\xff\xfe", b"\xfe\xff")):
        return data.decode("utf-16")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("cp1254", errors="replace")


def _ms(t: str) -> int:
    parts = t.replace(",", ".").split(":")
    if len(parts) == 2:
        parts = ["0"] + parts
    h, m, s = parts
    sec, _, frac = s.partition(".")
    frac = (frac + "000")[:3]
    return ((int(h) * 60 + int(m)) * 60 + int(sec)) * 1000 + int(frac)


def html_to_ass(text: str) -> str:
    def font(m):
        col = re.search(r'color\s*=\s*"?#?([0-9a-fA-F]{6})', m.group(0))
        if col:
            rgb = col.group(1)
            return "{\\c&H%s%s%s&}" % (rgb[4:6], rgb[2:4], rgb[0:2])
        return ""
    t = text
    t = re.sub(r"<font[^>]*>", font, t, flags=re.I)
    t = re.sub(r"</font>", "{\\\\c}", t, flags=re.I)
    for tag in "ibus":
        t = re.sub(rf"<{tag}>", "{\\\\%s1}" % tag, t, flags=re.I)
        t = re.sub(rf"</{tag}>", "{\\\\%s0}" % tag, t, flags=re.I)
    t = re.sub(r"<[^>]+>", "", t)                       # kalan HTML/VTT etiketleri
    t = t.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", "\\h")
    t = t.replace("}{", "")                              # ardışık blokları birleştir
    return t


def ass_to_html(text: str) -> str:
    """SRT yazarken: {\\i1} gibi temel etiketler korunur, diğerleri atılır."""
    out = []
    for m in re.finditer(r"\{([^}]*)\}|([^{]+)", text):
        if m.group(2) is not None:
            out.append(m.group(2))
            continue
        for tag, val in re.findall(r"\\([ibus])(\d)", m.group(1)):
            out.append(f"<{tag}>" if val == "1" else f"</{tag}>")
    s = "".join(out)
    return s.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", "\u00a0")


def _blocks(text: str):
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    return re.split(r"\n\s*\n", text.strip("\n"))


def parse_srt(text: str) -> list[Event]:
    events = []
    for blk in _blocks(text):
        lines = blk.split("\n")
        for i, ln in enumerate(lines[:3]):
            m = _ARROW.search(ln)
            if m:
                body = "\n".join(lines[i + 1:]).strip("\n")
                events.append(Event(start=round_ms(_ms(m.group(1))), end=round_ms(_ms(m.group(2))),
                                    text=html_to_ass(body).replace("\n", "\\N")))
                break
    return events


def parse_vtt(text: str) -> list[Event]:
    events = []
    for blk in _blocks(text):
        lines = blk.split("\n")
        for i, ln in enumerate(lines[:3]):
            m = _VTT_ARROW.search(ln)
            if m:
                body = "\n".join(lines[i + 1:]).strip("\n")
                events.append(Event(start=round_ms(_ms(m.group(1))), end=round_ms(_ms(m.group(2))),
                                    text=html_to_ass(body).replace("\n", "\\N")))
                break
    return events


def load_any(path: str) -> AssDocument:
    """.ass/.ssa -> bayt korumalı yükleme; .srt/.vtt -> tek stilli yeni belge."""
    low = path.lower()
    if low.endswith((".ass", ".ssa")):
        return AssDocument.load(path)
    with open(path, "rb") as f:
        text = decode_text(f.read())
    evs = parse_vtt(text) if low.endswith(".vtt") or text.lstrip().startswith("WEBVTT") else parse_srt(text)
    doc = AssDocument.new()
    doc.ensure_section("events").items = evs or [Event()]
    doc.path = None
    doc.source_path = path
    return doc


def to_srt(events, skip_comments: bool = True) -> str:
    out = []
    n = 0
    for ev in events:
        if skip_comments and ev.comment:
            continue
        n += 1
        out.append(str(n))
        out.append(f"{format_time_srt(ev.start)} --> {format_time_srt(ev.end)}")
        out.append(ass_to_html(ev.text))
        out.append("")
    return "\r\n".join(out).replace("\n", "\r\n").replace("\r\r\n", "\r\n")


def format_time_srt(ms: int) -> str:
    ms = max(0, int(ms))
    h, rem = divmod(ms, 3600000)
    m, rem = divmod(rem, 60000)
    s, ms = divmod(rem, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


__all__ = ["load_any", "parse_srt", "parse_vtt", "to_srt", "html_to_ass", "ass_to_html",
           "decode_text", "format_time", "format_time_srt"]
