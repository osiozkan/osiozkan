"""SRT / WebVTT / düz metin içe-dışa aktarma → AssFile."""
import re
from .ass import AssFile, Line, Style
from .encoding import decode_bytes

_TS = r"(\d+):(\d{2}):(\d{2})[,.](\d{1,3})"
_TIME = re.compile(rf"{_TS}\s*-->\s*{_TS}")
_VTT_TIME = re.compile(r"(?:(\d+):)?(\d{2}):(\d{2})[.,](\d{1,3})\s*-->\s*(?:(\d+):)?(\d{2}):(\d{2})[.,](\d{1,3})")


def _ms(h, m, s, f):
    return ((int(h or 0) * 60 + int(m)) * 60 + int(s)) * 1000 + int((f + "00")[:3])


def _html_to_ass(t: str) -> str:
    t = re.sub(r"<\s*b\s*>", r"{\\b1}", t, flags=re.I)
    t = re.sub(r"<\s*/\s*b\s*>", r"{\\b0}", t, flags=re.I)
    t = re.sub(r"<\s*i\s*>", r"{\\i1}", t, flags=re.I)
    t = re.sub(r"<\s*/\s*i\s*>", r"{\\i0}", t, flags=re.I)
    t = re.sub(r"<\s*u\s*>", r"{\\u1}", t, flags=re.I)
    t = re.sub(r"<\s*/\s*u\s*>", r"{\\u0}", t, flags=re.I)
    t = re.sub(r"<\s*font[^>]*color=[\"']?#?([0-9a-fA-F]{6})[\"']?[^>]*>",
               lambda m: "{\\c&H%s%s%s&}" % (m.group(1)[4:6], m.group(1)[2:4], m.group(1)[0:2]), t, flags=re.I)
    t = re.sub(r"<\s*/\s*font\s*>", "", t, flags=re.I)
    t = re.sub(r"<[^>]+>", "", t)
    t = re.sub(r"\{\\an(\d)\}", lambda m: m.group(0), t)
    return t.replace("\r", "").replace("\n", "\\N")


def _ass_to_html(t: str) -> str:
    t = re.sub(r"\{\\an?\d\}", "", t)
    t = t.replace("{\\i1}", "<i>").replace("{\\i0}", "</i>").replace("{\\b1}", "<b>").replace("{\\b0}", "</b>")
    t = re.sub(r"\{[^}]*\}", "", t)
    return t.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ")


def parse_srt(text: str) -> AssFile:
    a = AssFile.new()
    text = text.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = block.split("\n")
        for i, l in enumerate(lines[:3]):
            m = _TIME.search(l)
            if m:
                g = m.groups()
                a.events.append(Line(text=_html_to_ass("\n".join(lines[i + 1:])), start=_ms(*g[:4]), end=_ms(*g[4:])))
                break
    return a


def parse_vtt(text: str) -> AssFile:
    a = AssFile.new()
    text = text.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = block.split("\n")
        for i, l in enumerate(lines[:2]):
            m = _VTT_TIME.search(l)
            if m:
                g = m.groups()
                a.events.append(Line(text=_html_to_ass("\n".join(lines[i + 1:])),
                                     start=_ms(g[0], g[1], g[2], g[3]), end=_ms(g[4], g[5], g[6], g[7])))
                break
    return a


def _fmt(ms: int, sep: str) -> str:
    ms = max(0, int(ms))
    h, r = divmod(ms, 3600000)
    m, r = divmod(r, 60000)
    s, f = divmod(r, 1000)
    return f"{h:02d}:{m:02d}:{s:02d}{sep}{f:03d}"


def to_srt(a: AssFile) -> str:
    evs = sorted((e for e in a.events if not e.comment), key=lambda e: (e.start, e.end))
    return "\n".join(f"{i}\n{_fmt(e.start, ',')} --> {_fmt(e.end, ',')}\n{_ass_to_html(e.text)}\n"
                     for i, e in enumerate(evs, 1))


def to_vtt(a: AssFile) -> str:
    evs = sorted((e for e in a.events if not e.comment), key=lambda e: (e.start, e.end))
    return "WEBVTT\n\n" + "\n".join(f"{_fmt(e.start, '.')} --> {_fmt(e.end, '.')}\n{_ass_to_html(e.text)}\n" for e in evs)


def load_any(path: str) -> AssFile:
    with open(path, "rb") as f:
        text = decode_bytes(f.read())
    low = path.lower()
    if low.endswith(".srt"):
        return parse_srt(text)
    if low.endswith(".vtt"):
        return parse_vtt(text)
    return AssFile.parse(text)


def save_any(a: AssFile, path: str) -> None:
    low = path.lower()
    if low.endswith(".srt"):
        data = to_srt(a)
    elif low.endswith(".vtt"):
        data = to_vtt(a)
    else:
        a.save(path)
        return
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(data)
