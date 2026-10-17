"""Diğer altyazı biçimleri.

Okuma: SRT, WebVTT, TTML/DFXP (core/ttml.py), YouTube SBV, MicroDVD (.sub, kare tabanlı).
Yazma: SRT, WebVTT, TTML/DFXP, SBV, MicroDVD, düz metin (zamanlı/zamansız).

Okuma sonucu her zaman bir AssDocument'tır (tek "Default" stil), böylece
referans altyazı, içe aktarma ve aktarma aracı aynı modelle çalışır.
SRT etiketleri: <i> <b> <u> <s> -> {\\i1}… ; <font color="#RRGGBB"> -> {\\c&HBBGGRR&};
satır sonları -> \\N. Kodlama: UTF-8 (BOM'lu/BOM'suz), olmazsa cp1254 (Türkçe Windows).
"""

from __future__ import annotations

import re

from .assfile import AssDocument, Event
from .asstime import format_time, round_ms
from .textutil import strip_tags

_TIME = re.compile(r"(\d+):(\d{1,2}):(\d{1,2})[,.](\d{1,3})")
_ARROW = re.compile(r"(\d+:\d{1,2}:\d{1,2}[,.]\d{1,3})\s*-->\s*(\d+:\d{1,2}:\d{1,2}[,.]\d{1,3})")
_VTT_ARROW = re.compile(r"((?:\d+:)?\d{1,2}:\d{1,2}\.\d{1,3})\s*-->\s*((?:\d+:)?\d{1,2}:\d{1,2}\.\d{1,3})")


def decode_text(data: bytes) -> str:
    if data.startswith(b"\xef\xbb\xbf"):
        return data[3:].decode("utf-8", errors="replace")
    if data.startswith((b"\xff\xfe", b"\xfe\xff")):
        return data.decode("utf-16")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("cp1254", errors="replace")


def _ms(t: str) -> int:
    parts = t.replace(",", ".").split(":")
    if len(parts) == 2:
        parts = ["0"] + parts
    h, m, s = parts
    sec, _, frac = s.partition(".")
    frac = (frac + "000")[:3]
    return ((int(h) * 60 + int(m)) * 60 + int(sec)) * 1000 + int(frac)


def html_to_ass(text: str) -> str:
    def font(m):
        col = re.search(r'color\s*=\s*"?#?([0-9a-fA-F]{6})', m.group(0))
        if col:
            rgb = col.group(1)
            return "{\\c&H%s%s%s&}" % (rgb[4:6], rgb[2:4], rgb[0:2])
        return ""
    t = text
    t = re.sub(r"<font[^>]*>", font, t, flags=re.I)
    t = re.sub(r"</font>", "{\\\\c}", t, flags=re.I)
    for tag in "ibus":
        t = re.sub(rf"<{tag}>", "{\\\\%s1}" % tag, t, flags=re.I)
        t = re.sub(rf"</{tag}>", "{\\\\%s0}" % tag, t, flags=re.I)
    t = re.sub(r"<[^>]+>", "", t)                       # kalan HTML/VTT etiketleri
    t = t.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", "\\h")
    t = t.replace("}{", "")                              # ardışık blokları birleştir
    return t


def ass_to_html(text: str) -> str:
    """SRT yazarken: {\\i1} gibi temel etiketler korunur, diğerleri atılır."""
    out = []
    for m in re.finditer(r"\{([^}]*)\}|([^{]+)", text):
        if m.group(2) is not None:
            out.append(m.group(2))
            continue
        for tag, val in re.findall(r"\\([ibus])(\d)", m.group(1)):
            out.append(f"<{tag}>" if val == "1" else f"</{tag}>")
    s = "".join(out)
    return s.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", "\u00a0")


def _blocks(text: str):
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    return re.split(r"\n\s*\n", text.strip("\n"))


def parse_srt(text: str) -> list[Event]:
    events = []
    for blk in _blocks(text):
        lines = blk.split("\n")
        for i, ln in enumerate(lines[:3]):
            m = _ARROW.search(ln)
            if m:
                body = "\n".join(lines[i + 1:]).strip("\n")
                events.append(Event(start=round_ms(_ms(m.group(1))), end=round_ms(_ms(m.group(2))),
                                    text=html_to_ass(body).replace("\n", "\\N")))
                break
    return events


def parse_vtt(text: str) -> list[Event]:
    events = []
    for blk in _blocks(text):
        lines = blk.split("\n")
        for i, ln in enumerate(lines[:3]):
            m = _VTT_ARROW.search(ln)
            if m:
                body = "\n".join(lines[i + 1:]).strip("\n")
                events.append(Event(start=round_ms(_ms(m.group(1))), end=round_ms(_ms(m.group(2))),
                                    text=html_to_ass(body).replace("\n", "\\N")))
                break
    return events


_SBV_TIME = re.compile(r"^(\d+:\d{1,2}:\d{1,2}\.\d{1,3}),(\d+:\d{1,2}:\d{1,2}\.\d{1,3})\s*$")
_MDVD = re.compile(r"^\{(\d+)\}\{(\d*)\}(.*)$")
DEFAULT_FPS = 24000 / 1001


def parse_sbv(text: str) -> list[Event]:
    events = []
    for blk in _blocks(text):
        lines = blk.split("\n")
        m = _SBV_TIME.match(lines[0].strip())
        if m:
            body = "\n".join(lines[1:]).strip("\n")
            events.append(Event(start=round_ms(_ms(m.group(1))), end=round_ms(_ms(m.group(2))),
                                text=html_to_ass(body).replace("\n", "\\N")))
    return events


def microdvd_fps_hint(text: str) -> float | None:
    """İlk satır {1}{1}23.976 biçimindeyse kare hızı."""
    for ln in text.splitlines()[:3]:
        m = _MDVD.match(ln.strip())
        if m and m.group(1) in ("0", "1") and m.group(2) in ("0", "1"):
            try:
                v = float(m.group(3).strip().replace(",", "."))
                return v if 1 < v < 300 else None
            except ValueError:
                return None
    return None


def is_microdvd(text: str) -> bool:
    lines = [ln for ln in text.splitlines() if ln.strip()][:5]
    return bool(lines) and all(_MDVD.match(ln.strip()) for ln in lines)


def parse_microdvd(text: str, fps: float | None = None) -> list[Event]:
    """{başlangıç kare}{bitiş kare}Metin|ikinci satır; {y:i} → italik. fps verilmezse dosyadaki ipucu
    ({1}{1}23.976), o da yoksa 23.976."""
    hint = microdvd_fps_hint(text)
    fps = fps or hint or DEFAULT_FPS
    events = []
    for ln in text.splitlines():
        m = _MDVD.match(ln.strip())
        if not m:
            continue
        a, b, body = int(m.group(1)), m.group(2), m.group(3)
        if hint and a in (0, 1) and b in ("0", "1"):
            continue
        end = int(b) if b else a + int(round(fps * 2))
        tags = ""
        for code in re.findall(r"\{[yY]:([^}]*)\}", body):
            for flag in "ibus":
                if flag in code.lower():
                    tags += f"\\{flag}1"
        body = re.sub(r"\{[^}]*\}", "", body)
        txt = ("{" + tags + "}" if tags else "") + body.replace("|", "\\N")
        events.append(Event(start=round_ms(a * 1000.0 / fps), end=round_ms(end * 1000.0 / fps), text=txt))
    return events


def load_any(path: str, fps: float | None = None) -> AssDocument:
    """.ass/.ssa -> bayt korumalı yükleme; diğerleri -> tek stilli yeni belge (path yok, Farklı kaydet ister).
    fps yalnız MicroDVD (.sub) için kullanılır."""
    low = path.lower()
    if low.endswith((".ass", ".ssa")):
        return AssDocument.load(path)
    with open(path, "rb") as f:
        data = f.read()
    if low.endswith((".ttml", ".dfxp", ".xml")) or data.lstrip(b"\xef\xbb\xbf \r\n\t").startswith(b"<"):
        from .ttml import parse_ttml
        evs = parse_ttml(data)
    else:
        text = decode_text(data)
        if low.endswith(".sbv"):
            evs = parse_sbv(text)
        elif low.endswith(".sub"):
            if not is_microdvd(text):
                raise ValueError("Bu .sub dosyası MicroDVD değil (VobSub resim altyazısı OCR gerektirir; "
                                 "desteklenmiyor)")
            evs = parse_microdvd(text, fps)
        elif low.endswith(".vtt") or text.lstrip().startswith("WEBVTT"):
            evs = parse_vtt(text)
        else:
            evs = parse_srt(text)
    doc = AssDocument.new()
    doc.ensure_section("events").items = evs or [Event()]
    doc.path = None
    doc.source_path = path
    return doc


def to_srt(events, skip_comments: bool = True) -> str:
    out = []
    n = 0
    for ev in events:
        if skip_comments and ev.comment:
            continue
        n += 1
        out.append(str(n))
        out.append(f"{format_time_srt(ev.start)} --> {format_time_srt(ev.end)}")
        out.append(ass_to_html(ev.text))
        out.append("")
    return "\r\n".join(out).replace("\n", "\r\n").replace("\r\r\n", "\r\n")


def format_time_srt(ms: int) -> str:
    ms = max(0, int(ms))
    h, rem = divmod(ms, 3600000)
    m, rem = divmod(rem, 60000)
    s, ms = divmod(rem, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def _dialogue(events):
    for ev in events:
        if ev.comment or re.search(r"\\p[1-9]", ev.text):
            continue
        yield ev


def to_vtt(events) -> str:
    """WebVTT: <i>/<b>/<u>; \\an7-9 → üstte (line:10%)."""
    out = ["WEBVTT", ""]
    n = 0
    for ev in _dialogue(events):
        body = ass_to_html(ev.text).strip("\n")
        if not body.strip():
            continue
        n += 1
        top = re.search(r"\\an[789]", ev.text)
        out.append(str(n))
        out.append(f"{format_time_srt(ev.start).replace(',', '.')} --> {format_time_srt(ev.end).replace(',', '.')}"
                   + (" line:10%" if top else ""))
        out.append(body)
        out.append("")
    return "\n".join(out)


def to_sbv(events) -> str:
    def t(ms):
        ms = max(0, int(ms))
        h, r = divmod(ms, 3600000)
        m, r = divmod(r, 60000)
        s, r = divmod(r, 1000)
        return f"{h}:{m:02d}:{s:02d}.{r:03d}"
    out = []
    for ev in _dialogue(events):
        body = re.sub(r"<[^>]+>", "", ass_to_html(ev.text)).strip("\n")
        if body.strip():
            out += [f"{t(ev.start)},{t(ev.end)}", body, ""]
    return "\n".join(out)


def to_microdvd(events, fps: float = DEFAULT_FPS) -> str:
    out = [f"{{1}}{{1}}{fps:.3f}".rstrip("0").rstrip(".")]
    for ev in _dialogue(events):
        body = re.sub(r"<[^>]+>", "", ass_to_html(ev.text)).strip("\n")
        if not body.strip():
            continue
        ital = re.search(r"\\i1", ev.text) and not re.search(r"\\i0", ev.text)
        a, b = int(round(ev.start * fps / 1000.0)), int(round(ev.end * fps / 1000.0))
        out.append(f"{{{a}}}{{{b}}}" + ("{y:i}" if ital else "") + body.replace("\n", "|"))
    return "\r\n".join(out) + "\r\n"


def to_text(events, with_times: bool = False) -> str:
    """Düz metin (okuma/çeviri kontrolü için): her altyazı bir paragraf."""
    out = []
    for ev in _dialogue(events):
        body = strip_tags(ev.text).replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ").strip()
        if not body:
            continue
        if with_times:
            out.append(f"[{format_time(ev.start)} → {format_time(ev.end)}]")
        out.append(body)
        out.append("")
    return "\n".join(out)


# Dışa aktarıcılar: anahtar -> (ad, uzantı, işlev(doc, **seçenekler) -> bytes)
def _enc(text: str, bom: bool = True, crlf: bool = True) -> bytes:
    if crlf:
        text = text.replace("\r\n", "\n").replace("\n", "\r\n")
    data = text.encode("utf-8")
    return (b"\xef\xbb\xbf" + data) if bom else data


def export_bytes(doc: AssDocument, key: str, fps: float | None = None, lang: str = "tr",
                 ttml_time: str = "clock") -> bytes:
    evs = doc.events
    if key == "ass":
        return doc.to_bytes()
    if key == "srt":
        return _enc(to_srt(evs), bom=True, crlf=False)
    if key == "vtt":
        return _enc(to_vtt(evs), bom=False, crlf=False)
    if key == "ttml":
        from .ttml import to_ttml
        title = doc.get_info("Title", "") or ""
        return to_ttml(doc, lang=lang, time_format=ttml_time, title=title).encode("utf-8")
    if key == "sbv":
        return _enc(to_sbv(evs), bom=False, crlf=False)
    if key == "sub":
        return _enc(to_microdvd(evs, fps or DEFAULT_FPS), bom=False, crlf=False)
    if key == "txt":
        return _enc(to_text(evs), bom=True)
    if key == "txt_times":
        return _enc(to_text(evs, with_times=True), bom=True)
    raise ValueError(key)


EXPORTERS = {
    "ass": ("ASS altyazı", ".ass"),
    "srt": ("SubRip (SRT)", ".srt"),
    "vtt": ("WebVTT", ".vtt"),
    "ttml": ("TTML / DFXP (Netflix, Amazon)", ".dfxp"),
    "sbv": ("YouTube SBV", ".sbv"),
    "sub": ("MicroDVD (kare tabanlı)", ".sub"),
    "txt": ("Düz metin", ".txt"),
    "txt_times": ("Düz metin + zamanlar", ".txt"),
}

IMPORT_EXT = (".ass", ".ssa", ".srt", ".vtt", ".ttml", ".dfxp", ".xml", ".sbv", ".sub")
IMPORT_FILTER = ("Altyazı (*.ass *.ssa *.srt *.vtt *.ttml *.dfxp *.xml *.sbv *.sub);;ASS (*.ass *.ssa);;"
                 "SRT / WebVTT (*.srt *.vtt);;TTML / DFXP (*.ttml *.dfxp *.xml);;YouTube SBV (*.sbv);;"
                 "MicroDVD (*.sub);;Tüm dosyalar (*)")


__all__ = ["load_any", "parse_srt", "parse_vtt", "parse_sbv", "parse_microdvd", "to_srt", "to_vtt", "to_sbv",
           "to_microdvd", "to_text", "export_bytes", "EXPORTERS", "IMPORT_EXT", "IMPORT_FILTER",
           "html_to_ass", "ass_to_html", "decode_text", "format_time", "format_time_srt"]
