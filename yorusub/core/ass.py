"""ASS/SSA ayrıştırma ve yazma. Bilinmeyen bölümler (Fonts, Graphics, Aegisub
Project Garbage, Extradata) kayıpsız korunur."""
from __future__ import annotations
import re
from dataclasses import dataclass, field, replace
from typing import Optional

EVENT_FORMAT = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
STYLE_FORMAT = ["Name", "Fontname", "Fontsize", "PrimaryColour", "SecondaryColour", "OutlineColour",
                "BackColour", "Bold", "Italic", "Underline", "StrikeOut", "ScaleX", "ScaleY", "Spacing",
                "Angle", "BorderStyle", "Outline", "Shadow", "Alignment", "MarginL", "MarginR", "MarginV",
                "Encoding"]


def ms_to_ass(ms: int) -> str:
    ms = max(0, int(round(ms)))
    cs = int(round(ms / 10.0))  # ASS centisaniye hassasiyetindedir
    h, cs = divmod(cs, 360000)
    m, cs = divmod(cs, 6000)
    s, cs = divmod(cs, 100)
    return f"{h}:{m:02d}:{s:02d}.{cs:02d}"


def ass_to_ms(t: str) -> int:
    t = t.strip()
    m = re.fullmatch(r"(\d+):(\d+):(\d+)[.,:](\d+)", t)
    if not m:
        raise ValueError(f"Geçersiz ASS zamanı: {t!r}")
    h, mi, s, frac = m.groups()
    frac_ms = int((frac + "000")[:3]) if len(frac) != 2 else int(frac) * 10
    return ((int(h) * 60 + int(mi)) * 60 + int(s)) * 1000 + frac_ms


def _trim(lines):
    while lines and not lines[-1].strip():
        lines = lines[:-1]
    return lines


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
    borderstyle: int = 1
    outline: float = 2
    shadow: float = 0
    align: int = 2
    margin_l: int = 10
    margin_r: int = 10
    margin_t: int = 10  # MarginV
    encoding: int = 1

    @staticmethod
    def _num(v: float) -> str:
        return str(int(v)) if float(v).is_integer() else str(v)

    def to_fields(self) -> list[str]:
        b = lambda x: "-1" if x else "0"
        n = self._num
        return [self.name, self.fontname, n(self.fontsize), self.primary, self.secondary, self.outline_c,
                self.back, b(self.bold), b(self.italic), b(self.underline), b(self.strikeout),
                n(self.scale_x), n(self.scale_y), n(self.spacing), n(self.angle), str(self.borderstyle),
                n(self.outline), n(self.shadow), str(self.align), str(self.margin_l), str(self.margin_r),
                str(self.margin_t), str(self.encoding)]

    @classmethod
    def from_fields(cls, f: dict[str, str]) -> "Style":
        def fl(k, d=0.0):
            try:
                return float(f.get(k, d))
            except ValueError:
                return d
        bl = lambda k: f.get(k, "0").strip() not in ("0", "")
        return cls(
            name=f.get("Name", "Default"), fontname=f.get("Fontname", "Arial"), fontsize=fl("Fontsize", 48),
            primary=f.get("PrimaryColour", "&H00FFFFFF"), secondary=f.get("SecondaryColour", "&H000000FF"),
            outline_c=f.get("OutlineColour", f.get("TertiaryColour", "&H00000000")),
            back=f.get("BackColour", "&H00000000"), bold=bl("Bold"), italic=bl("Italic"),
            underline=bl("Underline"), strikeout=bl("StrikeOut"), scale_x=fl("ScaleX", 100),
            scale_y=fl("ScaleY", 100), spacing=fl("Spacing"), angle=fl("Angle"),
            borderstyle=int(fl("BorderStyle", 1)), outline=fl("Outline", 2), shadow=fl("Shadow"),
            align=int(fl("Alignment", 2)), margin_l=int(fl("MarginL", 10)), margin_r=int(fl("MarginR", 10)),
            margin_t=int(fl("MarginV", 10)), encoding=int(fl("Encoding", 1)))


@dataclass
class Line:
    text: str = ""
    start: int = 0          # ms
    end: int = 0            # ms
    style: str = "Default"
    actor: str = ""
    layer: int = 0
    margin_l: int = 0
    margin_r: int = 0
    margin_t: int = 0
    effect: str = ""
    comment: bool = False

    @property
    def duration(self) -> int:
        return self.end - self.start

    def clone(self) -> "Line":
        return replace(self)

    @property
    def plain(self) -> str:
        """Etiketsiz metin; \\N ve \\n boşluk/satır sonuna çevrilir."""
        t = re.sub(r"\{[^}]*\}", "", self.text)
        return t.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ")


@dataclass
class AssFile:
    info: list[tuple[str, str]] = field(default_factory=list)   # sıralı Script Info
    styles: list[Style] = field(default_factory=list)
    events: list[Line] = field(default_factory=list)
    extra_sections: list[tuple[str, list[str]]] = field(default_factory=list)  # kayıpsız saklanan bölümler
    header_comments: list[str] = field(default_factory=list)

    # ---- Script Info yardımcıları
    def get_info(self, key: str, default: str = "") -> str:
        for k, v in self.info:
            if k.lower() == key.lower():
                return v
        return default

    def set_info(self, key: str, value) -> None:
        value = str(value)
        for i, (k, _) in enumerate(self.info):
            if k.lower() == key.lower():
                self.info[i] = (k, value)
                return
        self.info.append((key, value))

    @property
    def res(self) -> tuple[int, int]:
        return int(self.get_info("PlayResX", "384") or 384), int(self.get_info("PlayResY", "288") or 288)

    def style(self, name: str) -> Optional[Style]:
        for s in self.styles:
            if s.name == name:
                return s
        return None

    @staticmethod
    def new() -> "AssFile":
        a = AssFile()
        a.info = [("Title", "Untitled"), ("ScriptType", "v4.00+"), ("WrapStyle", "0"),
                  ("ScaledBorderAndShadow", "yes"), ("YCbCr Matrix", "TV.709"),
                  ("PlayResX", "1920"), ("PlayResY", "1080")]
        a.styles = [Style(fontsize=60)]
        return a

    # ---- Ayrıştırma
    @classmethod
    def parse(cls, text: str) -> "AssFile":
        text = text.lstrip("﻿")
        a = cls()
        section = None
        sec_lines: list[str] = []
        fmt_ev, fmt_st = EVENT_FORMAT, STYLE_FORMAT
        for raw in text.splitlines():
            line = raw.rstrip("\r\n")
            ms = re.fullmatch(r"\s*\[(.+?)\]\s*", line)
            if ms:
                if section and section.lower() not in ("script info", "v4+ styles", "v4 styles", "events"):
                    a.extra_sections.append((section, _trim(sec_lines)))
                section, sec_lines = ms.group(1), []
                continue
            low = (section or "").lower()
            if low == "script info":
                if line.startswith(";") or line.startswith("!"):
                    a.info.append((line, "\0comment"))
                elif ":" in line:
                    k, v = line.split(":", 1)
                    a.info.append((k.strip(), v.strip()))
            elif low in ("v4+ styles", "v4 styles"):
                if line.lower().startswith("format:"):
                    fmt_st = [x.strip() for x in line.split(":", 1)[1].split(",")]
                elif line.lower().startswith("style:"):
                    vals = [x.strip() for x in line.split(":", 1)[1].split(",", len(fmt_st) - 1)]
                    a.styles.append(Style.from_fields(dict(zip(fmt_st, vals))))
            elif low == "events":
                if line.lower().startswith("format:"):
                    fmt_ev = [x.strip() for x in line.split(":", 1)[1].split(",")]
                else:
                    m = re.match(r"(Dialogue|Comment):\s?(.*)", line)
                    if m:
                        vals = m.group(2).split(",", len(fmt_ev) - 1)
                        d = dict(zip(fmt_ev, vals))
                        if low and "Text" in d:
                            try:
                                a.events.append(Line(
                                    text=d["Text"], start=ass_to_ms(d.get("Start", "0:00:00.00")),
                                    end=ass_to_ms(d.get("End", "0:00:00.00")), style=d.get("Style", "Default").strip(),
                                    actor=d.get("Name", d.get("Actor", "")).strip(), layer=int(d.get("Layer", d.get("Marked", "0")).replace("Marked=", "") or 0),
                                    margin_l=int(d.get("MarginL", 0) or 0), margin_r=int(d.get("MarginR", 0) or 0),
                                    margin_t=int(d.get("MarginV", 0) or 0), effect=d.get("Effect", "").strip(),
                                    comment=m.group(1) == "Comment"))
                            except ValueError:
                                pass
            else:
                sec_lines.append(line)
        if section and section.lower() not in ("script info", "v4+ styles", "v4 styles", "events"):
            a.extra_sections.append((section, _trim(sec_lines)))
        if not a.styles:
            a.styles = [Style()]
        return a

    # ---- Yazma
    def dumps(self) -> str:
        out = ["[Script Info]"]
        for k, v in self.info:
            out.append(k if v == "\0comment" else f"{k}: {v}")
        out += ["", "[V4+ Styles]", "Format: " + ", ".join(STYLE_FORMAT)]
        for s in self.styles:
            out.append("Style: " + ",".join(s.to_fields()))
        for name, lines in self.extra_sections:
            out += ["", f"[{name}]"] + lines
        out += ["", "[Events]", "Format: " + ", ".join(EVENT_FORMAT)]
        for e in self.events:
            out.append(f"{'Comment' if e.comment else 'Dialogue'}: {e.layer},{ms_to_ass(e.start)},{ms_to_ass(e.end)},"
                       f"{e.style},{e.actor},{e.margin_l},{e.margin_r},{e.margin_t},{e.effect},{e.text}")
        return "\n".join(out) + "\n"

    @classmethod
    def load(cls, path: str) -> "AssFile":
        from .encoding import read_text
        return cls.parse(read_text(path))

    def save(self, path: str) -> None:
        with open(path, "w", encoding="utf-8-sig", newline="\n") as f:
            f.write(self.dumps())
