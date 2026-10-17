"""Denetim (QC): yaygın hataları bulma ve düzeltme (Subtitle Edit "Fix common errors" karşılığı).

Her kural:
  check(events, i, opts) -> [Issue]          tek satır (ya da komşusuyla) denetimi
  fix_text(text, opts) -> text               (metin kurallarında) düzeltme
Zaman kurallarının düzeltmesi Issue.fix içinde {alan: değer} olarak gelir.

Metin düzeltmeleri YALNIZ görünen metne uygulanır; {…} etiket blokları ve \\p
çizimleri olduğu gibi kalır. Türkçe büyük/küçük harf (i↔İ, ı↔I) doğru yapılır.
"""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass, field

from .textutil import cpl_info, cps, strip_tags, visible_lines


@dataclass
class QcOptions:
    min_duration: int = 700          # ms
    max_duration: int = 7000
    cps_max: float = 25.0
    min_gap: int = 84                # ms (~2 kare); 0 < boşluk < min_gap ise uyar
    max_lines: int = 2
    ellipsis: str = "..."            # "..." ya da "…"
    same_style_only: bool = True     # çakışma/boşluk yalnız aynı stil+katman içinde
    spell = None                     # yazım denetleyicisi (check(word)->bool, suggest(word))
    glossary = None                  # core.glossary.Glossary
    ref_text = None                  # satır -> referans metni (EN/JA) işlevi


@dataclass
class Issue:
    row: int
    rule: str
    detail: str
    fix: dict | None = None          # {"text": ...} ya da {"start"/"end": ...}
    severity: str = "warn"           # warn | error | info

    @property
    def fixable(self) -> bool:
        return self.fix is not None


# ---------------------------------------------------------------------------
# Türkçe harf ve görünen metin yardımcıları
# ---------------------------------------------------------------------------

def tr_upper(s: str) -> str:
    return s.replace("i", "İ").replace("ı", "I").upper()


def tr_lower(s: str) -> str:
    return s.replace("I", "ı").replace("İ", "i").lower()


_BLOCK = re.compile(r"(\{[^}]*\})")


def map_plain(text: str, fn) -> str:
    """fn'i yalnız görünen metin parçalarına uygular (etiket blokları ve çizimler hariç)."""
    parts = _BLOCK.split(text)
    drawing = False
    out = []
    plain_idx = [i for i, p in enumerate(parts) if p and not p.startswith("{")]
    first = plain_idx[0] if plain_idx else -1
    last = plain_idx[-1] if plain_idx else -1
    for i, p in enumerate(parts):
        if p.startswith("{") and p.endswith("}"):
            for d in re.finditer(r"\\p(\d+)", p):
                drawing = int(d.group(1)) > 0
            out.append(p)
        elif p and not drawing:
            out.append(fn(p, i == first, i == last))
        else:
            out.append(p)
    return "".join(out)


def plain_text(text: str) -> str:
    return strip_tags(text)


# ---------------------------------------------------------------------------
# Metin düzeltme işlevleri
# ---------------------------------------------------------------------------

def fix_double_space(text, _o=None):
    return map_plain(text, lambda p, f, l: re.sub(r" {2,}", " ", p))


def fix_edge_space(text, _o=None):
    def fn(p, first, last):
        p = re.sub(r" *\\N *", r"\\N", p)
        if first:
            p = p.lstrip(" ")
        if last:
            p = p.rstrip(" ")
        return p
    return map_plain(text, fn)


def fix_space_before_punct(text, _o=None):
    # "kelime ," -> "kelime,"  ("..." ile başlayan devam satırlarına dokunma)
    return map_plain(text, lambda p, f, l: re.sub(r"(?<=\S) +([,;:!?]|\.(?!\.))", r"\1", p))


def fix_space_after_punct(text, _o=None):
    def fn(p, f, l):
        p = re.sub(r"([,;:!?])(?=[^\W\d_])", r"\1 ", p)                  # "a,b" -> "a, b"
        p = re.sub(r"(?<=[^\W\d_]{2})\.(?=[A-ZÇĞİÖŞÜ][a-zçğıöşü])", ". ", p)   # "bitti.Sonra"
        return p
    return map_plain(text, fn)


def fix_dots(text, o=None):
    target = (o.ellipsis if o else "...")

    def fn(p, f, l):
        p = p.replace("…", "...")
        p = re.sub(r"(?<!\.)\.{2}(?!\.)", "...", p)       # ".." -> "..."
        p = re.sub(r"\.{4,}", "...", p)                     # "...." -> "..."
        if target != "...":
            p = p.replace("...", target)
        return p
    return map_plain(text, fn)


_WORD = r"[^\W\d_]+"


def fix_repeated_word(text, _o=None):
    def fn(p, f, l):
        return re.sub(rf"\b({_WORD})( +)\1\b", r"\1", p, flags=re.IGNORECASE)
    return map_plain(text, fn)


def fix_lowercase_start(text, _o=None):
    done = [False]

    def fn(p, f, l):
        if done[0]:
            return p
        m = re.search(r"[^\W\d_]", p)
        if not m:
            return p
        done[0] = True
        c = m.group(0)
        return p[:m.start()] + tr_upper(c) + p[m.end():]
    return map_plain(text, fn)


def fix_dash_space(text, _o=None):
    return map_plain(text, lambda p, f, l: re.sub(r"(^|\\N)-(?=[^\s\-])", r"\1- ", p))


def fix_linebreak_edge(text, _o=None):
    def fn(p, first, last):
        p = re.sub(r"(\\N){2,}", r"\\N", p)
        if first:
            p = re.sub(r"^(\\N)+", "", p)
        if last:
            p = re.sub(r"(\\N)+$", "", p)
        return p
    return map_plain(text, fn)


# ---------------------------------------------------------------------------
# Kurallar
# ---------------------------------------------------------------------------

@dataclass
class Rule:
    id: str
    name: str
    default: bool = True
    fixer: object = None             # metin düzeltme işlevi
    kind: str = "text"               # text | time | info

    def check(self, events, i, o) -> list[Issue]:
        ev = events[i]
        if self.fixer is not None:
            new = self.fixer(ev.text, o)
            if new != ev.text:
                return [Issue(i, self.id, _preview(ev.text, new), {"text": new})]
        return []


def _preview(old, new):
    a, b = strip_tags(old), strip_tags(new)
    return f"{a[:50]}  →  {b[:50]}"


class _Empty(Rule):
    def check(self, events, i, o):
        ev = events[i]
        if "\\p" in ev.text and re.search(r"\\p[1-9]", ev.text):
            return []
        if not plain_text(ev.text).replace("\\N", "").replace("\\h", "").strip():
            return [Issue(i, self.id, "Görünen metin yok", None, "error")]
        return []


class _Duration(Rule):
    def check(self, events, i, o):
        ev = events[i]
        d = ev.end - ev.start
        out = []
        if self.id == "too_short" and d < o.min_duration:
            # düzelt: bitişi uzat, sonraki satıra taşmadan
            nxt = _next_start(events, i, o)
            new_end = ev.start + o.min_duration
            if nxt is not None:
                new_end = min(new_end, nxt)
            fix = {"end": new_end} if new_end > ev.end else None
            out.append(Issue(i, self.id, f"{d} ms (< {o.min_duration})", fix, "warn"))
        if self.id == "too_long" and d > o.max_duration:
            out.append(Issue(i, self.id, f"{d / 1000:.1f} sn (> {o.max_duration / 1000:.0f})", None, "info"))
        return out


class _Cps(Rule):
    def check(self, events, i, o):
        ev = events[i]
        v = cps(ev.text, ev.end - ev.start)
        if v > o.cps_max:
            return [Issue(i, self.id, f"{v:.0f} kr/sn (> {o.cps_max:.0f})", None, "error" if v > o.cps_max * 1.3 else "warn")]
        return []


class _Cpl(Rule):
    def check(self, events, i, o):
        info = cpl_info(events[i].text)
        if info["status"] != "ok":
            return [Issue(i, self.id, info["label"], None, "error" if info["status"] == "error" else "warn")]
        return []


class _Lines(Rule):
    def check(self, events, i, o):
        n = len(visible_lines(events[i].text))
        if n > o.max_lines:
            return [Issue(i, self.id, f"{n} satır", None, "warn")]
        return []


class _Unbalanced(Rule):
    def check(self, events, i, o):
        t = events[i].text
        probs = []
        if t.count("{") != t.count("}"):
            probs.append("{ } dengesiz")
        p = plain_text(t)
        if p.count("(") != p.count(")"):
            probs.append("( ) dengesiz")
        if p.count('"') % 2:
            probs.append('" tek sayıda')
        if p.count("“") != p.count("”"):
            probs.append("“ ” dengesiz")
        return [Issue(i, self.id, ", ".join(probs), None, "warn")] if probs else []


class _LowerStart(Rule):
    def check(self, events, i, o):
        ev = events[i]
        p = plain_text(ev.text).lstrip(" -\"'“(")
        if not p or p.startswith(".") or p.startswith("…"):
            return []
        c = p[0]
        if not c.isalpha() or not c.islower():
            return []
        prev = _prev_line(events, i, o)
        if prev is not None:
            pp = plain_text(events[prev].text).rstrip(" \"'”)")
            if pp and pp[-1] not in ".!?":
                return []            # önceki cümle sürüyor
        return [Issue(i, self.id, f"'{c}' küçük harfle başlıyor", {"text": fix_lowercase_start(ev.text)}, "info")]


class _Overlap(Rule):
    def check(self, events, i, o):
        ev = events[i]
        j = _next_index(events, i, o)
        if j is None:
            return []
        nx = events[j]
        if nx.start < ev.end:
            ov = ev.end - nx.start
            fix = {"end": nx.start} if nx.start > ev.start else None
            return [Issue(i, self.id, f"{ov} ms satır {j + 1} ile çakışıyor", fix, "error")]
        return []


class _Gap(Rule):
    def check(self, events, i, o):
        ev = events[i]
        j = _next_index(events, i, o)
        if j is None:
            return []
        gap = events[j].start - ev.end
        if 0 < gap < o.min_gap:
            return [Issue(i, self.id, f"{gap} ms boşluk (satır {j + 1})", {"end": events[j].start}, "info")]
        return []


class _Spell(Rule):
    def check(self, events, i, o):
        sp = o.spell
        if sp is None or not getattr(sp, "ready", False):
            return []
        bad = [w for w, _s, _e in words_in(events[i].text) if not sp.check(w)]
        if bad:
            return [Issue(i, self.id, ", ".join(dict.fromkeys(bad)), None, "warn")]
        return []


class _Glossary(Rule):
    def check(self, events, i, o):
        g = o.glossary
        if g is None or not g.terms:
            return []
        ev = events[i]
        plain = plain_text(ev.text)
        out = []
        hits = g.find_forbidden(plain)
        if hits:
            new = map_plain(ev.text, lambda p, f, l: g.fix_forbidden(p))
            detail = ", ".join(dict.fromkeys(f"{plain[a:b]} → {t.target}" for t, _bad, a, b, _s in hits))
            out.append(Issue(i, self.id, detail, {"text": new} if new != ev.text else None, "warn"))
        if o.ref_text is not None:
            ref = o.ref_text(i) or ""
            if ref:
                flagged = {id(t) for t, *_ in hits}          # yasak varyantı zaten raporlanan terim
                miss = [t for t in g.missing_terms(plain_text(ref), plain) if id(t) not in flagged]
                if miss:
                    out.append(Issue(i, self.id, "Terim eksik: " + ", ".join(f"{t.source} → {t.target}" for t in miss),
                                     None, "info"))
        return out


def _key(ev, o):
    return (ev.style, ev.layer) if o.same_style_only else None


class _Nav:
    """Komşu satır dizini: (stil, katman) grubu başına (başlangıç, satır) sırası.
    run_qc başında bir kez kurulur; _next_index/_prev_line O(1) olur (eskiden her çağrı O(n) idi)."""

    def __init__(self, events, o):
        groups: dict = {}
        for j, e in enumerate(events):
            if not e.comment:
                groups.setdefault(_key(e, o), []).append((e.start, j))
        self.next: dict[int, int] = {}
        self.prev: dict[int, int] = {}
        for lst in groups.values():
            lst.sort()
            for k in range(len(lst)):
                j = lst[k][1]
                if k + 1 < len(lst):
                    self.next[j] = lst[k + 1][1]
                if k > 0:
                    self.prev[j] = lst[k - 1][1]


def _next_index(events, i, o):
    nav = getattr(o, "_nav", None)
    if nav is not None and not events[i].comment:
        return nav.next.get(i)
    ev = events[i]
    best = None
    for j, e in enumerate(events):
        if j == i or e.comment or _key(e, o) != _key(ev, o):
            continue
        if e.start >= ev.start and (e.start, j) > (ev.start, i):
            if best is None or (e.start, j) < (events[best].start, best):
                best = j
    return best


def _next_start(events, i, o):
    j = _next_index(events, i, o)
    return events[j].start if j is not None else None


def _prev_line(events, i, o):
    nav = getattr(o, "_nav", None)
    if nav is not None and not events[i].comment:
        return nav.prev.get(i)
    ev = events[i]
    best = None
    for j, e in enumerate(events):
        if j == i or e.comment or _key(e, o) != _key(ev, o):
            continue
        if (e.start, j) < (ev.start, i) and (best is None or (e.start, j) > (events[best].start, best)):
            best = j
    return best


RULES: list[Rule] = [
    _Empty("empty", "Boş satır", kind="info"),
    Rule("double_space", "Çift boşluk", fixer=fix_double_space),
    Rule("edge_space", "Satır başı/sonu ve \\N çevresinde boşluk", fixer=fix_edge_space),
    Rule("space_before_punct", "Noktalamadan önce boşluk", fixer=fix_space_before_punct),
    Rule("space_after_punct", "Noktalamadan sonra boşluk eksik", fixer=fix_space_after_punct),
    Rule("dots", "Üç nokta (.. / .... / …)", fixer=fix_dots),
    Rule("repeated_word", "Tekrarlanan kelime", fixer=fix_repeated_word),
    _LowerStart("lowercase_start", "Cümle küçük harfle başlıyor", fixer=fix_lowercase_start),
    Rule("dash_space", "Diyalog çizgisinden sonra boşluk", default=False, fixer=fix_dash_space),
    Rule("linebreak_edge", "Baştaki/sondaki ya da çift \\N", fixer=fix_linebreak_edge),
    _Unbalanced("unbalanced", "Dengesiz parantez/tırnak/etiket", kind="info"),
    _Duration("too_short", "Çok kısa süre", kind="time"),
    _Duration("too_long", "Çok uzun süre", kind="info"),
    _Cps("cps", "Okuma hızı (CPS)", kind="info"),
    _Cpl("cpl", "Satır uzunluğu (CPL)", kind="info"),
    _Lines("lines", "Satır sayısı", kind="info"),
    _Overlap("overlap", "Çakışan satırlar", kind="time"),
    _Gap("gap", "Çok küçük boşluk (kapat)", default=False, kind="time"),
    _Spell("spell", "Yazım (Türkçe sözlük)", kind="info"),
    _Glossary("glossary", "Terim sözlüğü", kind="suggest"),
]
RULES_BY_ID = {r.id: r for r in RULES}


def run_qc(events, rows: list[int], enabled: set[str], opts: QcOptions | None = None) -> list[Issue]:
    o = opts or QcOptions()
    o._nav = _Nav(events, o)
    out = []
    for i in rows:
        if i < 0 or i >= len(events) or events[i].comment:
            continue
        for r in RULES:
            if r.id in enabled:
                out.extend(r.check(events, i, o))
    o._nav = None
    return out


def apply_fixes(events, issues: list[Issue], opts: QcOptions | None = None) -> dict[int, dict]:
    """Seçili sorunların düzeltmelerini satır başına birleştirir.
    Metin kuralları kural sırasıyla AYNI satırın güncel metnine yeniden uygulanır
    (böylece iki düzeltme birbirini ezmez)."""
    o = opts or QcOptions()
    order = {r.id: k for k, r in enumerate(RULES)}
    by_row: dict[int, list[Issue]] = {}
    for iss in issues:
        if iss.fix is not None:
            by_row.setdefault(iss.row, []).append(iss)
    changes: dict[int, dict] = {}
    for row, iss_list in by_row.items():
        ev = events[row]
        text = ev.text
        ch: dict = {}
        for iss in sorted(iss_list, key=lambda x: order.get(x.rule, 99)):
            r = RULES_BY_ID.get(iss.rule)
            if "text" in iss.fix:
                text = r.fixer(text, o) if (r and r.fixer) else iss.fix["text"]
            else:
                ch.update({k: v for k, v in iss.fix.items() if k in ("start", "end")})
        if text != ev.text:
            ch["text"] = text
        if ch:
            changes[row] = ch
    return changes


# ---------------------------------------------------------------------------
# Kelimeler (yazım denetimi için)
# ---------------------------------------------------------------------------

_WORD_RE = re.compile(r"[^\W\d_]+(?:['’][^\W\d_]+)?(?:-[^\W\d_]+)*")


def words_in(text: str) -> list[tuple[str, int, int]]:
    """Görünen kelimeler ve HAM metindeki konumları (vurgulayıcı için)."""
    out = []
    pos = 0
    drawing = False
    for part in _BLOCK.split(text):
        if part.startswith("{") and part.endswith("}"):
            for d in re.finditer(r"\\p(\d+)", part):
                drawing = int(d.group(1)) > 0
        elif part and not drawing:
            # \N \n \h kaçışlarını kelimeden ayır
            masked = re.sub(r"\\[Nnh]", "  ", part)
            for m in _WORD_RE.finditer(masked):
                out.append((m.group(0), pos + m.start(), pos + m.end()))
        pos += len(part)
    return out


def normalize_word(w: str) -> str:
    return unicodedata.normalize("NFC", w)
