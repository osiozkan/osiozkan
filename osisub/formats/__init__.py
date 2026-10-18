"""Subtitle format readers/writers. Everything converts to/from core.ass.Document."""
from __future__ import annotations
import os
import re
from ..core.ass import Document, Event, decode_bytes
from ..core.timeutil import parse_srt_time, format_srt_time, format_vtt_time
from ..core.tags import srt_to_ass_text, ass_to_srt_text

_ARROW = re.compile(r"(\d+:\d+:\d+[,.]\d+|\d+:\d+[,.]\d+)\s*-->\s*(\d+:\d+:\d+[,.]\d+|\d+:\d+[,.]\d+)")


def _ts(s: str) -> int:
    if s.count(":") == 1:
        s = "00:" + s
    return parse_srt_time(s)


def parse_srt(text: str, vtt: bool = False) -> Document:
    doc = Document.new()
    doc.info = [(k, v) for k, v in doc.info if k != "Title"] + [("Title", "Imported")]
    doc.set_info("PlayResX", "1280")
    doc.set_info("PlayResY", "720")
    doc.styles[0].fontsize = 40
    text = text.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
    for block in re.split(r"\n\s*\n", text):
        lines = [l for l in block.split("\n")]
        for i, l in enumerate(lines):
            m = _ARROW.search(l)
            if m:
                body = "\n".join(lines[i + 1:]).strip("\n")
                if vtt:
                    body = re.sub(r"<\d+:\d+[:.]\d+(?:\.\d+)?>", "", body)
                    body = re.sub(r"</?c[^>]*>", "", body)
                    body = re.sub(r"</?v[^>]*>", "", body)
                if not body.strip():
                    break
                doc.events.append(Event(start=_ts(m.group(1)), end=_ts(m.group(2)),
                                        text=srt_to_ass_text(body)))
                break
    return doc


def dump_srt(doc: Document) -> str:
    out = []
    n = 0
    for e in sorted((e for e in doc.events if not e.comment), key=lambda e: (e.start, e.end)):
        n += 1
        out.append(f"{n}\n{format_srt_time(e.start)} --> {format_srt_time(e.end)}\n{ass_to_srt_text(e.text)}\n")
    return "\n".join(out)


def dump_vtt(doc: Document) -> str:
    out = ["WEBVTT\n"]
    for e in sorted((e for e in doc.events if not e.comment), key=lambda e: (e.start, e.end)):
        out.append(f"{format_vtt_time(e.start)} --> {format_vtt_time(e.end)}\n{ass_to_srt_text(e.text)}\n")
    return "\n".join(out)


def dump_txt(doc: Document) -> str:
    from ..core.tags import strip_tags
    return "\n".join(strip_tags(e.text) for e in doc.events if not e.comment) + "\n"


def load_any(path: str) -> Document:
    data = open(path, "rb").read()
    text = decode_bytes(data)
    ext = os.path.splitext(path)[1].lower()
    if ext in (".ass", ".ssa") or text.lstrip("\ufeff").lstrip().startswith("[Script Info]"):
        doc = Document.parse(text)
    elif ext == ".vtt" or text.lstrip("\ufeff").startswith("WEBVTT"):
        doc = parse_srt(text, vtt=True)
    else:
        doc = parse_srt(text)
    doc.filename = path if ext in (".ass", ".ssa") else None
    return doc


def save_any(doc: Document, path: str) -> None:
    ext = os.path.splitext(path)[1].lower()
    if ext in (".ass", ".ssa"):
        doc.save(path)
    elif ext == ".srt":
        open(path, "w", encoding="utf-8-sig", newline="\r\n").write(dump_srt(doc))
    elif ext == ".vtt":
        open(path, "w", encoding="utf-8", newline="\n").write(dump_vtt(doc))
    elif ext == ".txt":
        open(path, "w", encoding="utf-8", newline="\n").write(dump_txt(doc))
    else:
        raise ValueError(f"unsupported output format: {ext}")
