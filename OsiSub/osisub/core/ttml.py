"""TTML / DFXP okuma-yazma (Netflix, Amazon, Crunchyroll dışa aktarımları; Subtitle Edit "Timed Text").

Okuma:
  * Ad alanından bağımsız (tt, head/styling/style, layout/region, body/div/p/span/br).
  * Zaman: saat (HH:MM:SS.mmm, HH:MM:SS:FF), göreli (12.5s, 1500ms, 30f, 123t); ttp:frameRate,
    ttp:frameRateMultiplier, ttp:tickRate (Netflix: 10 000 000) desteklenir; div/body begin'i eklenir.
  * Biçem: tts:fontStyle italic → {\\i1}, fontWeight bold → {\\b1}, textDecoration underline → {\\u1},
    tts:color → {\\c&HBBGGRR&}; style="…" başvuruları ve iç içe span'ler çözülür; <br/> → \\N.
  * Konum: bölge (ya da p) üstteyse (displayAlign="before" ya da origin y < %50) satır başına {\\an8}.

Yazma (DFXP/TTML1): tek "Default" biçimi; stilin ve satırın italik/kalın/altı çizili/renk değişimleri
<span> olur, \\an7-9 (ya da stil hizası) üst bölgeye gider, çizimli/boş satırlar atlanır.
"""

from __future__ import annotations

import html
import re
import xml.etree.ElementTree as ET

from .assfile import AssDocument, Event
from .asstime import round_ms
from .textutil import strip_tags

NS_TT = "http://www.w3.org/ns/ttml"
NS_TTS = "http://www.w3.org/ns/ttml#styling"
NS_TTP = "http://www.w3.org/ns/ttml#parameter"
NS_TTM = "http://www.w3.org/ns/ttml#metadata"
XML_NS = "http://www.w3.org/XML/1998/namespace"


def _local(tag) -> str:
    return tag.rsplit("}", 1)[-1] if isinstance(tag, str) else ""


def _attr(el, name: str, default=None):
    """Ad alanından bağımsız öznitelik (tts:color, xml:id, ttp:tickRate...)."""
    for k, v in el.attrib.items():
        if _local(k) == name:
            return v
    return default


# ---------------------------------------------------------------------------
# Zaman
# ---------------------------------------------------------------------------

class TimeBase:
    def __init__(self, root):
        fr = float(_attr(root, "frameRate", "30") or 30)
        mult = _attr(root, "frameRateMultiplier")
        if mult:
            try:
                a, b = (float(x) for x in mult.split())
                fr = fr * a / b
            except ValueError:
                pass
        self.frame_rate = fr
        self.sub_frame_rate = float(_attr(root, "subFrameRate", "1") or 1)
        tr = _attr(root, "tickRate")
        self.tick_rate = float(tr) if tr else (fr * self.sub_frame_rate if _attr(root, "frameRate") else 1.0)

    def ms(self, s: str | None) -> int | None:
        if s is None:
            return None
        s = s.strip()
        m = re.fullmatch(r"(\d+):(\d{2}):(\d{2})(?:\.(\d+))?", s)
        if m:
            h, mi, se, frac = m.groups()
            v = (int(h) * 3600 + int(mi) * 60 + int(se)) * 1000.0
            if frac:
                v += float("0." + frac) * 1000
            return int(round(v))
        m = re.fullmatch(r"(\d+):(\d{2}):(\d{2}):(\d+)(?:\.(\d+))?", s)
        if m:
            h, mi, se, fr, sub = m.groups()
            v = (int(h) * 3600 + int(mi) * 60 + int(se)) * 1000.0
            v += (int(fr) + (int(sub) / self.sub_frame_rate if sub else 0)) * 1000.0 / self.frame_rate
            return int(round(v))
        m = re.fullmatch(r"(\d+(?:\.\d+)?)(h|ms|m|s|f|t)", s)
        if m:
            n, unit = float(m.group(1)), m.group(2)
            mul = {"h": 3600000.0, "m": 60000.0, "s": 1000.0, "ms": 1.0,
                   "f": 1000.0 / self.frame_rate, "t": 1000.0 / self.tick_rate}[unit]
            return int(round(n * mul))
        raise ValueError(f"TTML zamanı anlaşılamadı: {s}")


# ---------------------------------------------------------------------------
# Okuma
# ---------------------------------------------------------------------------

def _style_props(el, styles: dict) -> dict:
    """Bir öğenin kendi biçemleri (style başvuruları + doğrudan tts: öznitelikleri)."""
    props = {}
    for sid in (_attr(el, "style") or "").split():
        props.update(styles.get(sid, {}))
    for k, v in el.attrib.items():
        ns = k[1:].split("}")[0] if k.startswith("{") else ""
        if ns == NS_TTS or k.startswith("tts:"):
            props[_local(k)] = v
    return props


def _norm_props(p: dict) -> dict:
    out = {}
    if "fontStyle" in p:
        out["i"] = p["fontStyle"].strip().lower() in ("italic", "oblique")
    if "fontWeight" in p:
        out["b"] = p["fontWeight"].strip().lower() == "bold"
    if "textDecoration" in p:
        out["u"] = "underline" in p["textDecoration"].lower() and "nounderline" not in p["textDecoration"].lower()
    if "color" in p:
        c = _parse_color(p["color"])
        if c:
            out["c"] = None if c == "FFFFFF" else c      # beyaz = stilin varsayılanı (etiket yazılmaz)
    return out


_NAMED = {"white": "FFFFFF", "black": "000000", "red": "FF0000", "green": "00FF00", "lime": "00FF00",
          "blue": "0000FF", "yellow": "FFFF00", "cyan": "00FFFF", "aqua": "00FFFF", "magenta": "FF00FF",
          "fuchsia": "FF00FF", "gray": "808080", "grey": "808080", "silver": "C0C0C0", "orange": "FFA500"}


def _parse_color(v: str) -> str | None:
    """'#RRGGBB', '#RRGGBBAA', 'rgb(r,g,b)', 'rgba(...)', ad → RRGGBB."""
    v = v.strip().lower()
    if v in _NAMED:
        return _NAMED[v]
    m = re.fullmatch(r"#([0-9a-f]{6})(?:[0-9a-f]{2})?", v)
    if m:
        return m.group(1).upper()
    m = re.fullmatch(r"rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)", v.split(")")[0] + "")
    if m:
        return "".join(f"{min(255, int(x)):02X}" for x in m.groups())
    return None


def _ass_color(rrggbb: str) -> str:
    return f"&H{rrggbb[4:6]}{rrggbb[2:4]}{rrggbb[0:2]}&"


def _diff_tags(cur: dict, new: dict) -> str:
    out = []
    for k in ("i", "b", "u"):
        if bool(new.get(k)) != bool(cur.get(k)):
            out.append(f"\\{k}{1 if new.get(k) else 0}")
    if new.get("c") != cur.get("c"):
        out.append(f"\\c{_ass_color(new['c'])}" if new.get("c") else "\\c")
    return "{" + "".join(out) + "}" if out else ""


def _text(s: str) -> str:
    s = re.sub(r"\s+", " ", s)
    return s.replace("{", "(").replace("}", ")")


def _walk(el, cur: dict, styles: dict) -> str:
    out = [_text(el.text) if el.text else ""]
    for ch in el:
        tag = _local(ch.tag)
        if tag == "br":
            out.append("\\N")
        elif tag in ("span", "p", "div"):
            new = dict(cur)
            new.update(_norm_props(_style_props(ch, styles)))
            out.append(_diff_tags(cur, new) + _walk(ch, new, styles) + _diff_tags(new, cur))
        elif tag not in ("metadata", "set"):
            out.append(_walk(ch, cur, styles))
        if ch.tail:
            out.append(_text(ch.tail))
    return "".join(out)


def _clean(t: str) -> str:
    t = t.replace("{}", "")
    t = re.sub(r" *\\N *", r"\\N", t)
    t = t.replace("}{", "")
    t = t.strip()
    return re.sub(r"\{(?:\\[ibu]0|\\c)+\}$", "", t).rstrip()      # sondaki yalnız-kapatan blok gereksiz


def _is_top(el, regions: dict) -> bool:
    for src in (el, regions.get(_attr(el, "region") or "")):
        if src is None:
            continue
        da = (_attr(src, "displayAlign") or "").lower()
        if da == "before":
            return True
        if da == "after":
            return False
        org = _attr(src, "origin")
        if org:
            m = re.match(r"\s*[-\d.]+%?\s+([-\d.]+)%", org)
            if m:
                return float(m.group(1)) < 40.0
    return False


def parse_ttml(data: bytes | str) -> list[Event]:
    if isinstance(data, str):
        data = data.encode("utf-8")
    data = data.lstrip(b"\xef\xbb\xbf")
    root = ET.fromstring(data)
    if _local(root.tag) != "tt":
        raise ValueError("TTML değil (kök öğe <tt> olmalı)")
    tb = TimeBase(root)
    styles: dict[str, dict] = {}
    regions: dict[str, ET.Element] = {}
    for el in root.iter():
        tag = _local(el.tag)
        if tag == "style" and _attr(el, "id"):
            styles[_attr(el, "id")] = _style_props(el, styles)
        elif tag == "region" and _attr(el, "id"):
            regions[_attr(el, "id")] = el
    body = next((el for el in root.iter() if _local(el.tag) == "body"), None)
    if body is None:
        return []
    events: list[Event] = []

    def visit(el, offset: int, inherited: dict, region_el):
        tag = _local(el.tag)
        begin = tb.ms(_attr(el, "begin"))
        if tag == "p":
            start = offset + (begin or 0)
            end_v = tb.ms(_attr(el, "end"))
            dur = tb.ms(_attr(el, "dur"))
            end = offset + end_v if end_v is not None else (start + dur if dur is not None else start + 2000)
            props = dict(inherited)
            props.update(_norm_props(_style_props(el, styles)))
            text = _walk(el, props, styles)
            lead = _diff_tags({}, props)
            text = _clean(lead + text)
            if _is_top(el, regions) or (region_el is not None and _attr(el, "region") is None
                                        and _is_top(region_el, regions)):
                text = "{\\an8}" + text if not text.startswith("{") else "{\\an8" + text[1:]
            if strip_tags(text).replace("\\N", "").strip():
                events.append(Event(start=round_ms(start), end=round_ms(max(start, end)), text=text))
            return
        props = dict(inherited)
        props.update(_norm_props(_style_props(el, styles)))
        reg = el if _attr(el, "region") else region_el
        for ch in el:
            visit(ch, offset + (begin or 0), props, reg)

    visit(body, 0, {}, None)
    events.sort(key=lambda e: (e.start, e.end))
    return events


# ---------------------------------------------------------------------------
# Yazma
# ---------------------------------------------------------------------------

_BLOCK = re.compile(r"\{([^}]*)\}")


def _runs(text: str, base: dict):
    """ASS metnini (metin, durum) koşularına ayırır. Durum: i/b/u/c; \\N → '\\n' karakteri."""
    state = dict(base)
    runs = []
    pos = 0
    drawing = False
    for m in _BLOCK.finditer(text + "{}"):
        seg = text[pos:m.start()] if m.start() <= len(text) else ""
        if seg and not drawing:
            seg = seg.replace("\\N", "\n").replace("\\n", " ").replace("\\h", "\u00a0")
            runs.append((seg, dict(state)))
        inner = m.group(1)
        for tag, val in re.findall(r"\\(i|b|u)(\d*)", inner):
            if val != "":
                state[tag] = val not in ("0",)
        for col in re.findall(r"\\1?c(&H[0-9A-Fa-f]+&?)?(?=\\|$)", inner):
            if col:
                h = col.strip("&Hh").rjust(6, "0")[-6:]
                state["c"] = (h[4:6] + h[2:4] + h[0:2]).upper()
            else:
                state["c"] = base.get("c")
        for d in re.findall(r"\\p(\d+)", inner):
            drawing = int(d) > 0
        pos = m.end()
    return runs


def _span(seg: str, st: dict) -> str:
    body = "<br/>".join(html.escape(x, quote=False) for x in seg.split("\n"))
    attrs = []
    if st.get("i"):
        attrs.append('tts:fontStyle="italic"')
    if st.get("b"):
        attrs.append('tts:fontWeight="bold"')
    if st.get("u"):
        attrs.append('tts:textDecoration="underline"')
    if st.get("c") and st["c"] != "FFFFFF":
        attrs.append(f'tts:color="#{st["c"]}"')
    return f"<span {' '.join(attrs)}>{body}</span>" if attrs else body


def _fmt_clock(ms: int) -> str:
    ms = max(0, int(ms))
    h, r = divmod(ms, 3600000)
    m, r = divmod(r, 60000)
    s, r = divmod(r, 1000)
    return f"{h:02d}:{m:02d}:{s:02d}.{r:03d}"


def to_ttml(doc: AssDocument, lang: str = "tr", time_format: str = "clock", tick_rate: int = 10_000_000,
            title: str = "") -> str:
    styles = {s.name: s for s in doc.styles}
    body = []
    for ev in doc.events:
        if ev.comment or re.search(r"\\p[1-9]", ev.text):
            continue
        st = styles.get(ev.style)
        base = {"i": bool(st and st.italic), "b": bool(st and st.bold), "u": bool(st and st.underline)}
        runs = _runs(ev.text, base)
        content = "".join(_span(seg, s) for seg, s in _merge(runs)).strip()
        content = re.sub(r"\s*<br/>\s*", "<br/>", content)
        if not re.sub(r"<[^>]+>", "", content).strip():
            continue
        an = re.findall(r"\\an(\d)", ev.text)
        align = int(an[0]) if an else (st.align if st else 2)
        region = "top" if align in (7, 8, 9) else "bottom"
        if time_format == "ticks":
            b, e = f"{ev.start * tick_rate // 1000}t", f"{ev.end * tick_rate // 1000}t"
        else:
            b, e = _fmt_clock(ev.start), _fmt_clock(ev.end)
        body.append(f'      <p begin="{b}" end="{e}" region="{region}">{content}</p>')
    tick = f' ttp:tickRate="{tick_rate}"' if time_format == "ticks" else ""
    head_title = f"\n    <metadata><ttm:title>{html.escape(title, quote=False)}</ttm:title></metadata>" if title else ""
    return (f'<?xml version="1.0" encoding="utf-8"?>\n'
            f'<tt xmlns="{NS_TT}" xmlns:tts="{NS_TTS}" xmlns:ttp="{NS_TTP}" xmlns:ttm="{NS_TTM}" '
            f'xml:lang="{html.escape(lang)}" ttp:timeBase="media"{tick}>\n'
            f'  <head>{head_title}\n'
            '    <styling>\n'
            '      <style xml:id="s0" tts:fontFamily="proportionalSansSerif" tts:fontSize="100%" '
            'tts:color="white" tts:textAlign="center"/>\n'
            '    </styling>\n'
            '    <layout>\n'
            '      <region xml:id="bottom" tts:origin="10% 10%" tts:extent="80% 80%" tts:displayAlign="after" '
            'tts:textAlign="center"/>\n'
            '      <region xml:id="top" tts:origin="10% 10%" tts:extent="80% 80%" tts:displayAlign="before" '
            'tts:textAlign="center"/>\n'
            '    </layout>\n'
            '  </head>\n'
            '  <body style="s0">\n    <div>\n' + "\n".join(body) + '\n    </div>\n  </body>\n</tt>\n')


def _merge(runs):
    out = []
    for seg, st in runs:
        if out and out[-1][1] == st:
            out[-1] = (out[-1][0] + seg, st)
        else:
            out.append((seg, st))
    return out


__all__ = ["parse_ttml", "to_ttml", "TimeBase"]
