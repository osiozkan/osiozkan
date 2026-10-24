"""ASS document model: parse / write, timecodes, colors."""
from __future__ import annotations
import re
from dataclasses import dataclass, field, asdict
from typing import List, Optional, Tuple

STYLE_FIELDS = ["name", "fontname", "fontsize", "color1", "color2", "color3", "color4",
                "bold", "italic", "underline", "strikeout", "scale_x", "scale_y", "spacing",
                "angle", "borderstyle", "outline", "shadow", "align", "margin_l", "margin_r",
                "margin_t", "encoding"]
STYLE_FORMAT = ("Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, "
                "Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, "
                "Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding")
EVENT_FORMAT = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
_KNOWN = {"script info", "v4+ styles", "events"}


def parse_time(s: str) -> int:
    """'h:mm:ss.cc' -> milliseconds."""
    s = s.strip()
    m = re.match(r"^(-?\d+):(\d+):(\d+)(?:[.,](\d+))?$", s)
    if not m:
        raise ValueError(f"bad time: {s!r}")
    h, mi, se, frac = m.groups()
    ms = int((frac or "0").ljust(3, "0")[:3])
    return ((int(h) * 60 + int(mi)) * 60 + int(se)) * 1000 + ms


def format_time(ms: int) -> str:
    """ms -> 'h:mm:ss.cc' (rounded to centiseconds like Aegisub)."""
    cs = max(0, int(round(ms / 10.0)))
    h, r = divmod(cs, 360000)
    m, r = divmod(r, 6000)
    s, c = divmod(r, 100)
    return f"{h}:{m:02d}:{s:02d}.{c:02d}"


def color_to_ass(v: str) -> str:
    """Normalize a color string to '&HAABBGGRR&'."""
    v = v.strip()
    if v.startswith("&H") or v.startswith("&h"):
        d = v[2:].rstrip("&")
    elif re.fullmatch(r"-?\d+", v):
        d = "%08X" % (int(v) & 0xFFFFFFFF)
    else:
        d = v.lstrip("#")
    return "&H" + d.upper().rjust(8, "0")[-8:] + "&"


def _color_field(v: str) -> str:
    return color_to_ass(v)[:-1]  # in file: &HAABBGGRR (no trailing &)


def _flag(v: str) -> int:
    v = v.strip()
    return 1 if v in ("-1", "1") else 0


@dataclass
class Style:
    name: str = "Default"
    fontname: str = "Arial"
    fontsize: float = 48
    color1: str = "&H00FFFFFF&"
    color2: str = "&H000000FF&"
    color3: str = "&H00000000&"
    color4: str = "&H00000000&"
    bold: bool = False
    italic: bool = False
    underline: bool = False
    strikeout: bool = False
    scale_x: float = 100
    scale_y: float = 100
    spacing: float = 0
    angle: float = 0
    borderstyle: int = 1
    outline: float = 2
    shadow: float = 2
    align: int = 2
    margin_l: int = 10
    margin_r: int = 10
    margin_t: int = 10
    encoding: int = 1

    @property
    def margin_b(self):
        return self.margin_t

    def to_line(self) -> str:
        b = lambda x: "-1" if x else "0"
        n = lambda x: ("%g" % x)
        f = [self.name, self.fontname, n(self.fontsize), _color_field(self.color1), _color_field(self.color2),
             _color_field(self.color3), _color_field(self.color4), b(self.bold), b(self.italic),
             b(self.underline), b(self.strikeout), n(self.scale_x), n(self.scale_y), n(self.spacing),
             n(self.angle), str(self.borderstyle), n(self.outline), n(self.shadow), str(self.align),
             str(self.margin_l), str(self.margin_r), str(self.margin_t), str(self.encoding)]
        return "Style: " + ",".join(f)

    @staticmethod
    def from_fields(fmt: List[str], vals: List[str]) -> "Style":
        d = {k.strip().lower(): v.strip() for k, v in zip(fmt, vals)}
        g = lambda k, dflt: d.get(k, dflt)
        fl = lambda k, dflt: float(g(k, dflt) or dflt)
        s = Style(
            name=g("name", "Default"), fontname=g("fontname", "Arial"), fontsize=fl("fontsize", "48"),
            color1=color_to_ass(g("primarycolour", "&H00FFFFFF")), color2=color_to_ass(g("secondarycolour", "&H000000FF")),
            color3=color_to_ass(g("outlinecolour", "&H00000000")), color4=color_to_ass(g("backcolour", "&H00000000")),
            bold=bool(_flag(g("bold", "0"))), italic=bool(_flag(g("italic", "0"))),
            underline=bool(_flag(g("underline", "0"))), strikeout=bool(_flag(g("strikeout", "0"))),
            scale_x=fl("scalex", "100"), scale_y=fl("scaley", "100"), spacing=fl("spacing", "0"), angle=fl("angle", "0"),
            borderstyle=int(fl("borderstyle", "1")), outline=fl("outline", "2"), shadow=fl("shadow", "2"),
            align=int(fl("alignment", "2")), margin_l=int(fl("marginl", "10")), margin_r=int(fl("marginr", "10")),
            margin_t=int(fl("marginv", "10")), encoding=int(fl("encoding", "1")))
        return s


@dataclass
class Event:
    comment: bool = False
    layer: int = 0
    start: int = 0   # ms
    end: int = 0     # ms
    style: str = "Default"
    actor: str = ""
    margin_l: int = 0
    margin_r: int = 0
    margin_t: int = 0
    effect: str = ""
    text: str = ""

    @property
    def duration(self) -> int:
        return self.end - self.start

    def to_line(self) -> str:
        kind = "Comment" if self.comment else "Dialogue"
        return (f"{kind}: {self.layer},{format_time(self.start)},{format_time(self.end)},{self.style},"
                f"{self.actor},{self.margin_l:04d},{self.margin_r:04d},{self.margin_t:04d},{self.effect},{self.text}")

    @staticmethod
    def from_line(kind: str, body: str, fmt: List[str]) -> "Event":
        n = len(fmt)
        vals = body.split(",", n - 1)
        if len(vals) < n:
            vals += [""] * (n - len(vals))
        d = {k.strip().lower(): v for k, v in zip(fmt, vals)}
        i = lambda k: int(float(d.get(k, "0").strip() or 0))
        return Event(comment=kind.lower() == "comment", layer=i("layer") if "layer" in d else i("marked"),
                     start=parse_time(d.get("start", "0:00:00.00")), end=parse_time(d.get("end", "0:00:00.00")),
                     style=d.get("style", "Default").strip(), actor=d.get("name", d.get("actor", "")).strip(),
                     margin_l=i("marginl"), margin_r=i("marginr"), margin_t=i("marginv"),
                     effect=d.get("effect", "").strip(), text=d.get("text", ""))


def _default_info():
    return [("ScriptType", "v4.00+"), ("WrapStyle", "0"), ("ScaledBorderAndShadow", "yes"),
            ("YCbCr Matrix", "TV.601"), ("PlayResX", "1920"), ("PlayResY", "1080")]


@dataclass
class Document:
    info: List[Tuple[str, str]] = field(default_factory=_default_info)
    styles: List[Style] = field(default_factory=lambda: [Style()])
    events: List[Event] = field(default_factory=list)
    extra: List[Tuple[str, List[str]]] = field(default_factory=list)  # unknown sections, raw lines
    garbage: List[Tuple[str, str]] = field(default_factory=list)      # [Aegisub Project Garbage] kept in extra

    # -- info helpers
    def get_info(self, key: str, default: Optional[str] = None) -> Optional[str]:
        for k, v in self.info:
            if k.lower() == key.lower():
                return v
        return default

    def set_info(self, key: str, value: str):
        for i, (k, _) in enumerate(self.info):
            if k.lower() == key.lower():
                self.info[i] = (k, str(value))
                return
        self.info.append((key, str(value)))

    @property
    def play_res(self) -> Tuple[int, int]:
        return int(self.get_info("PlayResX", "1920") or 1920), int(self.get_info("PlayResY", "1080") or 1080)

    def style(self, name: str) -> Optional[Style]:
        for s in self.styles:
            if s.name == name:
                return s
        return None

    # -- serialization
    def dumps(self) -> str:
        out = ["[Script Info]", "; Script generated by OsiSub"]
        for k, v in self.info:
            out.append(f"{k}: {v}")
        out += ["", "[V4+ Styles]", "Format: " + STYLE_FORMAT]
        out += [s.to_line() for s in self.styles]
        for name, lines in self.extra:
            out += ["", f"[{name}]"] + lines
        out += ["", "[Events]", "Format: " + EVENT_FORMAT]
        out += [e.to_line() for e in self.events]
        return "\n".join(out) + "\n"

    @staticmethod
    def loads(text: str) -> "Document":
        doc = Document(info=[], styles=[], events=[])
        section = None
        sfmt = efmt = None
        extra_cur: Optional[List[str]] = None
        for raw in text.replace("\r\n", "\n").replace("\r", "\n").lstrip("﻿").split("\n"):
            line = raw.rstrip()
            m = re.match(r"^\[(.+)\]$", line.strip())
            if m:
                section = m.group(1)
                if section.lower() not in _KNOWN:
                    extra_cur = []
                    doc.extra.append((section, extra_cur))
                else:
                    extra_cur = None
                continue
            if not line.strip():
                continue
            sl = (section or "").lower()
            if sl == "script info":
                if line.startswith(";"):
                    continue
                if ":" in line:
                    k, v = line.split(":", 1)
                    doc.info.append((k.strip(), v.strip()))
            elif sl == "v4+ styles":
                if line.lower().startswith("format:"):
                    sfmt = [x.strip() for x in line.split(":", 1)[1].split(",")]
                elif line.lower().startswith("style:"):
                    fmt = sfmt or [x.strip() for x in STYLE_FORMAT.split(",")]
                    doc.styles.append(Style.from_fields(fmt, line.split(":", 1)[1].lstrip().split(",")))
            elif sl == "events":
                if line.lower().startswith("format:"):
                    efmt = [x.strip() for x in line.split(":", 1)[1].split(",")]
                else:
                    m2 = re.match(r"^(Dialogue|Comment):\s?(.*)$", line, re.I | re.S)
                    if m2:
                        doc.events.append(Event.from_line(m2.group(1), m2.group(2),
                                                           efmt or [x.strip() for x in EVENT_FORMAT.split(",")]))
            elif extra_cur is not None:
                extra_cur.append(line)
        if not doc.info:
            doc.info = _default_info()
        if not doc.styles:
            doc.styles = [Style()]
        return doc

    def copy(self) -> "Document":
        return Document.loads(self.dumps())


def decode_bytes(data: bytes) -> str:
    """Decode subtitle file bytes: UTF-8/16 BOM, UTF-8, then Turkish/Western legacy codepages."""
    if data.startswith(b"\xff\xfe") or data.startswith(b"\xfe\xff"):
        return data.decode("utf-16")
    try:
        return data.decode("utf-8-sig")
    except UnicodeDecodeError:
        pass
    for enc in ("cp1254", "cp1252", "latin-1"):
        try:
            return data.decode(enc)
        except UnicodeDecodeError:
            continue
    return data.decode("utf-8", "replace")
