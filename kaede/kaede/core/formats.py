"""Import/export: ASS/SSA, SRT, WebVTT, SBV, plain text. Encoding detection (UTF-8/16, cp1254, cp1252...)."""
from __future__ import annotations
import html
import os
import re

from .ass import Document, Event, Style
from .timeutil import srt_time, vtt_time, parse_srt_time

ENCODING_FALLBACKS = ["cp1254", "cp1252", "latin-1"]


def decode_bytes(data: bytes, hint: str | None = None) -> str:
    if hint:
        return data.decode(hint, errors="replace")
    if data.startswith(b"\xef\xbb\xbf"):
        return data[3:].decode("utf-8")
    if data.startswith((b"\xff\xfe", b"\xfe\xff")):
        return data.decode("utf-16")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        pass
    # no BOM utf-16?
    if len(data) > 4 and data[1:2] == b"\x00" and data[3:4] == b"\x00":
        try:
            return data.decode("utf-16-le")
        except UnicodeDecodeError:
            pass
    # Turkish legacy: pick cp1254 if it contains typical Turkish letters, else cp1252
    for enc in ("cp1254", "cp1252"):
        try:
            t = data.decode(enc)
            return t
        except UnicodeDecodeError:
            continue
    return data.decode("latin-1")


def read_text(path: str, encoding: str | None = None) -> str:
    with open(path, "rb") as f:
        return decode_bytes(f.read(), encoding)


# --- tag conversion -------------------------------------------------------
_HTML_TAG = re.compile(r"</?(i|b|u|s|em|strong)\s*>", re.I)


def srt_to_ass_text(t: str) -> str:
    t = t.replace("\r", "")
    t = re.sub(r"<i>", r"{\\i1}", t, flags=re.I)
    t = re.sub(r"</i>", r"{\\i0}", t, flags=re.I)
    t = re.sub(r"<b>", r"{\\b1}", t, flags=re.I)
    t = re.sub(r"</b>", r"{\\b0}", t, flags=re.I)
    t = re.sub(r"<u>", r"{\\u1}", t, flags=re.I)
    t = re.sub(r"</u>", r"{\\u0}", t, flags=re.I)
    t = re.sub(r"<s>", r"{\\s1}", t, flags=re.I)
    t = re.sub(r"</s>", r"{\\s0}", t, flags=re.I)
    t = re.sub(r'<font\s+color="?#?([0-9a-fA-F]{6})"?\s*>',
               lambda m: "{\\c&H%s%s%s&}" % (m.group(1)[4:6], m.group(1)[2:4], m.group(1)[0:2]), t, flags=re.I)
    t = re.sub(r"</font>", "", t, flags=re.I)
    t = re.sub(r"\{\\an(\d)\}", lambda m: "{\\an%s}" % m.group(1), t)
    t = re.sub(r"<[^>]+>", "", t)
    t = html.unescape(t)
    t = t.replace("\n", "\\N")
    # merge adjacent blocks
    return re.sub(r"\}\{", "", t)


def ass_to_srt_text(t: str, keep_tags: bool = True) -> str:
    t = t.replace("\\h", "\u00a0")
    out, italic, bold, under = [], False, False, False
    pos = 0
    for m in re.finditer(r"\{([^}]*)\}", t):
        out.append(t[pos:m.start()])
        pos = m.end()
        if not keep_tags:
            continue
        for tag, val in re.findall(r"\\(i|b|u)(\d)", m.group(1)):
            on = val != "0"
            if tag == "i" and on != italic:
                out.append("<i>" if on else "</i>"); italic = on
            elif tag == "b" and on != bold:
                out.append("<b>" if on else "</b>"); bold = on
            elif tag == "u" and on != under:
                out.append("<u>" if on else "</u>"); under = on
    out.append(t[pos:])
    s = "".join(out)
    if italic: s += "</i>"
    if bold: s += "</b>"
    if under: s += "</u>"
    s = s.replace("\\N", "\n").replace("\\n", "\n")
    return s


# --- SRT ------------------------------------------------------------------
_SRT_BLOCK = re.compile(
    r"(?:^|\n)\s*(\d+)?\s*\n?\s*(\d+:\d+:\d+[,.]\d+)\s*-->\s*(\d+:\d+:\d+[,.]\d+)[^\n]*\n(.*?)(?=\n\s*\n|\Z)",
    re.S)


def parse_srt(text: str, style: str = "Default") -> Document:
    text = text.replace("\ufeff", "").replace("\r\n", "\n").replace("\r", "\n")
    doc = Document()
    doc.events = []
    for m in _SRT_BLOCK.finditer(text + "\n"):
        body = m.group(4).strip("\n")
        doc.events.append(Event(start=parse_srt_time(m.group(2)), end=parse_srt_time(m.group(3)),
                                style=style, text=srt_to_ass_text(body)))
    return doc


def dump_srt(doc: Document, keep_tags: bool = True) -> str:
    out = []
    n = 0
    for e in doc.events:
        if e.comment:
            continue
        from .tags import has_drawing
        if has_drawing(e.text):
            continue
        n += 1
        out.append(f"{n}\n{srt_time(e.start)} --> {srt_time(e.end)}\n{ass_to_srt_text(e.text, keep_tags)}\n")
    return "\n".join(out)


# --- WebVTT ----------------------------------------------------------------
def parse_vtt(text: str, style: str = "Default") -> Document:
    text = text.replace("\ufeff", "").replace("\r\n", "\n").replace("\r", "\n")
    text = re.sub(r"^WEBVTT[^\n]*\n", "", text)
    text = re.sub(r"(?m)^(NOTE|STYLE|REGION)\b.*?(?:\n\n|\Z)", "", text, flags=re.S)
    return parse_srt(text, style)


def dump_vtt(doc: Document) -> str:
    out = ["WEBVTT\n"]
    for e in doc.events:
        if e.comment:
            continue
        out.append(f"{vtt_time(e.start)} --> {vtt_time(e.end)}\n{ass_to_srt_text(e.text)}\n")
    return "\n".join(out)


# --- SBV (YouTube) -----------------------------------------------------------
def parse_sbv(text: str, style: str = "Default") -> Document:
    doc = Document(); doc.events = []
    for block in re.split(r"\n\s*\n", text.replace("\r\n", "\n").strip()):
        ls = block.split("\n")
        m = re.match(r"(\d+:\d+:\d+\.\d+),(\d+:\d+:\d+\.\d+)", ls[0])
        if m:
            doc.events.append(Event(start=parse_srt_time(m.group(1)), end=parse_srt_time(m.group(2)),
                                    style=style, text="\\N".join(ls[1:])))
    return doc


def dump_sbv(doc: Document) -> str:
    return "\n".join(f"{vtt_time(e.start)},{vtt_time(e.end)}\n{ass_to_srt_text(e.text, False)}\n"
                     for e in doc.events if not e.comment)


# --- plain text --------------------------------------------------------------
def dump_txt(doc: Document, per_line: bool = True) -> str:
    from .tags import strip_tags
    return "\n".join(strip_tags(e.text).replace("\\N", " ").replace("\\n", " ")
                     for e in doc.events if not e.comment) + "\n"


# --- generic -----------------------------------------------------------------
def load_document(path: str, encoding: str | None = None) -> Document:
    ext = os.path.splitext(path)[1].lower()
    text = read_text(path, encoding)
    if ext in (".ass", ".ssa"):
        return Document.parse(text)
    if ext == ".srt":
        return parse_srt(text)
    if ext == ".vtt":
        return parse_vtt(text)
    if ext == ".sbv":
        return parse_sbv(text)
    if ext == ".txt":
        doc = Document(); doc.events = []
        t = 0
        for ln in text.splitlines():
            if ln.strip():
                doc.events.append(Event(start=t, end=t + 2000, text=ln.strip())); t += 2000
        return doc
    # sniff
    if "[Script Info]" in text[:2000]:
        return Document.parse(text)
    if text.lstrip().startswith("WEBVTT"):
        return parse_vtt(text)
    return parse_srt(text)


def save_document(doc: Document, path: str, bom: bool = True, keep_tags: bool = True) -> None:
    ext = os.path.splitext(path)[1].lower()
    enc = "utf-8-sig" if bom and ext not in (".vtt",) else "utf-8"
    if ext in (".ass", ".ssa"):
        data = doc.dump()
    elif ext == ".srt":
        data = dump_srt(doc, keep_tags)
    elif ext == ".vtt":
        data = dump_vtt(doc)
    elif ext == ".sbv":
        data = dump_sbv(doc)
    elif ext == ".txt":
        data = dump_txt(doc)
    else:
        raise ValueError(f"unsupported format: {ext}")
    with open(path, "w", encoding=enc, newline="\r\n" if ext in (".ass", ".ssa", ".srt") else "\n") as f:
        f.write(data)
