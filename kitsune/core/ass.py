"""ASS/SSA model, parser and writer. Flat ordered entry list, like Aegisub's Lua view."""
from __future__ import annotations
import copy
from dataclasses import dataclass, field, fields
from .timecode import ms_to_ass, ass_to_ms

DEFAULT_STYLE_FORMAT = ("Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,"
                        "Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,"
                        "Shadow,Alignment,MarginL,MarginR,MarginV,Encoding")
DEFAULT_EVENT_FORMAT = "Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text"

@dataclass
class Info:
    key: str
    value: str
    kind: str = "info"

@dataclass
class Style:
    name: str = "Default"
    fontname: str = "Arial"
    fontsize: float = 48
    color1: str = "&H00FFFFFF"
    color2: str = "&H000000FF"
    color3: str = "&H00000000"
    color4: str = "&H00000000"
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
    margin_t: int = 10      # vertical margin (ASS "MarginV")
    encoding: int = 1
    kind: str = "style"

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
    extra: dict = field(default_factory=dict)
    kind: str = "event"

    @property
    def duration(self): return self.end - self.start

@dataclass
class Raw:
    """Unknown/extra section line preserved verbatim (Fonts, Graphics, Extradata...)."""
    section: str
    line: str
    kind: str = "raw"

def _num(v, default=0):
    try:
        f = float(v)
        return int(f) if f == int(f) else f
    except (ValueError, TypeError):
        return default

def _flag(v): return str(v).strip() not in ("0", "", "False")

class Script:
    def __init__(self):
        self.entries: list = []
        self.style_format = DEFAULT_STYLE_FORMAT
        self.event_format = DEFAULT_EVENT_FORMAT
        self.filename: str | None = None
        self._init_defaults()

    def _init_defaults(self):
        for k, v in (("ScriptType", "v4.00+"), ("PlayResX", "1920"), ("PlayResY", "1080"),
                     ("WrapStyle", "0"), ("ScaledBorderAndShadow", "yes"), ("YCbCr Matrix", "TV.709")):
            self.entries.append(Info(k, v))
        self.entries.append(Style())

    # --- accessors
    @property
    def infos(self): return [e for e in self.entries if e.kind == "info"]
    @property
    def styles(self): return [e for e in self.entries if e.kind == "style"]
    @property
    def events(self): return [e for e in self.entries if e.kind == "event"]

    def get_info(self, key, default=None):
        for e in self.entries:
            if e.kind == "info" and e.key.lower() == key.lower():
                return e.value
        return default

    def set_info(self, key, value):
        for e in self.entries:
            if e.kind == "info" and e.key.lower() == key.lower():
                e.value = str(value); return
        idx = max([i for i, e in enumerate(self.entries) if e.kind == "info"], default=-1) + 1
        self.entries.insert(idx, Info(key, str(value)))

    def get_style(self, name):
        for s in self.styles:
            if s.name == name: return s
        return None

    @property
    def res(self):
        return int(_num(self.get_info("PlayResX", 640))), int(_num(self.get_info("PlayResY", 480)))

    # --- parsing
    @classmethod
    def parse(cls, text: str) -> "Script":
        sc = cls(); sc.entries = []
        text = text.lstrip("﻿")
        section = None
        sfmt = efmt = None
        for raw in text.splitlines():
            line = raw.rstrip("\r\n")
            st = line.strip()
            if not st or st.startswith(";") and section in ("script info", "v4+ styles", "v4 styles", "events"):
                if st.startswith(";") and section == "script info":
                    sc.entries.append(Info(";", st[1:].strip(), "info"))
                continue
            if st.startswith("[") and st.endswith("]"):
                section = st[1:-1].strip().lower(); continue
            if section == "script info":
                if ":" in st:
                    k, v = st.split(":", 1); sc.entries.append(Info(k.strip(), v.strip()))
            elif section in ("v4+ styles", "v4 styles", "v4+ styles+"):
                if st.lower().startswith("format:"):
                    sfmt = [x.strip().lower() for x in st[7:].split(",")]
                    sc.style_format = st[7:].strip()
                elif st.lower().startswith("style:"):
                    sc.entries.append(_parse_style(st[6:].strip(), sfmt))
            elif section == "events":
                low = st.lower()
                if low.startswith("format:"):
                    efmt = [x.strip().lower() for x in st[7:].split(",")]
                    sc.event_format = st[7:].strip()
                else:
                    for pfx, cm in (("dialogue:", False), ("comment:", True)):
                        if low.startswith(pfx):
                            sc.entries.append(_parse_event(st[len(pfx):].lstrip(), efmt, cm)); break
            elif section is not None:
                sc.entries.append(Raw(section, line))
        if not sc.styles:
            sc.entries.append(Style())
        return sc

    @classmethod
    def load(cls, path, encoding=None):
        data = open(path, "rb").read()
        text = decode_bytes(data, encoding)
        sc = cls.parse(text); sc.filename = str(path); return sc

    # --- writing
    def dumps(self) -> str:
        out = ["[Script Info]"]
        for e in self.infos:
            out.append(f"; {e.value}" if e.key == ";" else f"{e.key}: {e.value}")
        out += ["", "[V4+ Styles]", f"Format: {DEFAULT_STYLE_FORMAT}"]
        out += [_style_line(s) for s in self.styles]
        out += ["", "[Events]", f"Format: {DEFAULT_EVENT_FORMAT}"]
        out += [_event_line(e) for e in self.events]
        raws: dict = {}
        for e in self.entries:
            if e.kind == "raw": raws.setdefault(e.section, []).append(e.line)
        for sec, lines in raws.items():
            out += ["", f"[{sec.title() if sec in ('fonts','graphics') else sec}]"] + lines
        return "\n".join(out) + "\n"

    def save(self, path=None, encoding="utf-8-sig"):
        path = path or self.filename
        with open(path, "w", encoding=encoding, newline="\r\n") as f:
            f.write(self.dumps())
        self.filename = str(path)

    def copy(self): return copy.deepcopy(self)

def decode_bytes(data: bytes, encoding=None) -> str:
    if encoding: return data.decode(encoding)
    if data.startswith(b"\xff\xfe") or data.startswith(b"\xfe\xff"): return data.decode("utf-16")
    for enc in ("utf-8-sig", "cp1254", "cp1252", "latin-1"):
        try: return data.decode(enc)
        except UnicodeDecodeError: continue
    return data.decode("latin-1", "replace")

def _parse_style(body, fmt):
    fmt = fmt or [x.lower() for x in DEFAULT_STYLE_FORMAT.split(",")]
    vals = [v.strip() for v in body.split(",", len(fmt) - 1)]
    d = dict(zip(fmt, vals)); g = d.get
    s = Style(name=g("name", "Default"), fontname=g("fontname", "Arial"), fontsize=_num(g("fontsize"), 48),
              color1=g("primarycolour", "&H00FFFFFF"), color2=g("secondarycolour", "&H000000FF"),
              color3=g("outlinecolour", "&H00000000"), color4=g("backcolour", "&H00000000"),
              bold=_flag(g("bold", 0)), italic=_flag(g("italic", 0)), underline=_flag(g("underline", 0)),
              strikeout=_flag(g("strikeout", 0)), scale_x=_num(g("scalex"), 100), scale_y=_num(g("scaley"), 100),
              spacing=_num(g("spacing")), angle=_num(g("angle")), borderstyle=int(_num(g("borderstyle"), 1)),
              outline=_num(g("outline"), 2), shadow=_num(g("shadow"), 2), align=int(_num(g("alignment"), 2)),
              margin_l=int(_num(g("marginl"), 10)), margin_r=int(_num(g("marginr"), 10)),
              margin_t=int(_num(g("marginv"), 10)), encoding=int(_num(g("encoding"), 1)))
    return s

def _b(v): return "-1" if v else "0"
def _n(v): return str(int(v)) if float(v) == int(v) else f"{v:g}"

def _style_line(s: Style):
    return ("Style: " + ",".join([s.name, s.fontname, _n(s.fontsize), s.color1, s.color2, s.color3, s.color4,
            _b(s.bold), _b(s.italic), _b(s.underline), _b(s.strikeout), _n(s.scale_x), _n(s.scale_y),
            _n(s.spacing), _n(s.angle), str(s.borderstyle), _n(s.outline), _n(s.shadow), str(s.align),
            str(s.margin_l), str(s.margin_r), str(s.margin_t), str(s.encoding)]))

def _parse_event(body, fmt, comment):
    fmt = fmt or [x.lower() for x in DEFAULT_EVENT_FORMAT.split(",")]
    vals = body.split(",", len(fmt) - 1)
    d = dict(zip(fmt, vals)); g = d.get
    return Event(comment=comment, layer=int(_num(g("layer"), 0)), start=ass_to_ms(g("start", "0:00:00.00")),
                 end=ass_to_ms(g("end", "0:00:00.00")), style=g("style", "Default").strip(),
                 actor=(g("name") or g("actor") or "").strip(), margin_l=int(_num(g("marginl"))),
                 margin_r=int(_num(g("marginr"))), margin_t=int(_num(g("marginv"))),
                 effect=g("effect", "").strip(), text=g("text", ""))

def _event_line(e: Event):
    return (("Comment" if e.comment else "Dialogue") + f": {e.layer},{ms_to_ass(e.start)},{ms_to_ass(e.end)},"
            f"{e.style},{e.actor},{e.margin_l},{e.margin_r},{e.margin_t},{e.effect},{e.text}")
