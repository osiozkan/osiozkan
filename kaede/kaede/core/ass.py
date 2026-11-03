"""ASS/SSA document model, parser and writer (Aegisub compatible)."""
from __future__ import annotations
import copy
import re
from dataclasses import dataclass, field
from typing import Optional

from .timeutil import parse_ass_time, ass_time

STYLE_FIELDS = ["Name", "Fontname", "Fontsize", "PrimaryColour", "SecondaryColour",
                "OutlineColour", "BackColour", "Bold", "Italic", "Underline",
                "StrikeOut", "ScaleX", "ScaleY", "Spacing", "Angle", "BorderStyle",
                "Outline", "Shadow", "Alignment", "MarginL", "MarginR", "MarginV",
                "Encoding"]
EVENT_FIELDS = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR",
                "MarginV", "Effect", "Text"]


def _num(v, default=0.0):
    try:
        return float(str(v).strip())
    except ValueError:
        return default


def _int(v, default=0):
    try:
        return int(float(str(v).strip()))
    except ValueError:
        return default


def parse_color(s: str) -> tuple[int, int, int, int]:
    """'&HAABBGGRR' (or decimal, or &HBBGGRR&) -> (r, g, b, a)."""
    s = str(s).strip()
    try:
        if s.lower().startswith(("&h", "h")) or s.startswith("&"):
            v = int(s.strip("&").lstrip("hH") or "0", 16)
        else:
            v = int(s) & 0xFFFFFFFF
    except ValueError:
        v = 0
    return (v & 255, (v >> 8) & 255, (v >> 16) & 255, (v >> 24) & 255)


def format_color(r: int, g: int, b: int, a: int = 0) -> str:
    return f"&H{a:02X}{b:02X}{g:02X}{r:02X}"


@dataclass
class Style:
    name: str = "Default"
    fontname: str = "Arial"
    fontsize: float = 48
    primary: str = "&H00FFFFFF"
    secondary: str = "&H000000FF"
    outline: str = "&H00000000"
    back: str = "&H00000000"
    bold: bool = False
    italic: bool = False
    underline: bool = False
    strikeout: bool = False
    scale_x: float = 100
    scale_y: float = 100
    spacing: float = 0
    angle: float = 0
    border_style: int = 1
    outline_w: float = 2
    shadow: float = 2
    align: int = 2
    margin_l: int = 10
    margin_r: int = 10
    margin_t: int = 10
    encoding: int = 1

    @staticmethod
    def _b(v):
        return str(v).strip() in ("-1", "1")

    @classmethod
    def from_fields(cls, vals: dict[str, str], v4plus: bool = True) -> "Style":
        g = lambda k, d="": vals.get(k, d)
        align = _int(g("Alignment", 2), 2)
        if not v4plus:  # SSA numpad conversion
            align = {1: 1, 2: 2, 3: 3, 5: 7, 6: 8, 7: 9, 9: 4, 10: 5, 11: 6}.get(align, 2)
        return cls(
            name=g("Name", "Default"), fontname=g("Fontname", "Arial"),
            fontsize=_num(g("Fontsize", 48), 48),
            primary=g("PrimaryColour", "&H00FFFFFF"),
            secondary=g("SecondaryColour", "&H000000FF"),
            outline=g("OutlineColour", g("TertiaryColour", "&H00000000")),
            back=g("BackColour", "&H00000000"),
            bold=cls._b(g("Bold", 0)), italic=cls._b(g("Italic", 0)),
            underline=cls._b(g("Underline", 0)), strikeout=cls._b(g("StrikeOut", 0)),
            scale_x=_num(g("ScaleX", 100), 100), scale_y=_num(g("ScaleY", 100), 100),
            spacing=_num(g("Spacing", 0)), angle=_num(g("Angle", 0)),
            border_style=_int(g("BorderStyle", 1), 1),
            outline_w=_num(g("Outline", 2), 2), shadow=_num(g("Shadow", 2), 2),
            align=align, margin_l=_int(g("MarginL", 10), 10),
            margin_r=_int(g("MarginR", 10), 10), margin_t=_int(g("MarginV", 10), 10),
            encoding=_int(g("Encoding", 1), 1),
        )

    def to_line(self) -> str:
        def n(x):
            x = float(x)
            return str(int(x)) if x == int(x) else ("%g" % x)
        b = lambda v: "-1" if v else "0"
        return "Style: " + ",".join([
            self.name, self.fontname, n(self.fontsize), self.primary, self.secondary,
            self.outline, self.back, b(self.bold), b(self.italic), b(self.underline),
            b(self.strikeout), n(self.scale_x), n(self.scale_y), n(self.spacing),
            n(self.angle), str(self.border_style), n(self.outline_w), n(self.shadow),
            str(self.align), str(self.margin_l), str(self.margin_r), str(self.margin_t),
            str(self.encoding)])

    def copy(self) -> "Style":
        return copy.copy(self)


@dataclass
class Event:
    comment: bool = False
    layer: int = 0
    start: int = 0
    end: int = 0
    style: str = "Default"
    actor: str = ""
    margin_l: int = 0
    margin_r: int = 0
    margin_t: int = 0
    effect: str = ""
    text: str = ""
    extra: dict = field(default_factory=dict)  # Aegisub extradata keys

    @property
    def duration(self) -> int:
        return self.end - self.start

    def to_line(self) -> str:
        return "{}: {},{},{},{},{},{:04d},{:04d},{:04d},{},{}".format(
            "Comment" if self.comment else "Dialogue", self.layer,
            ass_time(self.start), ass_time(self.end), self.style, self.actor,
            self.margin_l, self.margin_r, self.margin_t, self.effect, self.text)

    def copy(self) -> "Event":
        e = copy.copy(self)
        e.extra = dict(self.extra)
        return e


_KV = re.compile(r"^([^:]+):\s?(.*)$")


class Document:
    """An ASS script. Sections not understood are preserved verbatim."""

    def __init__(self):
        self.info: list[tuple[str, str]] = [
            ("Title", "Untitled"), ("ScriptType", "v4.00+"), ("WrapStyle", "0"),
            ("ScaledBorderAndShadow", "yes"), ("YCbCr Matrix", "None"),
            ("PlayResX", "1920"), ("PlayResY", "1080")]
        self.styles: list[Style] = [Style()]
        self.events: list[Event] = []
        # raw extra sections: name -> list[str] (Fonts, Graphics, Aegisub Extradata...)
        self.sections: dict[str, list[str]] = {}
        self.garbage: list[tuple[str, str]] = []  # [Aegisub Project Garbage]
        self.v4plus = True

    # -- info helpers ------------------------------------------------------
    def get_info(self, key: str, default: str = "") -> str:
        for k, v in self.info:
            if k.lower() == key.lower():
                return v
        return default

    def set_info(self, key: str, value: str) -> None:
        for i, (k, _) in enumerate(self.info):
            if k.lower() == key.lower():
                self.info[i] = (k, str(value))
                return
        self.info.append((key, str(value)))

    @property
    def res(self) -> tuple[int, int]:
        x = _int(self.get_info("PlayResX", "0"))
        y = _int(self.get_info("PlayResY", "0"))
        if not x and not y:
            return (384, 288)
        if not x:
            x = y * 4 // 3
        if not y:
            y = x * 3 // 4
        return (x, y)

    def style(self, name: str) -> Optional[Style]:
        for s in self.styles:
            if s.name == name:
                return s
        return None

    # -- parse -------------------------------------------------------------
    @classmethod
    def parse(cls, text: str) -> "Document":
        doc = cls()
        doc.info, doc.styles, doc.events = [], [], []
        if text.startswith("﻿"):
            text = text[1:]
        section = None
        style_fmt = EVENT_FIELDS_DEFAULT = None
        ev_fmt = None
        for raw in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
            line = raw.rstrip()
            if not line.strip():
                continue
            if line.startswith("[") and line.rstrip().endswith("]"):
                section = line.strip()[1:-1]
                continue
            if section is None:
                continue
            sl = section.lower()
            if line.startswith(";") and sl in ("script info", "v4+ styles", "v4 styles", "events"):
                if sl == "script info":
                    doc.info.append(("; comment", line[1:].lstrip()))
                continue
            if sl == "script info":
                m = _KV.match(line)
                if m:
                    k, v = m.group(1).strip(), m.group(2)
                    doc.info.append((k, v))
                    if k.lower() == "scripttype" and v.strip().lower() in ("v4.00", "v4"):
                        doc.v4plus = False
            elif sl in ("v4+ styles", "v4 styles", "v4 styles+"):
                if sl == "v4 styles":
                    doc.v4plus = False
                if line.startswith("Format:"):
                    style_fmt = [x.strip() for x in line[7:].split(",")]
                elif line.startswith("Style:"):
                    vals = [x for x in line[6:].lstrip().split(",")]
                    fmt = style_fmt or STYLE_FIELDS
                    d = {fmt[i]: vals[i] for i in range(min(len(fmt), len(vals)))}
                    doc.styles.append(Style.from_fields(d, doc.v4plus))
            elif sl == "events":
                if line.startswith("Format:"):
                    ev_fmt = [x.strip() for x in line[7:].split(",")]
                else:
                    m = re.match(r"^(Dialogue|Comment):\s?(.*)$", line)
                    if m:
                        fmt = ev_fmt or EVENT_FIELDS
                        if "Marked" in fmt and fmt[0] == "Marked":
                            fmt = ["Layer"] + fmt[1:]
                        vals = m.group(2).split(",", len(fmt) - 1)
                        d = {fmt[i]: vals[i] for i in range(min(len(fmt), len(vals)))}
                        doc.events.append(Event(
                            comment=m.group(1) == "Comment",
                            layer=_int(d.get("Layer", d.get("Marked", 0)).replace("Marked=", "")),
                            start=parse_ass_time(d.get("Start", "0:00:00.00")),
                            end=parse_ass_time(d.get("End", "0:00:00.00")),
                            style=d.get("Style", "Default"),
                            actor=d.get("Name", d.get("Actor", "")),
                            margin_l=_int(d.get("MarginL", 0)), margin_r=_int(d.get("MarginR", 0)),
                            margin_t=_int(d.get("MarginV", d.get("MarginT", 0))),
                            effect=d.get("Effect", ""), text=d.get("Text", "")))
            elif sl == "aegisub project garbage":
                m = _KV.match(line)
                if m:
                    doc.garbage.append((m.group(1).strip(), m.group(2)))
            else:
                doc.sections.setdefault(section, []).append(line)
        if not doc.styles:
            doc.styles.append(Style())
        return doc

    # -- write -------------------------------------------------------------
    def dump_window(self, t0: int, t1: int) -> str:
        """Dump only events overlapping [t0, t1] ms (fast preview script for large documents)."""
        saved = self.events
        try:
            self.events = [e for e in saved if e.end >= t0 and e.start <= t1]
            return self.dump()
        finally:
            self.events = saved

    def dump(self) -> str:
        out = ["[Script Info]"]
        for k, v in self.info:
            out.append(f"; {v}" if k == "; comment" else f"{k}: {v}")
        if self.garbage:
            out += ["", "[Aegisub Project Garbage]"] + [f"{k}: {v}" for k, v in self.garbage]
        out += ["", "[V4+ Styles]", "Format: " + ", ".join(STYLE_FIELDS)]
        out += [s.to_line() for s in self.styles]
        for name, lines in self.sections.items():
            if name in ("Fonts", "Graphics"):
                out += ["", f"[{name}]"] + lines
        out += ["", "[Events]", "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"]
        out += [e.to_line() for e in self.events]
        for name, lines in self.sections.items():
            if name not in ("Fonts", "Graphics"):
                out += ["", f"[{name}]"] + lines
        return "\n".join(out) + "\n"

    def copy(self) -> "Document":
        d = Document()
        d.info = list(self.info)
        d.styles = [s.copy() for s in self.styles]
        d.events = [e.copy() for e in self.events]
        d.sections = {k: list(v) for k, v in self.sections.items()}
        d.garbage = list(self.garbage)
        d.v4plus = self.v4plus
        return d


def load(path: str) -> Document:
    from .formats import read_text
    return Document.parse(read_text(path))


def save(doc: Document, path: str, bom: bool = True) -> None:
    with open(path, "w", encoding="utf-8-sig" if bom else "utf-8", newline="\r\n") as f:
        f.write(doc.dump())
