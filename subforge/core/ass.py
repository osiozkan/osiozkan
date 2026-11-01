"""ASS / SSA document model, parser and writer.

The model is a *flat list* of lines (like Aegisub) so that the Lua Auto4 API
(`subs[i]`) maps 1:1.  Each line knows its section; the writer regroups.
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field, fields
from typing import Iterable, Optional

from .timecode import ass_to_ms, ms_to_ass

S_INFO = "Script Info"
S_STYLES = "V4+ Styles"
S_STYLES_SSA = "V4 Styles"
S_EVENTS = "Events"
S_FONTS = "Fonts"
S_GRAPHICS = "Graphics"
S_GARBAGE = "Aegisub Project Garbage"
S_EXTRA = "Aegisub Extradata"

STYLE_FORMAT = ("Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, "
                "Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, "
                "Shadow, Alignment, MarginL, MarginR, MarginV, Encoding")
EVENT_FORMAT = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"

_COLOR_RE = re.compile(r"^\s*&?[Hh]?([0-9A-Fa-f]{1,8})&?\s*$")


def norm_color(v) -> str:
    """Normalise any ASS colour spelling to '&HAABBGGRR'."""
    if isinstance(v, int):
        n = v & 0xFFFFFFFF
        return "&H%08X" % n
    s = str(v).strip()
    if re.fullmatch(r"-?\d+", s):
        return "&H%08X" % (int(s) & 0xFFFFFFFF)
    m = _COLOR_RE.match(s)
    if not m:
        return "&H00FFFFFF"
    h = m.group(1)
    if len(h) <= 6:
        return "&H00" + h.rjust(6, "0").upper()
    return "&H" + h.rjust(8, "0").upper()


def _num(v: float) -> str:
    if isinstance(v, bool):
        return "-1" if v else "0"
    if float(v) == int(v):
        return str(int(v))
    return ("%.4f" % v).rstrip("0").rstrip(".")


def _f(s: str, default=0.0) -> float:
    try:
        return float(s)
    except (TypeError, ValueError):
        return default


def _i(s: str, default=0) -> int:
    try:
        return int(float(s))
    except (TypeError, ValueError):
        return default


@dataclass
class Info:
    key: str
    value: str = ""
    section: str = S_INFO
    cls = "info"

    def to_ass(self) -> str:
        return f"{self.key}: {self.value}"


@dataclass
class Other:
    """Any line we don't interpret (comments, [Fonts] data, extradata...)."""
    raw: str
    section: str = S_INFO
    cls = "unknown"

    def to_ass(self) -> str:
        return self.raw


@dataclass
class Style:
    name: str = "Default"
    fontname: str = "Arial"
    fontsize: float = 48.0
    color1: str = "&H00FFFFFF"
    color2: str = "&H000000FF"
    color3: str = "&H00000000"
    color4: str = "&H00000000"
    bold: bool = False
    italic: bool = False
    underline: bool = False
    strikeout: bool = False
    scale_x: float = 100.0
    scale_y: float = 100.0
    spacing: float = 0.0
    angle: float = 0.0
    borderstyle: int = 1
    outline: float = 2.0
    shadow: float = 2.0
    align: int = 2
    margin_l: int = 10
    margin_r: int = 10
    margin_t: int = 10
    margin_b: int = 10
    encoding: int = 1
    relative_to: int = 2
    section: str = S_STYLES
    cls = "style"

    @property
    def margin_v(self) -> int:
        return self.margin_t

    @margin_v.setter
    def margin_v(self, v: int) -> None:
        self.margin_t = self.margin_b = int(v)

    def copy(self) -> "Style":
        return Style(**{f.name: getattr(self, f.name) for f in fields(self)})

    def to_ass(self) -> str:
        b = lambda x: "-1" if x else "0"
        return "Style: " + ",".join([
            self.name, self.fontname, _num(self.fontsize),
            norm_color(self.color1), norm_color(self.color2), norm_color(self.color3), norm_color(self.color4),
            b(self.bold), b(self.italic), b(self.underline), b(self.strikeout),
            _num(self.scale_x), _num(self.scale_y), _num(self.spacing), _num(self.angle),
            str(self.borderstyle), _num(self.outline), _num(self.shadow), str(self.align),
            str(self.margin_l), str(self.margin_r), str(self.margin_t), str(self.encoding)])


@dataclass
class Event:
    comment: bool = False
    layer: int = 0
    start_time: int = 0
    end_time: int = 0
    style: str = "Default"
    actor: str = ""
    margin_l: int = 0
    margin_r: int = 0
    margin_t: int = 0
    margin_b: int = 0
    effect: str = ""
    text: str = ""
    section: str = S_EVENTS
    cls = "dialogue"

    @property
    def margin_v(self) -> int:
        return self.margin_t

    @margin_v.setter
    def margin_v(self, v: int) -> None:
        self.margin_t = self.margin_b = int(v)

    @property
    def duration(self) -> int:
        return self.end_time - self.start_time

    def copy(self) -> "Event":
        return Event(**{f.name: getattr(self, f.name) for f in fields(self)})

    def to_ass(self) -> str:
        return "%s: %d,%s,%s,%s,%s,%d,%d,%d,%s,%s" % (
            "Comment" if self.comment else "Dialogue", self.layer,
            ms_to_ass(self.start_time), ms_to_ass(self.end_time), self.style, self.actor,
            self.margin_l, self.margin_r, self.margin_t, self.effect, self.text)


Line = Info | Other | Style | Event


@dataclass
class ASSFile:
    lines: list = field(default_factory=list)
    warnings: list = field(default_factory=list)
    was_ssa: bool = False

    # ---- construction --------------------------------------------------
    @classmethod
    def new(cls) -> "ASSFile":
        f = cls()
        for k, v in (("Title", "Untitled"), ("ScriptType", "v4.00+"), ("WrapStyle", "0"),
                     ("ScaledBorderAndShadow", "yes"), ("YCbCr Matrix", "TV.709"),
                     ("PlayResX", "1920"), ("PlayResY", "1080")):
            f.lines.append(Info(k, v))
        f.lines.append(Style(fontsize=72, outline=3, shadow=1, margin_l=60, margin_r=60, margin_t=40, margin_b=40))
        return f

    # ---- views ---------------------------------------------------------
    @property
    def events(self) -> list[Event]:
        return [l for l in self.lines if isinstance(l, Event)]

    @property
    def styles(self) -> list[Style]:
        return [l for l in self.lines if isinstance(l, Style)]

    def style_map(self) -> dict[str, Style]:
        return {s.name: s for s in self.styles}

    def get_info(self, key: str, default: str = "") -> str:
        kl = key.lower()
        for l in self.lines:
            if isinstance(l, Info) and l.section == S_INFO and l.key.lower() == kl:
                return l.value
        return default

    def set_info(self, key: str, value) -> None:
        kl = key.lower()
        for l in self.lines:
            if isinstance(l, Info) and l.section == S_INFO and l.key.lower() == kl:
                l.value = str(value)
                return
        # insert after last Script Info line
        idx = 0
        for i, l in enumerate(self.lines):
            if l.section == S_INFO:
                idx = i + 1
        self.lines.insert(idx, Info(key, str(value)))

    @property
    def play_res(self) -> tuple[int, int]:
        x = _i(self.get_info("PlayResX"), 0)
        y = _i(self.get_info("PlayResY"), 0)
        if x <= 0 and y <= 0:
            return 384, 288
        if x <= 0:
            x = y * 4 // 3
        if y <= 0:
            y = x * 3 // 4
        return x, y

    def ensure_default_style(self) -> None:
        if not self.styles:
            idx = len([1 for l in self.lines if l.section == S_INFO])
            self.lines.insert(idx, Style())

    def sort_events(self, key: str = "start") -> None:
        ev = [l for l in self.lines if isinstance(l, Event)]
        keyf = {"start": lambda e: (e.start_time, e.end_time), "end": lambda e: (e.end_time, e.start_time),
                "style": lambda e: e.style, "actor": lambda e: e.actor, "layer": lambda e: e.layer}[key]
        ev.sort(key=keyf)
        it = iter(ev)
        self.lines = [next(it) if isinstance(l, Event) else l for l in self.lines]

    def to_string(self) -> str:
        return write_ass(self)

    def save(self, path: str, encoding: str = "utf-8-sig") -> None:
        with open(path, "w", encoding=encoding, newline="") as fh:
            fh.write(write_ass(self))


# ---------------------------------------------------------------------------
# parsing
# ---------------------------------------------------------------------------

def _ssa_align(a: int) -> int:
    if a >= 9:
        return a - 9 + 4  # middle row: 9,10,11 -> 4,5,6 (SSA 9=left)
    if a >= 5:
        return a - 5 + 7  # top row
    return max(1, min(3, a))


def _parse_style(vals: dict, ssa: bool) -> Style:
    g = lambda k, d="": vals.get(k, d)
    s = Style(name=g("name", "Default").strip())
    s.fontname = g("fontname", "Arial").strip()
    s.fontsize = _f(g("fontsize"), 48)
    s.color1 = norm_color(g("primarycolour", "&H00FFFFFF"))
    s.color2 = norm_color(g("secondarycolour", "&H000000FF"))
    s.color3 = norm_color(g("outlinecolour", g("tertiarycolour", "&H00000000")))
    s.color4 = norm_color(g("backcolour", "&H00000000"))
    s.bold = _i(g("bold"), 0) != 0
    s.italic = _i(g("italic"), 0) != 0
    s.underline = _i(g("underline"), 0) != 0
    s.strikeout = _i(g("strikeout"), 0) != 0
    s.scale_x = _f(g("scalex"), 100)
    s.scale_y = _f(g("scaley"), 100)
    s.spacing = _f(g("spacing"), 0)
    s.angle = _f(g("angle"), 0)
    s.borderstyle = _i(g("borderstyle"), 1)
    s.outline = _f(g("outline"), 2)
    s.shadow = _f(g("shadow"), 2)
    al = _i(g("alignment"), 2)
    s.align = _ssa_align(al) if ssa else (al if 1 <= al <= 9 else 2)
    s.margin_l = _i(g("marginl"), 10)
    s.margin_r = _i(g("marginr"), 10)
    s.margin_t = s.margin_b = _i(g("marginv"), 10)
    s.encoding = _i(g("encoding"), 1)
    return s


def _split_format(s: str) -> list[str]:
    return [p.strip().lower().replace(" ", "") for p in s.split(",")]


def parse_ass(text: str) -> ASSFile:
    f = ASSFile()
    if text.startswith("﻿"):
        text = text[1:]
    section = None
    style_fmt: list[str] | None = None
    event_fmt: list[str] | None = None
    ssa = False
    for ln, raw in enumerate(text.splitlines(), 1):
        line = raw.rstrip("\r\n")
        if not line.strip():
            continue
        st = line.strip()
        if st.startswith("[") and st.endswith("]"):
            section = st[1:-1].strip()
            sl = section.lower()
            if sl == "v4 styles":
                ssa = True
                f.was_ssa = True
                section = S_STYLES
            elif sl == "v4+ styles":
                section = S_STYLES
            elif sl == "script info":
                section = S_INFO
            elif sl == "events":
                section = S_EVENTS
            elif sl == "fonts":
                section = S_FONTS
            elif sl == "graphics":
                section = S_GRAPHICS
            continue
        if section is None:
            section = S_INFO
        try:
            if section == S_INFO or section == S_GARBAGE:
                if st.startswith(";") or ":" not in st:
                    f.lines.append(Other(line, section))
                else:
                    k, v = line.split(":", 1)
                    f.lines.append(Info(k.strip(), v.strip(), section))
            elif section == S_STYLES:
                if st.lower().startswith("format:"):
                    style_fmt = _split_format(line.split(":", 1)[1])
                elif st.lower().startswith("style:"):
                    fmt = style_fmt or _split_format(STYLE_FORMAT if not ssa else
                        "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, "
                        "Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding")
                    parts = [p for p in line.split(":", 1)[1].lstrip().split(",")]
                    parts += [""] * (len(fmt) - len(parts))
                    vals = {k: v.strip() if k != "name" else v for k, v in zip(fmt, parts)}
                    f.lines.append(_parse_style(vals, ssa))
                else:
                    f.lines.append(Other(line, section))
            elif section == S_EVENTS:
                low = st.lower()
                if low.startswith("format:"):
                    event_fmt = _split_format(line.split(":", 1)[1])
                elif low.startswith(("dialogue:", "comment:")):
                    fmt = event_fmt or _split_format(EVENT_FORMAT)
                    kind, body = line.split(":", 1)
                    body = body[1:] if body.startswith(" ") else body
                    parts = body.split(",", len(fmt) - 1)
                    parts += [""] * (len(fmt) - len(parts))
                    v = dict(zip(fmt, parts))
                    e = Event(comment=kind.strip().lower() == "comment")
                    e.layer = _i(v.get("layer", v.get("marked", "0")).replace("Marked=", ""), 0)
                    e.start_time = ass_to_ms(v.get("start", "0:00:00.00"))
                    e.end_time = ass_to_ms(v.get("end", "0:00:00.00"))
                    e.style = v.get("style", "Default").strip()
                    e.actor = v.get("name", v.get("actor", "")).strip()
                    e.margin_l = _i(v.get("marginl"), 0)
                    e.margin_r = _i(v.get("marginr"), 0)
                    mv = v.get("marginv")
                    e.margin_t = _i(v.get("margint", mv), 0)
                    e.margin_b = _i(v.get("marginb", mv), 0)
                    e.effect = v.get("effect", "")
                    e.text = v.get("text", "")
                    f.lines.append(e)
                else:
                    f.lines.append(Other(line, section))
            else:
                f.lines.append(Other(line, section))
        except Exception as exc:  # keep going; never lose a file over one bad line
            f.warnings.append(f"line {ln}: {exc}")
            f.lines.append(Other(line, section or S_INFO))
    if f.was_ssa:
        f.set_info("ScriptType", "v4.00+")
    f.ensure_default_style()
    return f


def write_ass(f: ASSFile, with_garbage: bool = True) -> str:
    order: list[str] = []
    groups: dict[str, list] = {}
    for l in f.lines:
        if l.section not in groups:
            groups[l.section] = []
            order.append(l.section)
        groups[l.section].append(l)
    for sec in (S_EVENTS, S_STYLES, S_INFO):  # canonical order
        if sec in order:
            order.remove(sec)
    canon = [S_INFO, S_STYLES, S_EVENTS]
    rest = [s for s in order if s not in canon]
    out: list[str] = []
    for sec in canon + rest:
        if sec not in groups:
            if sec == S_EVENTS:
                groups[sec] = []
            else:
                continue
        if sec == S_GARBAGE and not with_garbage:
            continue
        out.append(f"[{sec}]")
        if sec == S_INFO and not any(isinstance(l, Info) and l.key.lower() == "scripttype" for l in groups[sec]):
            out.append("ScriptType: v4.00+")
        if sec == S_STYLES:
            out.append("Format: " + STYLE_FORMAT)
        if sec == S_EVENTS:
            out.append("Format: " + EVENT_FORMAT)
        out.extend(l.to_ass() for l in groups[sec])
        out.append("")
    return "\r\n".join(out)
