"""Editing tools: timing, text, QC. All functions operate on Document in-place unless noted."""
from __future__ import annotations
import re
from dataclasses import dataclass
from typing import Callable, Iterable, Optional

from .ass import Document, Event
from .tags import strip_tags, char_count, has_drawing, BLOCK
from .timeutil import FrameRate


# ------------------------------------------------------------------ Turkish case
_TR_UP = str.maketrans({"i": "İ", "ı": "I"})
_TR_LOW = str.maketrans({"İ": "i", "I": "ı"})


def tr_upper(s: str) -> str:
    return s.translate(_TR_UP).upper()


def tr_lower(s: str) -> str:
    return s.translate(_TR_LOW).lower()


def tr_title(s: str) -> str:
    return re.sub(r"(\w)(\w*)", lambda m: tr_upper(m.group(1)) + tr_lower(m.group(2)), s)


def tr_sentence(s: str) -> str:
    s = tr_lower(s)
    return re.sub(r"(^|[.!?…]\s+|\\N)(\w)", lambda m: m.group(1) + tr_upper(m.group(2)), s)


def apply_case(text: str, mode: str, turkish: bool = True) -> str:
    """Change case of visible text only (tags and \\N untouched)."""
    up, low, title, sent = ((tr_upper, tr_lower, tr_title, tr_sentence) if turkish
                            else (str.upper, str.lower, str.title, lambda x: x.lower().capitalize()))
    fn = {"upper": up, "lower": low, "title": title, "sentence": sent}[mode]
    out, pos = [], 0
    for m in re.finditer(r"\{[^}]*\}|\\[Nnh]", text):
        out.append(fn(text[pos:m.start()]))
        out.append(m.group(0))
        pos = m.end()
    out.append(fn(text[pos:]))
    return "".join(out)


# ------------------------------------------------------------------ selection helpers
def _sel(doc: Document, sel: Optional[Iterable[int]]) -> list[int]:
    return sorted(set(sel)) if sel is not None else list(range(len(doc.events)))


# ------------------------------------------------------------------ timing
def shift_times(doc: Document, ms: int, sel=None, start=True, end=True) -> None:
    for i in _sel(doc, sel):
        e = doc.events[i]
        if start: e.start = max(0, e.start + ms)
        if end: e.end = max(0, e.end + ms)


def shift_frames(doc: Document, fr: FrameRate, frames: int, sel=None) -> None:
    for i in _sel(doc, sel):
        e = doc.events[i]
        e.start = fr.start_ms(max(0, fr.frame_at(e.start) + frames))
        e.end = fr.end_ms(max(0, fr.frame_at(e.end) + frames - 1)) if e.end else 0


def snap_to_frames(doc: Document, fr: FrameRate, sel=None) -> None:
    for i in _sel(doc, sel):
        e = doc.events[i]
        e.start = fr.snap_start(e.start)
        e.end = max(e.start, fr.snap_end(e.end))


def snap_to_keyframes(doc: Document, keyframes_ms: list[int], start_threshold=250, end_threshold=250,
                      sel=None) -> int:
    """Snap start to previous keyframe (within threshold) and end to next keyframe. Returns #changed."""
    import bisect
    kf = sorted(keyframes_ms)
    changed = 0
    for i in _sel(doc, sel):
        e = doc.events[i]
        ns, ne = e.start, e.end
        j = bisect.bisect_right(kf, e.start) - 1
        if j >= 0 and 0 <= e.start - kf[j] <= start_threshold: ns = kf[j]
        j = bisect.bisect_left(kf, e.end)
        if j < len(kf) and 0 <= kf[j] - e.end <= end_threshold: ne = kf[j]
        if (ns, ne) != (e.start, e.end):
            e.start, e.end = ns, ne; changed += 1
    return changed


def sort_events(doc: Document, key: str = "start") -> None:
    keys = {"start": lambda e: (e.start, e.end), "end": lambda e: (e.end, e.start),
            "style": lambda e: (e.style, e.start), "actor": lambda e: (e.actor, e.start),
            "layer": lambda e: (e.layer, e.start)}
    doc.events.sort(key=keys[key])


def chain_gaps(doc: Document, max_gap: int = 500, min_gap: int = 0, sel=None, fr: Optional[FrameRate] = None) -> int:
    """Extend each end to next start if gap < max_gap ("chaining"); enforce min_gap otherwise."""
    idx = [i for i in _sel(doc, sel) if not doc.events[i].comment]
    idx.sort(key=lambda i: doc.events[i].start)
    n = 0
    for a, b in zip(idx, idx[1:]):
        x, y = doc.events[a], doc.events[b]
        gap = y.start - x.end
        if 0 < gap <= max_gap:
            x.end = y.start - min_gap; n += 1
        elif -2000 < gap < min_gap and min_gap:
            x.end = max(x.start + 1, y.start - min_gap); n += 1
    return n


def fix_overlaps(doc: Document, sel=None, min_gap: int = 0) -> int:
    idx = [i for i in _sel(doc, sel) if not doc.events[i].comment]
    idx.sort(key=lambda i: doc.events[i].start)
    n = 0
    for a, b in zip(idx, idx[1:]):
        x, y = doc.events[a], doc.events[b]
        if (x.layer, x.style) == (y.layer, y.style) or True:
            if x.end > y.start - min_gap and x.start < y.start:
                x.end = max(x.start + 1, y.start - min_gap); n += 1
    return n


def enforce_duration(doc: Document, min_ms=1000, max_ms=7000, cps: float = 0, sel=None,
                     next_limit: bool = True) -> int:
    """Lengthen short lines (without overlapping the next line) and optionally by CPS."""
    n = 0
    idx = sorted((i for i in _sel(doc, sel) if not doc.events[i].comment), key=lambda i: doc.events[i].start)
    for k, i in enumerate(idx):
        e = doc.events[i]
        need = min_ms
        if cps:
            need = max(need, int(char_count(e.text) / cps * 1000))
        need = min(need, max_ms)
        if e.duration < need:
            new_end = e.start + need
            if next_limit and k + 1 < len(idx):
                new_end = min(new_end, max(e.end, doc.events[idx[k + 1]].start))
            if new_end > e.end:
                e.end = new_end; n += 1
    return n


def split_at(doc: Document, index: int, char_pos: int) -> None:
    """Split event at visible character position; time proportional to text length."""
    e = doc.events[index]
    vis = strip_tags(e.text)
    if not 0 < char_pos < len(vis):
        return
    # map visible position -> raw position
    raw, seen = 0, 0
    while raw < len(e.text) and seen < char_pos:
        m = BLOCK.match(e.text, raw)
        if m:
            raw = m.end(); continue
        raw += 1; seen += 1
    a, b = e.text[:raw].rstrip(), e.text[raw:].lstrip()
    mid = e.start + int(e.duration * (len(strip_tags(a)) / max(1, len(vis))))
    n = e.copy()
    e.text, e.end = a, mid
    n.text, n.start = b, mid
    doc.events.insert(index + 1, n)


def merge_events(doc: Document, indices: list[int], sep: str = "\\N") -> int:
    idx = sorted(indices)
    if len(idx) < 2:
        return idx[0] if idx else 0
    first = doc.events[idx[0]]
    first.text = sep.join(doc.events[i].text for i in idx)
    first.end = max(doc.events[i].end for i in idx)
    for i in reversed(idx[1:]):
        del doc.events[i]
    return idx[0]


def duplicate_events(doc: Document, indices: list[int]) -> list[int]:
    out = []
    for off, i in enumerate(sorted(indices)):
        doc.events.insert(i + off + 1, doc.events[i + off].copy())
        out.append(i + off + 1)
    return out


# ------------------------------------------------------------------ text
def find_replace(doc: Document, find: str, repl: str, regex=False, case=False, whole_word=False,
                 field: str = "text", sel=None, turkish_case: bool = True) -> int:
    flags = 0 if case else re.IGNORECASE
    pat = find if regex else re.escape(find)
    if whole_word:
        pat = r"(?<!\w)" + pat + r"(?!\w)"
    if not case and turkish_case and not regex:
        # make i/İ/ı/I mutually tolerant when matching case-insensitively
        def fold(m):
            ch = m.group(0)
            return {"i": "[iİ]", "İ": "[iİ]", "ı": "[ıI]", "I": "[ıI]"}.get(ch, ch)
        pat = re.sub(r"[iİıI]", fold, pat)
    rx = re.compile(pat, flags)
    n = 0
    for i in _sel(doc, sel):
        e = doc.events[i]
        old = getattr(e, field)
        new, c = rx.subn(repl if regex else repl.replace("\\", "\\\\"), old)
        if c:
            setattr(e, field, new); n += c
    return n


def strip_all_tags(doc: Document, sel=None) -> None:
    for i in _sel(doc, sel):
        doc.events[i].text = strip_tags(doc.events[i].text)


def normalize_text(doc: Document, sel=None) -> int:
    """Typographic cleanup: multiple spaces, space before punctuation, '...' -> '…' (optional kept), trim."""
    n = 0
    for i in _sel(doc, sel):
        e = doc.events[i]
        t = e.text
        t = re.sub(r" {2,}", " ", t)
        t = re.sub(r"\s+(\\N)", r"\1", t)
        t = re.sub(r"(\\N)\s+", r"\1", t)
        t = re.sub(r"\s+([,.!?;:])", r"\1", t)
        t = t.strip()
        if t != e.text:
            e.text = t; n += 1
    return n


def remove_empty(doc: Document) -> int:
    before = len(doc.events)
    doc.events = [e for e in doc.events if strip_tags(e.text).strip() or has_drawing(e.text) or e.comment]
    return before - len(doc.events)


def rename_style(doc: Document, old: str, new: str) -> None:
    s = doc.style(old)
    if s: s.name = new
    for e in doc.events:
        if e.style == old: e.style = new


def resample(doc: Document, new_x: int, new_y: int, margins: bool = True) -> None:
    """Scale resolution: fonts, outlines, margins, \\pos/\\fs tags are scaled (basic)."""
    ox, oy = doc.res
    fx, fy = new_x / ox, new_y / oy
    for s in doc.styles:
        s.fontsize *= fy; s.outline_w *= fy; s.shadow *= fy; s.spacing *= fx
        if margins:
            s.margin_l = round(s.margin_l * fx); s.margin_r = round(s.margin_r * fx); s.margin_t = round(s.margin_t * fy)
    for e in doc.events:
        if margins:
            e.margin_l = round(e.margin_l * fx); e.margin_r = round(e.margin_r * fx); e.margin_t = round(e.margin_t * fy)
        def pos(m):
            return "\\%s(%g,%g" % (m.group(1), float(m.group(2)) * fx, float(m.group(3)) * fy)
        e.text = re.sub(r"\\(pos|move|org)\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)", pos, e.text)
        e.text = re.sub(r"\\(fs)(\d+(?:\.\d+)?)", lambda m: "\\fs%g" % (float(m.group(2)) * fy), e.text)
    doc.set_info("PlayResX", str(new_x)); doc.set_info("PlayResY", str(new_y))


# ------------------------------------------------------------------ QC
@dataclass
class Issue:
    index: int
    code: str
    message: str
    severity: str = "warning"  # error | warning | info


@dataclass
class QCConfig:
    max_cps: float = 20.0          # anime releases typically 17-20
    max_cpl: int = 42              # chars per line
    max_lines: int = 2
    min_duration: int = 500
    max_duration: int = 8000
    min_gap: int = 0               # ms; frames-based gap checks use this
    check_overlap: bool = True
    check_styles: bool = True
    check_turkish: bool = True
    allowed_styles: Optional[set] = None


_TR_SUSPECT = [
    (re.compile(r"\bi(?=[A-ZÇĞİÖŞÜ]{2,})"), "Büyük harf içinde küçük 'i' (İ olmalı?)"),
    (re.compile(r"[ıİ]\w*[A-Z]\w*[a-z]"), "Karışık I/İ kullanımı olabilir"),
    (re.compile(r"Ä°|Ã§|Ã¶|Ã¼|ÅŸ|Ä±|ÄŸ|Ã‡|Ã–|Ãœ|Åž|Äž"), "Bozuk kodlama (mojibake): UTF-8 metin yanlış okunmuş"),
    (re.compile(r"[\u00dd\u00fd\u00de\u00fe\u00d0\u00f0]"), "Latin-1/Windows-1254 kalıntısı (Ý, ý, Þ, þ, Ð, ð)"),
]


def run_qc(doc: Document, cfg: Optional[QCConfig] = None, fr: Optional[FrameRate] = None) -> list[Issue]:
    cfg = cfg or QCConfig()
    out: list[Issue] = []
    style_names = {s.name for s in doc.styles}
    order = sorted((i for i, e in enumerate(doc.events) if not e.comment), key=lambda i: (doc.events[i].start, doc.events[i].end))
    for i, e in enumerate(doc.events):
        if e.comment:
            continue
        vis = strip_tags(e.text)
        if not vis.strip() and not has_drawing(e.text):
            out.append(Issue(i, "empty", "Boş satır", "error")); continue
        if has_drawing(e.text):
            continue
        if e.end <= e.start:
            out.append(Issue(i, "badtime", "Bitiş başlangıçtan önce/eşit", "error"))
        if cfg.check_styles and e.style not in style_names:
            out.append(Issue(i, "style", f"Tanımsız stil: {e.style}", "error"))
        if cfg.allowed_styles is not None and e.style not in cfg.allowed_styles:
            out.append(Issue(i, "style_unlisted", f"Beklenmeyen stil: {e.style}", "info"))
        dur = max(1, e.duration)
        cps = char_count(e.text) / (dur / 1000.0)
        if cps > cfg.max_cps:
            out.append(Issue(i, "cps", f"Okuma hızı {cps:.1f} cps (> {cfg.max_cps:g})", "warning"))
        if dur < cfg.min_duration:
            out.append(Issue(i, "short", f"Süre çok kısa: {dur} ms"))
        if dur > cfg.max_duration:
            out.append(Issue(i, "long", f"Süre çok uzun: {dur} ms", "info"))
        ls = [l for l in re.split(r"\\N|\\n", vis)]
        if len(ls) > cfg.max_lines:
            out.append(Issue(i, "lines", f"{len(ls)} satır (> {cfg.max_lines})"))
        for l in ls:
            if len(l.strip()) > cfg.max_cpl:
                out.append(Issue(i, "cpl", f"Satır uzunluğu {len(l.strip())} (> {cfg.max_cpl})")); break
        if "  " in vis:
            out.append(Issue(i, "spaces", "Çift boşluk", "info"))
        if vis != vis.strip() or re.search(r"\\N\s|\s\\N", e.text):
            out.append(Issue(i, "trim", "Baştaki/sondaki boşluk", "info"))
        if e.text.count("{") != e.text.count("}"):
            out.append(Issue(i, "tagbrace", "Eşleşmeyen { } parantezi", "error"))
        if re.search(r"\\i1", e.text) and not re.search(r"\\i0", e.text) and e.style:
            pass  # style may be italic by design; skip
        if cfg.check_turkish:
            for rx, msg in _TR_SUSPECT:
                if rx.search(vis):
                    out.append(Issue(i, "turkish", msg, "warning")); break
    if cfg.check_overlap:
        for a, b in zip(order, order[1:]):
            x, y = doc.events[a], doc.events[b]
            if x.end > y.start and x.layer == y.layer and x.style == y.style and not has_drawing(x.text + y.text):
                out.append(Issue(b, "overlap", f"Önceki satırla {x.end - y.start} ms çakışıyor"))
            elif cfg.min_gap and 0 < y.start - x.end < cfg.min_gap:
                out.append(Issue(b, "gap", f"Önceki satırla boşluk {y.start - x.end} ms", "info"))
    out.sort(key=lambda s: (s.index, s.code))
    return out
