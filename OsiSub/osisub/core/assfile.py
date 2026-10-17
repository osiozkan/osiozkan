"""Bayt korumalı ASS belge modeli.

Temel kural: dokunulmayan her satır, dosyaya okunduğu gibi (bayt bayt)
geri yazılır. Bunu her satır nesnesinde tutulan `raw` + `sig` ikilisi
sağlar: `sig`, satır okunduğu andaki alanların demetidir. Yazarken alanlar
hâlâ `sig` ile aynıysa `raw` aynen basılır; değilse satır, bölümün kendi
Format sırasına göre yeniden üretilir. Böylece:

  * UTF-8 BOM ve CRLF/LF korunur,
  * "Marked" gibi bilinmeyen Format alanları korunur,
  * "0000" biçimli kenar boşlukları, fazladan boşluklar vb. değişmez,
  * yalnız gerçekten düzenlenen satır yeniden biçimlenir.

Satır nesneleri DEĞİŞMEZ kabul edilir: düzenleme `ev.replace(...)` ile yeni
nesne üretir. Geri al (undo) anlık görüntüleri bu sayede yalnızca liste
kopyasıdır.
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field
from typing import Iterable

from .asstime import format_time, parse_time, round_ms

# ---------------------------------------------------------------------------
# Sayı biçimleme (Aegisub float_to_string ile uyumlu: gereksiz sıfır yok)
# ---------------------------------------------------------------------------


def fmt_num(v: float) -> str:
    if isinstance(v, bool):
        return "-1" if v else "0"
    if float(v).is_integer():
        s = str(int(v))
        return "0" if s == "-0" else s
    s = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def _to_float(s: str, default: float = 0.0) -> float:
    try:
        return float(s.strip())
    except (ValueError, AttributeError):
        return default


def _to_int(s: str, default: int = 0) -> int:
    try:
        return int(float(s.strip()))
    except (ValueError, AttributeError):
        return default


def _to_bool(s: str) -> bool:
    return _to_int(s) != 0


# ---------------------------------------------------------------------------
# Satır türleri
# ---------------------------------------------------------------------------

DEFAULT_EVENT_FORMAT = [
    "Layer", "Start", "End", "Style", "Name",
    "MarginL", "MarginR", "MarginV", "Effect", "Text",
]
DEFAULT_STYLE_FORMAT = [
    "Name", "Fontname", "Fontsize", "PrimaryColour", "SecondaryColour",
    "OutlineColour", "BackColour", "Bold", "Italic", "Underline", "StrikeOut",
    "ScaleX", "ScaleY", "Spacing", "Angle", "BorderStyle", "Outline",
    "Shadow", "Alignment", "MarginL", "MarginR", "MarginV", "Encoding",
]


@dataclass(slots=True, eq=False)
class RawLine:
    """Ayrıştırılmayan satır (yorum, boş satır, bilinmeyen bölüm içeriği)."""
    raw: str

    def serialize(self, fmt=None) -> str:
        return self.raw


@dataclass(slots=True, eq=False)
class InfoItem:
    key: str
    value: str
    raw: str | None = None
    sig: tuple | None = None

    def _key(self):
        return (self.key, self.value)

    def serialize(self, fmt=None) -> str:
        if self.raw is not None and self.sig == self._key():
            return self.raw
        return f"{self.key}: {self.value}"

    def replace(self, **kw) -> "InfoItem":
        return dataclasses.replace(self, **kw)

    @classmethod
    def parse(cls, line: str) -> "InfoItem | None":
        if ":" not in line:
            return None
        k, v = line.split(":", 1)
        item = cls(k.strip(), v.lstrip(), raw=line)
        item.sig = item._key()
        return item


@dataclass(slots=True, eq=False)
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
    margin_v: int = 10
    encoding: int = 1
    extra: tuple = ()           # ((alan_adı, ham_değer), ...) bilinmeyen alanlar
    raw: str | None = None
    sig: tuple | None = None
    lead: tuple = ()            # önündeki ham satırlar (yorum/boş)
    prefix: str = "Style: "
    rawvals: tuple = ()         # Format sırasıyla özgün alan metinleri

    _MAP = {
        "Name": ("name", str), "Fontname": ("fontname", str),
        "Fontsize": ("fontsize", float), "PrimaryColour": ("color1", str),
        "SecondaryColour": ("color2", str), "OutlineColour": ("color3", str),
        "TertiaryColour": ("color3", str), "BackColour": ("color4", str),
        "Bold": ("bold", bool), "Italic": ("italic", bool),
        "Underline": ("underline", bool), "StrikeOut": ("strikeout", bool),
        "ScaleX": ("scale_x", float), "ScaleY": ("scale_y", float),
        "Spacing": ("spacing", float), "Angle": ("angle", float),
        "BorderStyle": ("borderstyle", int), "Outline": ("outline", float),
        "Shadow": ("shadow", float), "Alignment": ("align", int),
        "MarginL": ("margin_l", int), "MarginR": ("margin_r", int),
        "MarginV": ("margin_v", int), "Encoding": ("encoding", int),
    }

    def _key(self):
        return (self.name, self.fontname, self.fontsize, self.color1, self.color2,
                self.color3, self.color4, self.bold, self.italic, self.underline,
                self.strikeout, self.scale_x, self.scale_y, self.spacing, self.angle,
                self.borderstyle, self.outline, self.shadow, self.align,
                self.margin_l, self.margin_r, self.margin_v, self.encoding, self.extra)

    def replace(self, **kw) -> "Style":
        return dataclasses.replace(self, **kw)

    @classmethod
    def parse(cls, line: str, fmt: list[str]) -> "Style":
        head, _, body = line.partition(":")
        prefix = line[: len(head) + 1]
        if body.startswith(" "):
            prefix += " "
            body = body[1:]
        vals = body.split(",", len(fmt) - 1)
        st = cls(raw=line, prefix=prefix if prefix.strip() == "Style:" else "Style: ",
                 rawvals=tuple(vals))
        extra = []
        for i, name in enumerate(fmt):
            v = vals[i] if i < len(vals) else ""
            m = cls._MAP.get(name)
            if m is None:
                extra.append((name, v))
                continue
            attr, typ = m
            if typ is str:
                setattr(st, attr, v.strip())
            elif typ is float:
                setattr(st, attr, _to_float(v))
            elif typ is int:
                setattr(st, attr, _to_int(v))
            elif typ is bool:
                setattr(st, attr, _to_bool(v))
        st.extra = tuple(extra)
        st.sig = st._key()
        return st

    _KEYNAMES = ("name", "fontname", "fontsize", "color1", "color2", "color3", "color4",
                 "bold", "italic", "underline", "strikeout", "scale_x", "scale_y", "spacing",
                 "angle", "borderstyle", "outline", "shadow", "align", "margin_l", "margin_r",
                 "margin_v", "encoding", "extra")

    def serialize(self, fmt: list[str]) -> str:
        if self.raw is not None and self.sig == self._key():
            return self.raw
        extra = dict(self.extra)
        # Değişmeyen alanlar özgün metniyle yazılır (ör. "0080" kenar boşluğu)
        orig = dict(zip(self._KEYNAMES, self.sig)) if self.sig else {}
        rv = self.rawvals if self.sig and len(self.rawvals) == len(fmt) else ()
        out = []
        for i, name in enumerate(fmt):
            m = self._MAP.get(name)
            if m is None:
                out.append(extra.get(name, "0"))
                continue
            attr, typ = m
            v = getattr(self, attr)
            if rv and attr in orig and orig[attr] == v and type(orig[attr]) is type(v):
                out.append(rv[i])
                continue
            if typ is str:
                out.append(str(v))
            elif typ is bool:
                out.append("-1" if v else "0")
            elif typ is int:
                out.append(str(int(v)))
            else:
                out.append(fmt_num(v))
        return (self.prefix or "Style: ") + ",".join(out)


@dataclass(slots=True, eq=False)
class Event:
    comment: bool = False
    layer: int = 0
    start: int = 0              # ms
    end: int = 5000             # ms
    style: str = "Default"
    actor: str = ""
    margin_l: int = 0
    margin_r: int = 0
    margin_v: int = 0           # = MarginV (Lua'da margin_t)
    margin_b: int = 0           # yalnız MarginB içeren biçimlerde anlamlı
    effect: str = ""
    text: str = ""
    extra: tuple = ()           # bilinmeyen Format alanları (ör. Marked)
    raw: str | None = None
    sig: tuple | None = None
    lead: tuple = ()
    prefix: str = ""            # "Dialogue: " / "Comment: " ham ön ek
    rawvals: tuple = ()         # Format sırasıyla özgün alan metinleri

    _MAP = {
        "Layer": "layer", "Start": "start", "End": "end", "Style": "style",
        "Name": "actor", "Actor": "actor", "MarginL": "margin_l",
        "MarginR": "margin_r", "MarginV": "margin_v", "MarginT": "margin_v",
        "MarginB": "margin_b", "Effect": "effect", "Text": "text",
    }

    def _key(self):
        return (self.comment, self.layer, self.start, self.end, self.style,
                self.actor, self.margin_l, self.margin_r, self.margin_v,
                self.margin_b, self.effect, self.text, self.extra)

    @property
    def modified(self) -> bool:
        return self.raw is None or self.sig != self._key()

    @property
    def duration(self) -> int:
        return self.end - self.start

    def replace(self, **kw) -> "Event":
        if "start" in kw:
            kw["start"] = round_ms(kw["start"])
        if "end" in kw:
            kw["end"] = round_ms(kw["end"])
        return dataclasses.replace(self, **kw)

    def fresh(self) -> "Event":
        """Ham bağı koparılmış kopya (yeni satır eklerken)."""
        return dataclasses.replace(self, raw=None, sig=None, lead=(), prefix="", rawvals=())

    @classmethod
    def parse(cls, line: str, fmt: list[str]) -> "Event":
        head, _, body = line.partition(":")
        comment = head.strip().lower() == "comment"
        prefix = line[: len(head) + 1]
        if body.startswith(" "):
            prefix += " "
            body = body[1:]
        vals = body.split(",", len(fmt) - 1)
        ev = cls(comment=comment, raw=line, prefix=prefix, rawvals=tuple(vals))
        extra = []
        has_b = "MarginB" in fmt
        for i, name in enumerate(fmt):
            v = vals[i] if i < len(vals) else ""
            attr = cls._MAP.get(name)
            if attr is None:
                extra.append((name, v))
            elif attr in ("start", "end"):
                setattr(ev, attr, parse_time(v))
            elif attr == "text":
                ev.text = v
            elif attr in ("style", "actor", "effect"):
                setattr(ev, attr, v.strip())
            else:
                setattr(ev, attr, _to_int(v))
        if not has_b:
            ev.margin_b = ev.margin_v
        ev.extra = tuple(extra)
        ev.sig = ev._key()
        return ev

    _KEYNAMES = ("comment", "layer", "start", "end", "style", "actor", "margin_l", "margin_r",
                 "margin_v", "margin_b", "effect", "text", "extra")

    def serialize(self, fmt: list[str]) -> str:
        if self.raw is not None and self.sig == self._key():
            return self.raw
        extra = dict(self.extra)
        # Osi kuralı: düzenlenen satırda bile DEĞİŞMEYEN alanlar (Marked, Style,
        # "0000" kenar boşlukları, zaman yazımı...) bayt bayt korunur.
        orig = dict(zip(self._KEYNAMES, self.sig)) if self.sig else {}
        rv = self.rawvals if self.sig and len(self.rawvals) == len(fmt) else ()
        out = []
        for i, name in enumerate(fmt):
            attr = self._MAP.get(name)
            if attr is None:
                out.append(extra.get(name, "0"))
            elif rv and attr in orig and orig[attr] == getattr(self, attr):
                out.append(rv[i])
            elif attr in ("start", "end"):
                out.append(format_time(getattr(self, attr)))
            elif attr in ("text", "style", "actor", "effect"):
                out.append(getattr(self, attr))
            else:
                out.append(str(int(getattr(self, attr))))
        if self.prefix and self.sig and orig.get("comment") == self.comment:
            return self.prefix + ",".join(out)
        kind = "Comment" if self.comment else "Dialogue"
        return f"{kind}: " + ",".join(out)


# ---------------------------------------------------------------------------
# Bölüm ve belge
# ---------------------------------------------------------------------------


@dataclass(eq=False)
class Section:
    name: str                       # "[Events]"
    header_raw: str                 # dosyadaki başlık satırı aynen
    kind: str                       # info | styles | events | raw
    pre: list = field(default_factory=list)       # Format'tan önceki ham satırlar
    format_raw: str | None = None
    format_fields: list = field(default_factory=list)
    items: list = field(default_factory=list)
    post: list = field(default_factory=list)      # bölüm sonundaki boş/ham satırlar

    def serialize_lines(self) -> list[str]:
        out = [self.header_raw]
        out.extend(self.pre)
        if self.format_raw is not None:
            out.append(self.format_raw)
        fmt = self.format_fields
        for it in self.items:
            lead = getattr(it, "lead", ())
            if lead:
                out.extend(lead)
            out.append(it.serialize(fmt))
        out.extend(self.post)
        return out


SECTION_KINDS = {
    "[script info]": "info",
    "[aegisub project garbage]": "info",
    "[v4+ styles]": "styles",
    "[v4 styles]": "styles",
    "[v4 styles+]": "styles",
    "[events]": "events",
}


class AssDocument:
    """Bir .ass dosyası. Bölüm sırası ve tüm ham satırlar korunur."""

    def __init__(self):
        self.path: str | None = None
        self.bom = True
        self.newline = "\r\n"
        self.final_newline = True
        self.preamble: list[str] = []
        self.sections: list[Section] = []

    # ---------------------------------------------------------------- yükleme
    @classmethod
    def new(cls) -> "AssDocument":
        doc = cls()
        info = Section("[Script Info]", "[Script Info]", "info")
        info.pre = ["; Script generated by OsiSub"]
        for k, v in (("Title", "Default OsiSub file"), ("ScriptType", "v4.00+"),
                     ("WrapStyle", "0"), ("ScaledBorderAndShadow", "yes"),
                     ("YCbCr Matrix", "None"), ("PlayResX", "1920"),
                     ("PlayResY", "1080")):
            info.items.append(InfoItem(k, v))
        info.post = [""]
        styles = Section("[V4+ Styles]", "[V4+ Styles]", "styles")
        styles.format_fields = list(DEFAULT_STYLE_FORMAT)
        styles.format_raw = "Format: " + ", ".join(DEFAULT_STYLE_FORMAT)
        styles.items.append(Style(fontsize=72, margin_l=20, margin_r=20, margin_v=40))
        styles.post = [""]
        events = Section("[Events]", "[Events]", "events")
        events.format_fields = list(DEFAULT_EVENT_FORMAT)
        events.format_raw = "Format: " + ", ".join(DEFAULT_EVENT_FORMAT)
        events.items.append(Event())
        doc.sections = [info, styles, events]
        return doc

    @classmethod
    def load(cls, path: str) -> "AssDocument":
        with open(path, "rb") as f:
            data = f.read()
        doc = cls.from_bytes(data)
        doc.path = path
        return doc

    @classmethod
    def from_bytes(cls, data: bytes) -> "AssDocument":
        doc = cls()
        doc.bom = data.startswith(b"\xef\xbb\xbf")
        if doc.bom:
            data = data[3:]
        text = data.decode("utf-8", errors="surrogateescape")
        doc.newline = "\r\n" if "\r\n" in text else "\n"
        doc.final_newline = text.endswith("\n")
        lines = text.split("\n")
        if doc.final_newline:
            lines.pop()
        # CRLF: her satırın sonundaki \r'yi ayır. Karışık satır sonlarında
        # yalnız \r'li satırlar CRLF sayılır; tutarlılık için belge geneli seçilir.
        lines = [ln[:-1] if ln.endswith("\r") else ln for ln in lines]
        doc._parse_lines(lines)
        return doc

    @classmethod
    def from_text(cls, text: str) -> "AssDocument":
        return cls.from_bytes(text.encode("utf-8", errors="surrogateescape"))

    def _parse_lines(self, lines: list[str]):
        cur: Section | None = None
        pending: list[str] = []       # sıradaki öğenin önüne gidecek ham satırlar

        def flush_pending_to_post():
            nonlocal pending
            if cur is not None:
                cur.post.extend(pending)
            else:
                self.preamble.extend(pending)
            pending = []

        for ln in lines:
            s = ln.strip()
            if s.startswith("[") and s.endswith("]") and len(s) > 2:
                flush_pending_to_post()
                kind = SECTION_KINDS.get(s.lower(), "raw")
                cur = Section(s, ln, kind)
                self.sections.append(cur)
                continue
            if cur is None:
                self.preamble.append(ln)
                continue
            if cur.kind == "raw":
                cur.items.append(RawLine(ln))
                continue
            if cur.kind == "info":
                if not s or s.startswith(";") or ":" not in s:
                    if cur.items:
                        pending.append(ln)
                    else:
                        cur.pre.append(ln)
                    continue
                item = InfoItem.parse(ln)
                for p in pending:
                    cur.items.append(RawLine(p))
                pending = []
                cur.items.append(item)
                continue
            # styles / events
            low = s.lower()
            if low.startswith("format:") and cur.format_raw is None:
                cur.pre.extend(pending)
                pending = []
                cur.format_raw = ln
                cur.format_fields = [f.strip() for f in s.split(":", 1)[1].split(",")]
                continue
            if cur.kind == "styles" and low.startswith("style:"):
                if cur.format_raw is None:
                    cur.format_fields = list(DEFAULT_STYLE_FORMAT)
                st = Style.parse(ln, cur.format_fields)
                st.lead = tuple(pending)
                pending = []
                cur.items.append(st)
                continue
            if cur.kind == "events" and (low.startswith("dialogue:") or low.startswith("comment:")):
                if cur.format_raw is None:
                    cur.format_fields = list(DEFAULT_EVENT_FORMAT)
                ev = Event.parse(ln, cur.format_fields)
                ev.lead = tuple(pending)
                pending = []
                cur.items.append(ev)
                continue
            # tanınmayan satır: bir sonraki öğenin önüne iliştir
            if not cur.items and cur.format_raw is None:
                cur.pre.append(ln)
            else:
                pending.append(ln)
        flush_pending_to_post()
        # Aegisub Project Garbage'daki ham satırları info öğesine çevirme;
        # yalnız sözlük olarak okunur (garbage()).

    # ------------------------------------------------------------- erişim
    def section(self, kind: str, name: str | None = None) -> Section | None:
        for s in self.sections:
            if s.kind == kind and (name is None or s.name.lower() == name.lower()):
                return s
        return None

    def ensure_section(self, kind: str) -> Section:
        s = self.section(kind, "[Script Info]" if kind == "info" else None)
        if s is not None:
            return s
        if kind == "info":
            s = Section("[Script Info]", "[Script Info]", "info", post=[""])
            self.sections.insert(0, s)
        elif kind == "styles":
            s = Section("[V4+ Styles]", "[V4+ Styles]", "styles",
                        format_fields=list(DEFAULT_STYLE_FORMAT),
                        format_raw="Format: " + ", ".join(DEFAULT_STYLE_FORMAT), post=[""])
            idx = 1 if self.sections and self.sections[0].kind == "info" else 0
            self.sections.insert(idx, s)
        elif kind == "events":
            s = Section("[Events]", "[Events]", "events",
                        format_fields=list(DEFAULT_EVENT_FORMAT),
                        format_raw="Format: " + ", ".join(DEFAULT_EVENT_FORMAT))
            # Events, stillerden hemen sonra gelir
            pos = len(self.sections)
            for i, sec in enumerate(self.sections):
                if sec.kind == "styles":
                    pos = i + 1
            self.sections.insert(pos, s)
        else:
            raise ValueError(kind)
        return s

    @property
    def events(self) -> list[Event]:
        return self.ensure_section("events").items

    @property
    def styles(self) -> list[Style]:
        return self.ensure_section("styles").items

    @property
    def info_section(self) -> Section:
        return self.ensure_section("info")

    def info_items(self) -> list[InfoItem]:
        return [it for it in self.info_section.items if isinstance(it, InfoItem)]

    def get_info(self, key: str, default: str | None = None) -> str | None:
        kl = key.lower()
        for it in self.info_items():
            if it.key.lower() == kl:
                return it.value
        return default

    def set_info(self, key: str, value: str | None):
        sec = self.info_section
        kl = key.lower()
        for i, it in enumerate(sec.items):
            if isinstance(it, InfoItem) and it.key.lower() == kl:
                if value is None:
                    del sec.items[i]
                elif it.value != value:
                    sec.items[i] = it.replace(value=value)
                return
        if value is not None:
            sec.items.append(InfoItem(key, value))

    def garbage(self) -> dict[str, str]:
        """[Aegisub Project Garbage] anahtarları (Video File, Audio File ...)."""
        s = self.section("info", "[Aegisub Project Garbage]")
        if not s:
            return {}
        return {it.key: it.value for it in s.items if isinstance(it, InfoItem)}

    def set_garbage(self, key: str, value: str | None):
        """[Aegisub Project Garbage] anahtarı yaz (yalnız değer değiştiyse dokunur)."""
        sec = self.section("info", "[Aegisub Project Garbage]")
        if sec is None:
            if value is None:
                return
            sec = Section("[Aegisub Project Garbage]", "[Aegisub Project Garbage]", "info", post=[""])
            pos = 1 if self.sections and self.sections[0].kind == "info" else 0
            self.sections.insert(pos, sec)
        kl = key.lower()
        for i, it in enumerate(sec.items):
            if isinstance(it, InfoItem) and it.key.lower() == kl:
                if value is None:
                    del sec.items[i]
                elif it.value != value:
                    sec.items[i] = it.replace(value=value)
                return
        if value is not None:
            sec.items.append(InfoItem(key, value))

    def style_by_name(self, name: str) -> Style | None:
        for st in self.styles:
            if st.name == name:
                return st
        # VSFilter/libass gibi: bulunamazsa Default
        return None

    def play_res(self) -> tuple[int, int]:
        x = _to_int(self.get_info("PlayResX", "0") or "0")
        y = _to_int(self.get_info("PlayResY", "0") or "0")
        if x <= 0 and y <= 0:
            return 384, 288
        if x <= 0:
            x = 1280 if y == 1024 else y * 4 // 3
        if y <= 0:
            y = 1024 if x == 1280 else x * 3 // 4
        return x, y

    # -------------------------------------------------------------- yazma
    def to_text(self) -> str:
        lines = list(self.preamble)
        for s in self.sections:
            lines.extend(s.serialize_lines())
        nl = self.newline
        text = nl.join(lines)
        if self.final_newline:
            text += nl
        return text

    def to_bytes(self) -> bytes:
        data = self.to_text().encode("utf-8", errors="surrogateescape")
        return (b"\xef\xbb\xbf" + data) if self.bom else data

    def save(self, path: str | None = None):
        path = path or self.path
        if not path:
            raise ValueError("Kayıt yolu yok")
        data = self.to_bytes()
        tmp = path + ".osisub-tmp"
        with open(tmp, "wb") as f:
            f.write(data)
        import os
        os.replace(tmp, path)
        self.path = path

    def clone(self) -> "AssDocument":
        """Satır nesnelerini paylaşan, listeleri ayrı kopya (makro çalışma kopyası)."""
        d = AssDocument()
        d.path, d.bom, d.newline, d.final_newline = self.path, self.bom, self.newline, self.final_newline
        d.preamble = list(self.preamble)
        for s in self.sections:
            d.sections.append(Section(s.name, s.header_raw, s.kind, list(s.pre), s.format_raw,
                                      list(s.format_fields), list(s.items), list(s.post)))
        return d

    def adopt(self, other: "AssDocument"):
        """Başka bir belgenin içeriğini al (makro sonucu ana belgeye geçerken)."""
        self.preamble = other.preamble
        self.sections = other.sections

    # ---------------------------------------------------- anlık görüntü
    def snapshot(self):
        """Geri al için ucuz kopya: yalnız liste referansları kopyalanır."""
        return tuple((s, list(s.pre), s.format_raw, list(s.format_fields),
                      list(s.items), list(s.post)) for s in self.sections), list(self.preamble)

    def restore(self, snap):
        secs, pre = snap
        self.preamble = list(pre)
        self.sections = []
        for s, p, fr, ff, items, post in secs:
            s.pre, s.format_raw, s.format_fields = list(p), fr, list(ff)
            s.items, s.post = list(items), list(post)
            self.sections.append(s)


def iter_dialogue(doc: AssDocument) -> Iterable[tuple[int, Event]]:
    for i, ev in enumerate(doc.events):
        if not ev.comment:
            yield i, ev
