"""Aegisub'ın OsiSub'da eksik kalan araçları (arayüzsüz çekirdek).

  select_rows          Satır seç (Edit > Select Lines): alan + içerir/tam/regex, ters, diyalog/yorum,
                       seçimi ayarla/ekle/çıkar/kesiştir
  resample_doc         Çözünürlüğü yeniden örnekle (720p → 1080p): stiller, kenar boşlukları, \\pos \\move
                       \\org \\clip \\iclip (dikdörtgen ve vektör), \\fs \\fsp \\bord \\shad \\blur \\pbo (\\t içinde de),
                       \\p çizimleri; PlayResX/Y (ve varsa LayoutResX/Y)
  split_at             imleçten böl: zamanı tahmin et / koru / video karesinden
  make_continuous      zamanları sürekli yap (başlangıç ya da bitiş değişir)
  duplicate_shift      1 kare kaydırarak çoğalt (kare kare tipografi)
  paste_over           panodaki satırları seçili satırların üstüne yapıştır (seçilen alanlar)
  load_keyframes / save_keyframes   Aegisub v1, XviD 2-geçiş, x264 stats, düz kare listesi
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from .asstime import round_ms
from .assfile import Event, fmt_num
from .framemap import END, START

# ---------------------------------------------------------------------------
# Satır seç
# ---------------------------------------------------------------------------

SELECT_FIELDS = {"text": "Metin", "style": "Stil", "actor": "Aktör", "effect": "Efekt"}
SELECT_MODES = {"contains": "İçerir", "exact": "Tam eşleşme", "regex": "Düzenli ifade"}
SELECT_ACTIONS = {"set": "Seçimi bununla değiştir", "add": "Seçime ekle", "sub": "Seçimden çıkar",
                  "and": "Seçimle kesiştir"}


@dataclass
class SelectOptions:
    pattern: str = ""
    field: str = "text"
    mode: str = "contains"
    case: bool = False
    invert: bool = False
    dialogue: bool = True
    comments: bool = False
    action: str = "set"


def select_rows(events, current: set[int], o: SelectOptions) -> set[int]:
    if o.mode == "regex":
        rx = re.compile(o.pattern, 0 if o.case else re.IGNORECASE)
        test = rx.search
    else:
        pat = o.pattern if o.case else o.pattern.casefold()
        if o.mode == "exact":
            def test(s):
                return s == pat
        else:
            def test(s):
                return pat in s
    hits = set()
    for i, ev in enumerate(events):
        if (ev.comment and not o.comments) or (not ev.comment and not o.dialogue):
            continue
        val = getattr(ev, o.field)
        if o.mode != "regex" and not o.case:
            val = val.casefold()
        ok = bool(test(val))
        if ok != o.invert:
            hits.add(i)
    if o.action == "add":
        return set(current) | hits
    if o.action == "sub":
        return set(current) - hits
    if o.action == "and":
        return set(current) & hits
    return hits


# ---------------------------------------------------------------------------
# Çözünürlük yeniden örnekleme
# ---------------------------------------------------------------------------

def _num(v: float) -> str:
    return fmt_num(round(v, 3))


def scale_drawing(cmds: str, rx: float, ry: float) -> str:
    """Vektör çizim komutlarındaki koordinat çiftlerini ölçekler (m/n/l/b/s/p/c harfleri korunur)."""
    out = []
    axis = 0
    for tok in re.findall(r"[a-zA-Z]|-?\d+(?:\.\d+)?|\S", cmds):
        if re.fullmatch(r"-?\d+(?:\.\d+)?", tok):
            out.append(_num(float(tok) * (rx if axis == 0 else ry)))
            axis ^= 1
        else:
            out.append(tok)
            if tok.isalpha():
                axis = 0
    return " ".join(out)


def _scale_args(args: str, kinds: str, rx: float, ry: float) -> str:
    vals = [a.strip() for a in args.split(",")]
    out = []
    for i, v in enumerate(vals):
        k = kinds[i] if i < len(kinds) else "-"
        try:
            f = float(v)
        except ValueError:
            out.append(v)
            continue
        out.append(_num(f * rx) if k == "x" else _num(f * ry) if k == "y" else v)
    return ",".join(out)


_SIZE_TAGS = {"fs": "y", "fsp": "x", "bord": "y", "xbord": "x", "ybord": "y", "shad": "y", "xshad": "x",
              "yshad": "y", "blur": "y", "pbo": "y"}
_TAG_RX = re.compile(r"\\(i?clip|move|pos|org|xbord|ybord|bord|xshad|yshad|shad|blur|pbo|fsp|fs(?![a-z]))"
                     r"\s*(\([^()]*\)|-?\d+(?:\.\d+)?)")


def scale_block(inner: str, rx: float, ry: float, sizes: bool = True) -> str:
    def rep(m):
        name, arg = m.group(1), m.group(2)
        if arg.startswith("("):
            body = arg[1:-1]
            if name in ("pos", "org"):
                return f"\\{name}({_scale_args(body, 'xy', rx, ry)})"
            if name == "move":
                return f"\\{name}({_scale_args(body, 'xyxy--', rx, ry)})"
            if name.endswith("clip"):
                parts = [p.strip() for p in body.split(",")]
                if len(parts) == 4 and all(re.fullmatch(r"-?\d+(?:\.\d+)?", p) for p in parts):
                    return f"\\{name}({_scale_args(body, 'xyxy', rx, ry)})"
                if len(parts) == 2 and re.fullmatch(r"\d+", parts[0]):        # \clip(ölçek, çizim)
                    return f"\\{name}({parts[0]},{scale_drawing(parts[1], rx, ry)})"
                return f"\\{name}({scale_drawing(body, rx, ry)})"
            return m.group(0)
        if not sizes or name not in _SIZE_TAGS:
            return m.group(0)
        k = _SIZE_TAGS[name]
        return f"\\{name}{_num(float(arg) * (rx if k == 'x' else ry))}"
    return _TAG_RX.sub(rep, inner)


def scale_text(text: str, rx: float, ry: float, sizes: bool = True, drawings: bool = True) -> str:
    out = []
    pos = 0
    drawing = False
    for m in re.finditer(r"\{([^}]*)\}", text):
        seg = text[pos:m.start()]
        out.append(scale_drawing(seg, rx, ry) if (drawing and drawings and seg.strip()) else seg)
        inner = scale_block(m.group(1), rx, ry, sizes)
        out.append("{" + inner + "}")
        for d in re.finditer(r"\\p(\d+)", m.group(1)):
            drawing = int(d.group(1)) > 0
        pos = m.end()
    seg = text[pos:]
    out.append(scale_drawing(seg, rx, ry) if (drawing and drawings and seg.strip()) else seg)
    return "".join(out)


@dataclass
class ResampleOptions:
    dst_w: int = 1920
    dst_h: int = 1080
    margins: bool = True
    sizes: bool = True           # yazı boyu, kenarlık, gölge, bulanıklık
    drawings: bool = True


def resample_doc(doc, o: ResampleOptions) -> tuple[list, list, float, float]:
    """Döndürür: (yeni stiller, yeni olaylar, rx, ry). PlayRes'i çağıran yazar (tek geri al adımı)."""
    sw, sh = doc.play_res()
    rx, ry = o.dst_w / sw, o.dst_h / sh
    styles = []
    for st in doc.styles:
        kw = {}
        if o.sizes:
            kw.update(fontsize=round(st.fontsize * ry, 3), outline=round(st.outline * ry, 3),
                      shadow=round(st.shadow * ry, 3), spacing=round(st.spacing * rx, 3))
        if o.margins:
            kw.update(margin_l=int(round(st.margin_l * rx)), margin_r=int(round(st.margin_r * rx)),
                      margin_v=int(round(st.margin_v * ry)))
        styles.append(st.replace(**kw) if kw else st)
    events = []
    for ev in doc.events:
        kw = {}
        t = scale_text(ev.text, rx, ry, o.sizes, o.drawings)
        if t != ev.text:
            kw["text"] = t
        if o.margins:
            m = {"margin_l": int(round(ev.margin_l * rx)), "margin_r": int(round(ev.margin_r * rx)),
                 "margin_v": int(round(ev.margin_v * ry)), "margin_b": int(round(ev.margin_b * ry))}
            kw.update({k: v for k, v in m.items() if v != getattr(ev, k)})
        events.append(ev.replace(**kw) if kw else ev)
    return styles, events, rx, ry


# ---------------------------------------------------------------------------
# Satır işlemleri
# ---------------------------------------------------------------------------

def split_at(ev: Event, pos: int, mode: str = "estimate", split_ms: int | None = None,
             frame_map=None) -> tuple[Event, Event] | None:
    """Metni ham konum `pos`'tan böler. mode: estimate (zaman karakter oranıyla), preserve (iki parça da
    aynı zaman), video (split_ms anında). İkinci parçaya baştaki etiket bloğu kopyalanır."""
    text = ev.text
    pos = max(0, min(len(text), pos))
    # imleç bir etiket bloğunun içindeyse bloğun sonuna taşı
    for m in re.finditer(r"\{[^}]*\}", text):
        if m.start() < pos < m.end():
            pos = m.end()
    a, b = text[:pos], text[pos:]
    if a.endswith("\\N"):
        a = a[:-2]
    if b.startswith("\\N"):
        b = b[2:]
    a, b = a.rstrip(" "), b.lstrip(" ")
    if not b.strip() and not a.strip():
        return None
    lead = re.match(r"^\{[^}]*\}", text)
    if lead and pos >= lead.end() and not b.startswith("{"):
        b = lead.group(0) + b
    if mode == "preserve":
        mid1 = mid2 = None
    elif mode == "video" and split_ms is not None:
        mid1 = mid2 = split_ms
    else:
        from .textutil import char_count, strip_tags
        la, lb = char_count(strip_tags(a)), char_count(strip_tags(b))
        mid1 = ev.start + (ev.end - ev.start) * la // max(1, la + lb)
        if frame_map is not None:
            mid1 = frame_map.snap(mid1)
        mid2 = mid1
    e1 = ev.replace(text=a) if mid1 is None else ev.replace(text=a, end=max(ev.start, mid1))
    e2 = ev.fresh().replace(text=b) if mid2 is None else ev.fresh().replace(text=b, start=min(ev.end, mid2))
    return e1, e2


def make_continuous(events, rows, change: str = "start") -> dict[int, dict]:
    """Seçili satırların ardışık çiftlerinde: change=start → sonraki başlangıç = önceki bitiş;
    change=end → önceki bitiş = sonraki başlangıç (Aegisub: Make times continuous)."""
    rows = sorted(rows)
    out: dict[int, dict] = {}
    for a, b in zip(rows, rows[1:]):
        ea, eb = events[a], events[b]
        if change == "start":
            s = ea.end
            if s != eb.start:
                out.setdefault(b, {})["start"] = s
                if eb.end < s:
                    out[b]["end"] = s
        else:
            e = eb.start
            if e != ea.end:
                out.setdefault(a, {})["end"] = max(ea.start, e)
    return out


def duplicate_shift(events, rows, frame_map) -> list[tuple[int, Event]]:
    """Her seçili satırın 1 kare sonraya kaydırılmış kopyası: [(eklenecek konum, olay)] (sondan başa)."""
    out = []
    for r in sorted(rows, reverse=True):
        ev = events[r]
        f0 = frame_map.frame_at_time(ev.start, START)
        f1 = frame_map.frame_at_time(ev.end, END)
        dup = ev.fresh().replace(start=frame_map.time_at_frame(f0 + 1, START),
                                 end=frame_map.time_at_frame(f1 + 1, END))
        out.append((r + 1, dup))
    return out


PASTE_FIELDS = {"text": "Metin", "start": "Başlangıç", "end": "Bitiş", "style": "Stil", "actor": "Aktör",
                "effect": "Efekt", "layer": "Katman", "margin_l": "Sol kenar", "margin_r": "Sağ kenar",
                "margin_v": "Dikey kenar", "comment": "Yorum"}


def parse_clipboard_lines(text: str, fmt: list[str]) -> list[Event]:
    """Pano metni: Dialogue:/Comment: satırları ya da düz metin satırları (her satır bir altyazı)."""
    out = []
    for ln in text.replace("\r\n", "\n").split("\n"):
        low = ln.lower()
        if low.startswith(("dialogue:", "comment:")):
            out.append(Event.parse(ln, fmt).fresh())
        elif ln.strip():
            out.append(Event(text=ln))
    return out


def paste_over(events, rows, pasted: list[Event], fields: set[str]) -> dict[int, dict]:
    """Yapıştırılan satırları seçili satırların (sırayla) üstüne, yalnız seçilen alanlarla yazar."""
    out = {}
    for r, src in zip(sorted(rows), pasted):
        kw = {f: getattr(src, f) for f in fields}
        if "start" in kw or "end" in kw:
            s = kw.get("start", events[r].start)
            e = kw.get("end", events[r].end)
            kw["end"] = max(s, e)
        out[r] = kw
    return out


# ---------------------------------------------------------------------------
# Anahtar kare dosyaları
# ---------------------------------------------------------------------------

def load_keyframes(text: str) -> list[int]:
    lines = text.replace("\r\n", "\n").split("\n")
    head = next((ln.strip() for ln in lines if ln.strip()), "")
    kfs = []
    if head.lower().startswith("# keyframe format v1"):
        for ln in lines[1:]:
            ln = ln.strip()
            if ln.isdigit():
                kfs.append(int(ln))
        return sorted(set(kfs))
    if head.lower().startswith("# xvid 2pass stat file"):
        n = 0
        for ln in lines:
            s = ln.strip()
            if not s or s.startswith("#"):
                continue
            if s[0] in "ipbsIPBS":
                if s[0] in "iI":
                    kfs.append(n)
                n += 1
        return kfs
    if re.search(r"^#options:", text, re.M) or re.search(r"\bin:\d+ out:\d+ type:", text):
        for m in re.finditer(r"\bin:(\d+) out:\d+ type:([IiPpBb])", text):
            if m.group(2) in "Ii":
                kfs.append(int(m.group(1)))
        return sorted(set(kfs))
    for ln in lines:                       # düz liste (WWXD/SCXvid çıktıları, elle yazılmış)
        ln = ln.strip()
        if not ln or ln.startswith("#") or ln.lower().startswith("fps"):
            continue
        if not ln.isdigit():
            raise ValueError(f"Anahtar kare dosyası tanınmadı: '{ln[:40]}'")
        kfs.append(int(ln))
    return sorted(set(kfs))


def save_keyframes(kfs, fps: float = 0.0) -> str:
    """Aegisub anahtar kare biçimi v1."""
    return "# keyframe format v1\nfps " + (fmt_num(round(fps, 6)) if fps else "0") + "\n" + \
        "".join(f"{k}\n" for k in sorted(set(kfs)))


def snap_ms(ms: int, frame_map=None) -> int:
    return round_ms(frame_map.snap(ms)) if frame_map is not None else round_ms(ms)
