"""ASS/SSA document model, parser and writer (lossless for unknown sections)."""
from __future__ import annotations
import re
from dataclasses import dataclass, field, replace
from typing import Optional
from .timeutil import parse_ass_time, format_ass_time

STYLE_FIELDS = [
    "Name", "Fontname", "Fontsize", "PrimaryColour", "SecondaryColour", "OutlineColour",
    "BackColour", "Bold", "Italic", "Underline", "StrikeOut", "ScaleX", "ScaleY", "Spacing",
    "Angle", "BorderStyle", "Outline", "Shadow", "Alignment", "MarginL", "MarginR", "MarginV",
    "Encoding",
]
DEFAULT_STYLE_FORMAT = ", ".join(STYLE_FIELDS)
EVENT_FORMAT = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"


@dataclass
class Style:
    name: str = "Default"
    fontname: str = "Arial"
    fontsize: float = 48
    primary: str = "&H00FFFFFF"
    secondary: str = "&H000000FF"
    outline_c: str = "&H00000000"
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
    outline: float = 2
    shadow: float = 2
    align: int = 2
    margin_l: int = 10
    margin_r: int = 10
    margin_v: int = 10
    encoding: int = 1

    @staticmethod
    def _num(v: str) -> float:
        try:
            f = float(v)
            return int(f) if f == int(f) else f
        except ValueError:
            return 0

    @classmethod
    def from_fields(cls, fmt: list[str], vals: list[str]) -> "Style":
        d = {k.strip().lower(): v.strip() for k, v in zip(fmt, vals)}
        n = cls._num
        def b(k):
            return n(d.get(k, "0")) != 0
        s = cls()
        s.name = d.get("name", "Default")
        s.fontname = d.get("fontname", "Arial")
        s.fontsize = n(d.get("fontsize", "48"))
        s.primary = d.get("primarycolour", s.primary)
        s.secondary = d.get("secondarycolour", s.secondary)
        s.outline_c = d.get("outlinecolour", d.get("tertiarycolour", s.outline_c))
        s.back = d.get("backcolour", s.back)
        s.bold, s.italic, s.underline, s.strikeout = b("bold"), b("italic"), b("underline"), b("strikeout")
        s.scale_x, s.scale_y = n(d.get("scalex", "100")), n(d.get("scaley", "100"))
        s.spacing, s.angle = n(d.get("spacing", "0")), n(d.get("angle", "0"))
        s.border_style = int(n(d.get("borderstyle", "1")))
        s.outline, s.shadow = n(d.get("outline", "2")), n(d.get("shadow", "2"))
        s.align = int(n(d.get("alignment", "2")))
        s.margin_l, s.margin_r = int(n(d.get("marginl", "10"))), int(n(d.get("marginr", "10")))
        s.margin_v = int(n(d.get("marginv", "10")))
        s.encoding = int(n(d.get("encoding", "1")))
        return s

    def to_line(self) -> str:
        def fb(v):
            return "-1" if v else "0"
        def fn(v):
            return str(int(v)) if float(v) == int(v) else repr(float(v))
        parts = [self.name, self.fontname, fn(self.fontsize), self.primary, self.secondary,
                 self.outline_c, self.back, fb(self.bold), fb(self.italic), fb(self.underline),
                 fb(self.strikeout), fn(self.scale_x), fn(self.scale_y), fn(self.spacing),
                 fn(self.angle), str(self.border_style), fn(self.outline), fn(self.shadow),
                 str(self.align), str(self.margin_l), str(self.margin_r), str(self.margin_v),
                 str(self.encoding)]
        return "Style: " + ",".join(parts)


@dataclass
class Event:
    comment: bool = False
    layer: int = 0
    start: int = 0  # ms
    end: int = 0
    style: str = "Default"
    actor: str = ""
    margin_l: int = 0
    margin_r: int = 0
    margin_v: int = 0
    effect: str = ""
    text: str = ""

    @property
    def duration(self) -> int:
        return self.end - self.start

    def copy(self) -> "Event":
        return replace(self)

    @classmethod
    def from_line(cls, kind: str, rest: str, fmt: list[str]) -> "Event":
        parts = rest.split(",", len(fmt) - 1)
        parts += [""] * (len(fmt) - len(parts))
        d = {k.strip().lower(): v for k, v in zip(fmt, parts)}
        def i(k):
            try:
                return int(float(d.get(k, "0").strip() or 0))
            except ValueError:
                return 0
        e = cls()
        e.comment = kind.lower() == "comment"
        e.layer = i("layer") if "layer" in d else i("marked")
        e.start = parse_ass_time(d.get("start", "0:00:00.00"))
        e.end = parse_ass_time(d.get("end", "0:00:00.00"))
        e.style = d.get("style", "Default").strip()
        e.actor = d.get("name", d.get("actor", "")).strip()
        e.margin_l, e.margin_r = i("marginl"), i("marginr")
        e.margin_v = i("marginv")
        e.effect = d.get("effect", "").strip()
        e.text = d.get("text", "")
        return e

    def to_line(self) -> str:
        def m(v):
            return f"{v:04d}" if v >= 0 else str(v)
        return (f"{'Comment' if self.comment else 'Dialogue'}: {self.layer},"
                f"{format_ass_time(self.start)},{format_ass_time(self.end)},{self.style},"
                f"{self.actor},{m(self.margin_l)},{m(self.margin_r)},{m(self.margin_v)},"
                f"{self.effect},{self.text}")


@dataclass
class Document:
    info: list[tuple[str, str]] = field(default_factory=list)  # ordered; key '' = comment line
    styles: list[Style] = field(default_factory=list)
    events: list[Event] = field(default_factory=list)
    garbage: list[tuple[str, str]] = field(default_factory=list)  # [Aegisub Project Garbage]
    attachments: list[tuple[str, str, list[str]]] = field(default_factory=list)  # (section, name, lines)
    extra_sections: list[tuple[str, list[str]]] = field(default_factory=list)
    style_format: str = DEFAULT_STYLE_FORMAT
    filename: Optional[str] = None

    # ---- script info helpers
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
        def g(k, d):
            try:
                return int(float(self.get_info(k, str(d))))
            except ValueError:
                return d
        return g("PlayResX", 384 if not self.get_info("PlayResY") else 0) or 384, g("PlayResY", 288)

    def style(self, name: str) -> Optional[Style]:
        for s in self.styles:
            if s.name == name:
                return s
        return None

    @classmethod
    def new(cls) -> "Document":
        d = cls()
        d.info = [("Title", "Untitled"), ("ScriptType", "v4.00+"), ("WrapStyle", "0"),
                  ("ScaledBorderAndShadow", "yes"), ("YCbCr Matrix", "TV.709"),
                  ("PlayResX", "1920"), ("PlayResY", "1080")]
        d.styles = [Style(fontsize=60, outline=3, shadow=1, margin_l=60, margin_r=60, margin_v=40)]
        return d

    # ---- parsing
    @classmethod
    def parse(cls, text: str) -> "Document":
        doc = cls()
        text = text.lstrip("﻿")
        section = None
        sfmt: list[str] = STYLE_FIELDS[:]
        efmt: list[str] = [x.strip() for x in EVENT_FORMAT.split(",")]
        cur_att = None
        extra: Optional[list[str]] = None
        for raw in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
            line = raw
            st = line.strip()
            if st.startswith("[") and st.endswith("]") and len(st) > 2 and (
                    cur_att is None or re.fullmatch(r"\[[A-Za-z0-9+ ]{2,40}\]", st)
                    and st[1:-1].lower() in ("script info", "v4+ styles", "v4 styles", "events", "fonts",
                                             "graphics", "aegisub project garbage", "aegisub extradata")):
                section = st[1:-1]
                cur_att = None
                low = section.lower()
                if low not in ("script info", "v4+ styles", "v4 styles", "events",
                               "aegisub project garbage", "fonts", "graphics"):
                    extra = []
                    doc.extra_sections.append((section, extra))
                else:
                    extra = None
                continue
            if section is None:
                continue
            low = section.lower()
            if extra is not None:
                extra.append(line)
                continue
            if low in ("fonts", "graphics"):
                if st.startswith("fontname:") or st.startswith("filename:"):
                    cur_att = (section, st.split(":", 1)[1].strip(), [])
                    doc.attachments.append(cur_att)
                elif cur_att is not None and st:
                    cur_att[2].append(st)
                continue
            if not st:
                continue
            if low == "script info":
                if st.startswith(";"):
                    doc.info.append(("", st))
                elif ":" in st:
                    k, v = st.split(":", 1)
                    doc.info.append((k.strip(), v.strip()))
            elif low == "aegisub project garbage":
                if ":" in st:
                    k, v = st.split(":", 1)
                    doc.garbage.append((k.strip(), v.strip()))
            elif low in ("v4+ styles", "v4 styles"):
                if st.lower().startswith("format:"):
                    sfmt = [x.strip() for x in st.split(":", 1)[1].split(",")]
                    doc.style_format = ", ".join(sfmt)
                elif st.lower().startswith("style:"):
                    vals = st.split(":", 1)[1].lstrip().split(",")
                    doc.styles.append(Style.from_fields(sfmt, vals))
            elif low == "events":
                if st.lower().startswith("format:"):
                    efmt = [x.strip() for x in st.split(":", 1)[1].split(",")]
                else:
                    m = re.match(r"^(Dialogue|Comment|Picture|Sound|Movie|Command):\s?(.*)$", line.lstrip(), re.S)
                    if m and m.group(1) in ("Dialogue", "Comment"):
                        doc.events.append(Event.from_line(m.group(1), m.group(2), efmt))
        if not any(k.lower() == "scripttype" for k, _ in doc.info):
            doc.info.insert(0, ("ScriptType", "v4.00+"))
        return doc

    @classmethod
    def load(cls, path: str) -> "Document":
        data = open(path, "rb").read()
        doc = cls.parse(decode_bytes(data))
        doc.filename = path
        return doc

    # ---- writing
    def dumps(self) -> str:
        out = ["[Script Info]"]
        for k, v in self.info:
            out.append(v if k == "" else f"{k}: {v}")
        out += ["", "[V4+ Styles]", "Format: " + DEFAULT_STYLE_FORMAT]
        out += [s.to_line() for s in self.styles]
        out += ["", "[Events]", "Format: " + EVENT_FORMAT]
        out += [e.to_line() for e in self.events]
        if self.garbage:
            out += ["", "[Aegisub Project Garbage]"] + [f"{k}: {v}" for k, v in self.garbage]
        for sec in ("Fonts", "Graphics"):
            items = [a for a in self.attachments if a[0].lower() == sec.lower()]
            if items:
                out += ["", f"[{sec}]"]
                for _, name, lines in items:
                    out.append(("fontname: " if sec == "Fonts" else "filename: ") + name)
                    out += lines
                    out.append("")
        for name, lines in self.extra_sections:
            out += ["", f"[{name}]"] + [l for l in lines if l.strip() != "" or True]
        return "\n".join(out).rstrip("\n") + "\n"

    def save(self, path: Optional[str] = None, bom: bool = True) -> None:
        path = path or self.filename
        if not path:
            raise ValueError("no path")
        with open(path, "wb") as f:
            f.write(("﻿" if bom else "").encode("utf-8") + self.dumps().replace("\n", "\r\n").encode("utf-8"))
        self.filename = path


def decode_bytes(data: bytes) -> str:
    """Decode subtitle bytes: BOMs first, then UTF-8, then common legacy codepages."""
    if data.startswith(b"\xef\xbb\xbf"):
        return data[3:].decode("utf-8", "replace")
    if data.startswith(b"\xff\xfe") or data.startswith(b"\xfe\xff"):
        return data.decode("utf-16", "replace")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        pass
    for enc in ("cp1254", "cp1252", "latin-1"):  # Turkish first
        try:
            return data.decode(enc)
        except UnicodeDecodeError:
            continue
    return data.decode("latin-1", "replace")
