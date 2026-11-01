"""Import/export for other subtitle formats (SRT, WebVTT, MicroDVD, LRC, TXT) <-> ASS model."""
from __future__ import annotations

import os
import re

from .ass import ASSFile, Event, Style, parse_ass
from .io import read_text
from .tags import plain_text, split_blocks, parse_block_tags
from .timecode import ms_to_srt, srt_to_ms, frames_to_ms


# ------------------------------------------------------------------ html <-> ass

def html_to_ass(s: str) -> str:
    s = re.sub(r"</?(?:i|em)>", lambda m: "{\\i0}" if m.group().startswith("</") else "{\\i1}", s, flags=re.I)
    s = re.sub(r"</?(?:b|strong)>", lambda m: "{\\b0}" if m.group().startswith("</") else "{\\b1}", s, flags=re.I)
    s = re.sub(r"</?u>", lambda m: "{\\u0}" if m.group().startswith("</") else "{\\u1}", s, flags=re.I)
    s = re.sub(r"</?s>", lambda m: "{\\s0}" if m.group().startswith("</") else "{\\s1}", s, flags=re.I)

    def font(m):
        col = re.search(r'color\s*=\s*["\']?#?([0-9a-fA-F]{6})', m.group(0))
        if col:
            h = col.group(1)
            return "{\\c&H%s%s%s&}" % (h[4:6].upper(), h[2:4].upper(), h[0:2].upper())
        return ""
    s = re.sub(r"<font[^>]*>", font, s, flags=re.I)
    s = re.sub(r"</font>", "{\\\\c}", s, flags=re.I)
    s = re.sub(r"<[^>]+>", "", s)
    s = re.sub(r"\{\\an\d\}", lambda m: m.group(0), s)
    return s.replace("\r", "").replace("\n", "\\N")


def ass_to_html(text: str, keep_style: bool = True) -> str:
    out = []
    open_i = open_b = open_u = False
    drawing = False
    for is_tag, part in split_blocks(text):
        if is_tag:
            for n, a in parse_block_tags(part):
                if n == "p":
                    drawing = a not in ("", "0")
                if not keep_style:
                    continue
                on = a not in ("0",)
                if n == "i" and on != open_i:
                    out.append("<i>" if on else "</i>"); open_i = on
                elif n == "b" and on != open_b:
                    out.append("<b>" if on else "</b>"); open_b = on
                elif n == "u" and on != open_u:
                    out.append("<u>" if on else "</u>"); open_u = on
            continue
        if not drawing:
            out.append(part.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " "))
    if open_u: out.append("</u>")
    if open_b: out.append("</b>")
    if open_i: out.append("</i>")
    return "".join(out).strip()


# ------------------------------------------------------------------ SRT

_SRT_TIME = re.compile(r"(\d+:\d{1,2}:\d{1,2}[,.]\d{1,3})\s*-->\s*(\d+:\d{1,2}:\d{1,2}[,.]\d{1,3})")


def parse_srt(text: str) -> ASSFile:
    f = ASSFile.new()
    f.set_info("Title", "Imported SRT")
    text = text.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = block.split("\n")
        idx = None
        for i, l in enumerate(lines[:3]):
            m = _SRT_TIME.search(l)
            if m:
                idx = i
                break
        if idx is None:
            continue
        m = _SRT_TIME.search(lines[idx])
        body = "\n".join(lines[idx + 1:]).strip()
        e = Event(start_time=srt_to_ms(m.group(1)), end_time=srt_to_ms(m.group(2)),
                  text=html_to_ass(body))
        an = re.match(r"\{\\an(\d)\}", body)
        if an:
            e.text = "{\\an%s}" % an.group(1) + html_to_ass(body[an.end():])
        f.lines.append(e)
    return f


def write_srt(f: ASSFile, keep_style: bool = True) -> str:
    out = []
    n = 0
    for e in sorted((e for e in f.events if not e.comment), key=lambda e: (e.start_time, e.end_time)):
        body = ass_to_html(e.text, keep_style)
        if not body.strip():
            continue
        n += 1
        out.append(f"{n}\r\n{ms_to_srt(e.start_time)} --> {ms_to_srt(e.end_time)}\r\n{body.replace(chr(10), chr(13)+chr(10))}\r\n")
    return "\r\n".join(out)


# ------------------------------------------------------------------ VTT

def parse_vtt(text: str) -> ASSFile:
    f = ASSFile.new()
    text = text.lstrip("\ufeff").replace("\r\n", "\n")
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = block.split("\n")
        for i, l in enumerate(lines[:2]):
            m = _SRT_TIME.search(l)
            if m:
                body = "\n".join(lines[i + 1:])
                body = re.sub(r"<\d+:\d+[:.]\d+(?:\.\d+)?>", "", body)
                body = re.sub(r"</?c(?:\.[^>]*)?>|</?v[^>]*>|</?lang[^>]*>", "", body)
                f.lines.append(Event(start_time=srt_to_ms(m.group(1)), end_time=srt_to_ms(m.group(2)),
                                     text=html_to_ass(body)))
                break
    return f


def write_vtt(f: ASSFile) -> str:
    out = ["WEBVTT", ""]
    for e in sorted((e for e in f.events if not e.comment), key=lambda e: e.start_time):
        body = ass_to_html(e.text)
        if body:
            out += [f"{ms_to_srt(e.start_time, '.')} --> {ms_to_srt(e.end_time, '.')}", body, ""]
    return "\n".join(out)


# ------------------------------------------------------------------ MicroDVD / LRC / TXT

def parse_microdvd(text: str, fps: float = 23.976) -> ASSFile:
    f = ASSFile.new()
    for l in text.splitlines():
        m = re.match(r"\{(\d+)\}\{(\d+)\}(.*)", l)
        if not m:
            continue
        if m.group(1) == "1" and m.group(2) == "1":
            try:
                fps = float(m.group(3))
            except ValueError:
                pass
            continue
        body = m.group(3).replace("|", "\\N")
        body = re.sub(r"\{y:i\}", "{\\\\i1}", body)
        f.lines.append(Event(start_time=frames_to_ms(int(m.group(1)), fps),
                             end_time=frames_to_ms(int(m.group(2)), fps), text=body))
    return f


def parse_lrc(text: str) -> ASSFile:
    f = ASSFile.new()
    items = []
    for l in text.splitlines():
        m = re.match(r"\[(\d+):(\d+(?:\.\d+)?)\](.*)", l)
        if m:
            items.append((int((int(m.group(1)) * 60 + float(m.group(2))) * 1000), m.group(3).strip()))
    items.sort()
    for i, (t, s) in enumerate(items):
        end = items[i + 1][0] if i + 1 < len(items) else t + 4000
        f.lines.append(Event(start_time=t, end_time=end, text=s))
    return f


def write_txt(f: ASSFile) -> str:
    return "\n".join(plain_text(e.text).replace("\n", " ") for e in f.events if not e.comment)


# ------------------------------------------------------------------ dispatch

def detect_format(path: str, text: str) -> str:
    ext = os.path.splitext(path)[1].lower().lstrip(".")
    head = text[:2000]
    if ext in ("ass", "ssa") or "[Script Info]" in head or "[V4+ Styles]" in head:
        return "ass"
    if ext == "vtt" or head.lstrip("\ufeff").startswith("WEBVTT"):
        return "vtt"
    if ext == "srt" or _SRT_TIME.search(head):
        return "srt"
    if ext == "sub" or re.search(r"^\{\d+\}\{\d+\}", head, re.M):
        return "microdvd"
    if ext == "lrc":
        return "lrc"
    return "ass"


def load_file(path: str, encoding: str | None = None) -> tuple[ASSFile, str, str]:
    """Returns (doc, format, encoding)."""
    text, enc = read_text(path, encoding)
    fmt = detect_format(path, text)
    doc = {"ass": parse_ass, "srt": parse_srt, "vtt": parse_vtt, "microdvd": parse_microdvd, "lrc": parse_lrc}[fmt](text)
    return doc, fmt, enc


def export_text(doc: ASSFile, fmt: str) -> str:
    fmt = fmt.lower()
    if fmt in ("ass", "ssa"):
        return doc.to_string()
    if fmt == "srt":
        return write_srt(doc)
    if fmt == "vtt":
        return write_vtt(doc)
    if fmt == "txt":
        return write_txt(doc)
    raise ValueError(f"unsupported export format: {fmt}")
