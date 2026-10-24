"""SRT / WebVTT import & export, format sniffing."""
import re
from .ass import Document, Event, Style, decode_bytes

_SRT_T = r"(\d+):(\d+):(\d+)[,.](\d{1,3})"


def _t(h, m, s, f):
    return ((int(h) * 60 + int(m)) * 60 + int(s)) * 1000 + int(f.ljust(3, "0"))


def _html_to_ass(t: str) -> str:
    t = re.sub(r"</?([biu])>", lambda m: "{\\%s%d}" % (m.group(1), 0 if m.group(0).startswith("</") else 1), t, flags=re.I)
    t = re.sub(r"</?font[^>]*>", "", t, flags=re.I)
    return t.replace("\n", "\\N")


def _ass_to_html(t: str) -> str:
    t = re.sub(r"\{\\([biu])1\}", lambda m: "<%s>" % m.group(1), t)
    t = re.sub(r"\{\\([biu])0\}", lambda m: "</%s>" % m.group(1), t)
    t = re.sub(r"\{[^}]*\}", "", t)
    return t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")


def parse_srt(text: str) -> Document:
    doc = Document()
    blocks = re.split(r"\n\s*\n", text.replace("\r\n", "\n").replace("\r", "\n").strip())
    for b in blocks:
        lines = b.split("\n")
        for i, ln in enumerate(lines):
            m = re.match(rf"^\s*{_SRT_T}\s*-->\s*{_SRT_T}", ln)
            if m:
                g = m.groups()
                doc.events.append(Event(start=_t(*g[:4]), end=_t(*g[4:]), text=_html_to_ass("\n".join(lines[i + 1:]).strip())))
                break
    return doc


def parse_vtt(text: str) -> Document:
    doc = Document()
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    for b in re.split(r"\n\s*\n", text.strip()):
        lines = b.split("\n")
        for i, ln in enumerate(lines):
            m = re.match(r"^\s*(?:(\d+):)?(\d+):(\d+)\.(\d{3})\s*-->\s*(?:(\d+):)?(\d+):(\d+)\.(\d{3})", ln)
            if m:
                g = m.groups()
                a = _t(g[0] or 0, g[1], g[2], g[3]); z = _t(g[4] or 0, g[5], g[6], g[7])
                doc.events.append(Event(start=a, end=z, text=_html_to_ass("\n".join(lines[i + 1:]).strip())))
                break
    return doc


def _stamp(ms: int, sep: str) -> str:
    ms = max(0, ms)
    h, r = divmod(ms, 3600000); m, r = divmod(r, 60000); s, f = divmod(r, 1000)
    return f"{h:02d}:{m:02d}:{s:02d}{sep}{f:03d}"


def to_srt(doc: Document) -> str:
    out = []
    ev = sorted((e for e in doc.events if not e.comment), key=lambda e: (e.start, e.end))
    for i, e in enumerate(ev, 1):
        out.append(f"{i}\n{_stamp(e.start, ',')} --> {_stamp(e.end, ',')}\n{_ass_to_html(e.text)}\n")
    return "\n".join(out)


def to_vtt(doc: Document) -> str:
    out = ["WEBVTT\n"]
    for e in sorted((e for e in doc.events if not e.comment), key=lambda e: (e.start, e.end)):
        out.append(f"{_stamp(e.start, '.')} --> {_stamp(e.end, '.')}\n{_ass_to_html(e.text)}\n")
    return "\n".join(out)


def sniff(text: str) -> str:
    head = text.lstrip("﻿ \n\r\t")[:4000]
    if head.upper().startswith("WEBVTT"):
        return "vtt"
    if re.search(r"^\[(Script Info|V4\+? Styles)\]", head, re.I | re.M):
        return "ass"
    if re.search(_SRT_T + r"\s*-->", head):
        return "srt"
    return "ass"


def load_bytes(data: bytes) -> Document:
    text = decode_bytes(data)
    kind = sniff(text)
    return {"ass": Document.loads, "srt": parse_srt, "vtt": parse_vtt}[kind](text)
