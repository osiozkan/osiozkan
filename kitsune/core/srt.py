"""SRT / WebVTT <-> Script conversion."""
import re
from .ass import Script, Event, Style, Info, decode_bytes
from .timecode import ms_to_srt, srt_to_ms

_TIME = re.compile(r"(\d+:)?\d+:\d+[,.]\d+\s*-->\s*(\d+:)?\d+:\d+[,.]\d+")

def _html_to_ass(t):
    t = t.replace("\r", "")
    for a, b in (("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"),
                 ("<u>", "{\\u1}"), ("</u>", "{\\u0}"), ("<I>", "{\\i1}"), ("</I>", "{\\i0}")):
        t = t.replace(a, b)
    t = re.sub(r"<font color=\"?#?([0-9a-fA-F]{6})\"?>", lambda m: "{\\c&H%s%s%s&}" % (m.group(1)[4:6], m.group(1)[2:4], m.group(1)[0:2]), t)
    t = t.replace("</font>", "")
    t = re.sub(r"<[^>]+>", "", t)
    return t.replace("\n", "\\N")

def _ass_to_html(t):
    t = t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
    for a, b in (("{\\i1}", "<i>"), ("{\\i0}", "</i>"), ("{\\b1}", "<b>"), ("{\\b0}", "</b>"),
                 ("{\\u1}", "<u>"), ("{\\u0}", "</u>")):
        t = t.replace(a, b)
    return re.sub(r"\{[^}]*\}", "", t)

def parse_srt(text: str, vtt=False) -> Script:
    sc = Script(); text = text.lstrip("﻿").replace("\r\n", "\n").replace("\r", "\n")
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = block.split("\n")
        for i, l in enumerate(lines):
            if _TIME.search(l):
                a, b = re.split(r"\s*-->\s*", l.strip())[:2]
                b = b.split()[0]
                body = "\n".join(lines[i + 1:])
                sc.entries.append(Event(start=srt_to_ms(a), end=srt_to_ms(b), text=_html_to_ass(body)))
                break
    return sc

def load_srt(path, encoding=None):
    sc = parse_srt(decode_bytes(open(path, "rb").read(), encoding)); sc.filename = None; return sc

def dump_srt(sc: Script, vtt=False) -> str:
    out = ["WEBVTT", ""] if vtt else []
    evs = sorted((e for e in sc.events if not e.comment), key=lambda e: (e.start, e.end))
    for i, e in enumerate(evs, 1):
        if not vtt: out.append(str(i))
        sep = "." if vtt else ","
        out.append(f"{ms_to_srt(e.start, sep)} --> {ms_to_srt(e.end, sep)}")
        out.append(_ass_to_html(e.text)); out.append("")
    return "\n".join(out)
