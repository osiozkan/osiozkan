"""Kaynak (EN) ↔ aday (TR) hizalama ve denetim — aligncompare'ın uygulama içi hali.

Osi'nin aligncompare kuralları (kılavuzdaki gibi):
  * Kaynak .ass/.srt ile aday .ass satırları ZAMAN ÖRTÜŞMESİNE göre gruplanır;
    1:1, 1:2, 2:1 (ve daha büyük) birleşme/bölünmeler tek grup olur.
  * İki dosya arasındaki küresel kayma kendiliğinden bulunur (±15 sn) ve eşleştirme
    o kaymayla yapılır; elle de verilebilir.
  * Typeset (\\pos/\\move/\\clip/\\p) ve sonu "\\N \\N \\N" olan ekran yazıları
    varsayılan olarak hizalamaya GİRMEZ (`include_typeset` ile girer).
  * Aday dosyaya DOKUNULMAZ; işaretli kopya ayrı belgedir.

Bayraklar:
  EKSİK       kaynakta var, adayda karşılığı yok (çevrilmemiş içerik)
  FAZLA       adayda var, kaynakta yok (halüsinasyon şüphesi; çoğu zaman kredi/çevirmen notu)
  İNGİLİZCE?  aday satır kaynağın kopyası gibi ya da İngilizce kalmış
  UZUNLUK     aday/kaynak karakter oranı makul aralığın dışında
  BOŞ         aday satırın görünür metni yok
  ZAMAN       eşleşmiş grupta başlangıç, kaymadan sonra da kaynaktan belirgin sapıyor
  SAYI        kaynaktaki rakam(lar) adayda yok (yazıyla çevrilmiş olabilir; yumuşak bayrak)

Örtüşme ölçüsü (aktarma aracıyla aynı): çakışan süre / iki satırdan KISA olanın süresi.
Oran `min_overlap` ve üstündeyse iki satır aynı gruba bağlanır; gruplar bağlı bileşenlerdir.

Hata yüzeyi (bilerek söylenir):
  * Kayma 15 sn'den büyükse ya da yanlış dosya çifti verildiyse her şey EKSİK/FAZLA çıkar;
    rapor bunu "zayıf ilinti" uyarısıyla söyler. Önce senkron (bsync / referansa göre senkron).
  * Kayma bölüm içinde değişiyorsa (sahne aralarında farklı) tek küresel kayma yetmez;
    ZAMAN bayrakları kümelenir — parçalı senkron gerekir.
  * Üst üste binen diyaloglar (iki konuşmacı aynı anda) tek gruba düşer; grup metinleri
    birleştirilerek değerlendirilir.
"""

from __future__ import annotations

import bisect
import csv
import difflib
import html
import io
import os
import re
import unicodedata
from dataclasses import dataclass, field

import numpy as np

from .asstime import format_time
from .textutil import strip_tags

RES = 10          # ms / örnek (kayma araması)

FLAG_ORDER = ["EKSİK", "FAZLA", "İNGİLİZCE?", "UZUNLUK", "BOŞ", "ZAMAN", "SAYI"]
FLAG_HELP = {
    "EKSİK": "Kaynakta var, adayda karşılığı yok (çevrilmemiş içerik)",
    "FAZLA": "Adayda var, kaynakta yok (halüsinasyon şüphesi; kredi/nota bakar)",
    "İNGİLİZCE?": "Aday satır kaynağın kopyası gibi ya da İngilizce kalmış",
    "UZUNLUK": "Aday/kaynak karakter oranı makul aralığın dışında",
    "BOŞ": "Aday satırın görünür metni yok",
    "ZAMAN": "Eşleşmiş grupta zamanlama kaynaktan belirgin sapıyor",
    "SAYI": "Kaynaktaki rakam(lar) adayda yok (yazıyla çeviri olabilir; yumuşak)",
}

# Türkçede de kullanılan ünlemler/sözcükler (hey, oh, of, ok, on, at, her, it, can, ben, he...) bilerek yok
_EN_WORDS = frozenset("""
the and you your you're i'm im i've i'll i'd me my mine we we're our us they they're them their
she his him is are was were been being to with what what's that that's this these those there
there's here where when why who how which don't dont doesn't didn't can't cannot won't wouldn't
isn't aren't wasn't weren't haven't hasn't would could should will shall have has had just
about from into because really something nothing anything everything someone anyone everyone
know think want need going gonna wanna gotta got get let let's please thank thanks sorry yes
yeah right well now then than very much more some any all only also too back again never
always maybe still even though thing things way time people come came go goes went see saw look
like said say says tell told make made take took give gave okay alright but or if so do does did
""".split())

_TR_LETTERS = set("çğıöşüÇĞİÖŞÜ")
_NUM = re.compile(r"\d+(?:[.,:]\d+)*")
_TS_TAGS = re.compile(r"\\(?:pos|move|i?clip)\s*\(|\\p[1-9]")


# ---------------------------------------------------------------------------
# Seçenekler ve sonuç türleri
# ---------------------------------------------------------------------------

@dataclass
class CompareOptions:
    shift: str | float = "auto"          # "auto" ya da saniye (aday kaynaktan bu kadar geç)
    shift_range: float = 15.0            # otomatik arama aralığı (± sn)
    ratio_min: float = 0.45
    ratio_max: float = 2.2
    sim: float = 0.85                    # "İNGİLİZCE?" benzerlik eşiği
    max_start_diff: float = 0.5          # sn — ZAMAN bayrağı eşiği
    min_overlap: float = 0.25
    skip_styles_src: str = ""            # regex
    skip_styles_cand: str = ""           # regex
    include_typeset: bool = False
    min_len: int = 10                    # UZUNLUK yalnız kaynak bu kadar karakterden uzunsa


@dataclass
class Line:
    side: str                # "src" | "cand"
    row: int                 # kendi dosyasındaki olay sırası (0 tabanlı)
    start: int
    end: int
    style: str
    text: str
    plain: str


@dataclass
class Flag:
    code: str
    detail: str = ""
    rows: list = field(default_factory=list)      # bayrağın düştüğü aday satırları (0 tabanlı)

    def label(self) -> str:
        return f"{self.code} {self.detail}".strip()


@dataclass
class Group:
    src: list
    cand: list
    flags: list = field(default_factory=list)

    @property
    def kind(self) -> str:
        return f"{len(self.src)}:{len(self.cand)}"

    @property
    def start(self) -> int:
        """Kaynak zaman ekseninde başlangıç (adayınki kaymadan arındırılmış olarak gelir)."""
        return min(x.start for x in self.src) if self.src else min(x.start for x in self.cand)

    @property
    def codes(self) -> list[str]:
        return [f.code for f in self.flags]

    def src_text(self, sep: str = " / ") -> str:
        return sep.join(x.plain for x in self.src)

    def cand_text(self, sep: str = " / ") -> str:
        return sep.join(x.plain for x in self.cand)


@dataclass
class CompareResult:
    groups: list
    shift_ms: int
    shift_auto: bool
    shift_score: float                   # örtüşmenin küçük dosyanın kapsamına oranı (0..1)
    src_count: int
    cand_count: int
    src_skipped: dict
    cand_skipped: dict
    warnings: list = field(default_factory=list)

    def counts(self) -> dict[str, int]:
        out = {k: 0 for k in FLAG_ORDER}
        for g in self.groups:
            for f in g.flags:
                out[f.code] = out.get(f.code, 0) + 1
        return out

    def kinds(self) -> dict[str, int]:
        out: dict[str, int] = {}
        for g in self.groups:
            if g.src and g.cand:
                out[g.kind] = out.get(g.kind, 0) + 1
        return out

    def flagged(self) -> list[Group]:
        return [g for g in self.groups if g.flags]

    def flags_by_cand_row(self) -> dict[int, list[Flag]]:
        out: dict[int, list[Flag]] = {}
        for g in self.groups:
            for f in g.flags:
                rows = f.rows or [c.row for c in g.cand]
                for r in rows:
                    out.setdefault(r, []).append(f)
        return out


# ---------------------------------------------------------------------------
# Metin yardımcıları
# ---------------------------------------------------------------------------

def plain_of(text: str) -> str:
    """Görünen metin: etiketler/çizimler atılır, \\N \\n \\h boşluk olur, boşluklar sadeleşir."""
    t = strip_tags(text).replace("\\N", " ").replace("\\n", " ").replace("\\h", " ")
    return " ".join(t.split())


def is_typeset(ev) -> bool:
    if _TS_TAGS.search(ev.text):
        return True
    return bool(re.search(r"(\\N\s*){3}$", ev.text.rstrip()))


def _norm(s: str) -> str:
    s = unicodedata.normalize("NFC", s).casefold()
    s = "".join(ch if (ch.isalnum() or ch.isspace()) else " " for ch in s)
    return " ".join(s.split())


def similarity(a: str, b: str) -> float:
    na, nb = _norm(a), _norm(b)
    if not na or not nb:
        return 0.0
    return difflib.SequenceMatcher(None, na, nb, autojunk=False).ratio()


def english_words(text: str) -> list[str]:
    """Metin İngilizce kalmış görünüyorsa İngilizce işlev sözcükleri; değilse boş liste."""
    if any(ch in _TR_LETTERS for ch in text):
        return []
    words = re.findall(r"[a-z]+(?:'[a-z]+)?", text.lower().replace("’", "'"))
    if len(words) < 3:
        return []
    hits = [w for w in words if w in _EN_WORDS]
    if len(hits) >= 2 and len(hits) / len(words) >= 0.3:
        return hits
    return []


def numbers_in(text: str) -> set[str]:
    return {re.sub(r"\D", "", m.group(0)) for m in _NUM.finditer(text)}


def _visible_len(s: str) -> int:
    s = unicodedata.normalize("NFC", s)
    return sum(1 for ch in s if not ch.isspace() and unicodedata.category(ch) not in ("Mn", "Me", "Cf"))


# ---------------------------------------------------------------------------
# Satır seçimi
# ---------------------------------------------------------------------------

def collect(events, side: str, opts: CompareOptions) -> tuple[list[Line], dict]:
    pat = opts.skip_styles_src if side == "src" else opts.skip_styles_cand
    rx = re.compile(pat) if pat else None
    out, skipped = [], {"yorum": 0, "typeset": 0, "stil": 0, "boş": 0}
    for i, ev in enumerate(events):
        if ev.comment:
            skipped["yorum"] += 1
            continue
        if rx is not None and rx.search(ev.style or ""):
            skipped["stil"] += 1
            continue
        if not opts.include_typeset and is_typeset(ev):
            skipped["typeset"] += 1
            continue
        plain = plain_of(ev.text)
        if side == "src" and not plain:
            skipped["boş"] += 1              # kaynakta görünmeyen satır karşılaştırılmaz
            continue
        out.append(Line(side, i, ev.start, ev.end, ev.style, ev.text, plain))
    return out, skipped


# ---------------------------------------------------------------------------
# Küresel kayma
# ---------------------------------------------------------------------------

def _coverage(lines, n, shift=0):
    a = np.zeros(n, np.float32)
    for x in lines:
        s, e = (x.start - shift) // RES, (x.end - shift) // RES
        a[max(0, s):max(0, min(n, e))] = 1.0
    return a


def _onsets(lines, n, sigma_ms=120):
    a = np.zeros(n, np.float32)
    for x in lines:
        for t in (x.start, x.end):
            k = t // RES
            if 0 <= k < n:
                a[k] += 1.0
    sig = sigma_ms / RES
    xs = np.arange(-int(3 * sig), int(3 * sig) + 1)
    ker = np.exp(-0.5 * (xs / sig) ** 2)
    return np.convolve(a, ker, mode="same").astype(np.float32)


def _xcorr(a, b, max_lag):
    """c[lag] = Σ a[t] · b[t + lag], lag ∈ [−max_lag, max_lag]."""
    n = len(a)
    m = 1 << int(np.ceil(np.log2(2 * n + 1)))
    corr = np.fft.irfft(np.conj(np.fft.rfft(a, m)) * np.fft.rfft(b, m), m)
    lags = np.arange(-max_lag, max_lag + 1)
    return lags, corr[lags % m]


def find_shift(src: list[Line], cand: list[Line], max_shift_ms: int) -> tuple[int, float]:
    """Adayın kaynağa göre kayması (ms; artı = aday GEÇ) ve güven (0..1).

    Kaba: kapsama ilintisi (bölünme farklarına dayanıklı); platonun ORTASI alınır.
    İnce: satır başı/sonu darbeleriyle ±300 ms içinde keskin tepe.
    """
    if not src or not cand:
        return 0, 0.0
    total = max(max(x.end for x in src), max(x.end for x in cand)) + max_shift_ms + 1000
    n = total // RES + 1
    R = max_shift_ms // RES
    a, b = _coverage(src, n), _coverage(cand, n)
    lags, vals = _xcorr(a, b, R)
    vmax = float(vals.max())
    if vmax <= 0:
        return 0, 0.0
    k = int(np.argmax(vals))
    lo = hi = k
    tol = vmax * 0.995
    while lo > 0 and vals[lo - 1] >= tol:
        lo -= 1
    while hi < len(vals) - 1 and vals[hi + 1] >= tol:
        hi += 1
    coarse = int(lags[(lo + hi) // 2])
    oa, ob = _onsets(src, n), _onsets(cand, n)
    fl, fv = _xcorr(oa, ob, R)
    win = np.abs(fl - coarse) <= 300 // RES
    fine = int(fl[win][int(np.argmax(fv[win]))]) if win.any() and fv[win].max() > 0 else coarse
    # güven: tepenin rastgele hizadaki örtüşmeden (ortanca) ne kadar yukarıda olduğu;
    # doğru çiftte ~0.8, aynı bölüme ait olmayan çiftte ~0.2
    base = float(np.median(vals))
    score = (vmax - base) / max(1.0, min(float(a.sum()), float(b.sum())) - base)
    return fine * RES, float(min(1.0, max(0.0, score)))


# ---------------------------------------------------------------------------
# Gruplama ve bayraklar
# ---------------------------------------------------------------------------

def _ratio(a0, a1, b0, b1) -> float:
    ov = min(a1, b1) - max(a0, b0)
    if ov <= 0:
        return 0.0
    return ov / max(1, min(a1 - a0, b1 - b0))


def build_groups(src: list[Line], cand: list[Line], shift_ms: int, min_overlap: float) -> list[Group]:
    """Zaman örtüşmesiyle bağlı bileşenler (aday zamanları kaymadan arındırılır)."""
    ns = len(src)
    parent = list(range(ns + len(cand)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    order = sorted(range(len(cand)), key=lambda j: cand[j].start)
    c_starts = [cand[j].start - shift_ms for j in order]
    max_c = max((c.end - c.start for c in cand), default=0)
    for i, s in enumerate(src):
        lo = bisect.bisect_left(c_starts, s.start - max_c)
        hi = bisect.bisect_right(c_starts, s.end)
        for k in range(lo, hi):
            j = order[k]
            c = cand[j]
            if _ratio(s.start, s.end, c.start - shift_ms, c.end - shift_ms) >= min_overlap:
                ra, rb = find(i), find(ns + j)
                if ra != rb:
                    parent[rb] = ra
    comps: dict[int, Group] = {}
    for i, s in enumerate(src):
        comps.setdefault(find(i), Group([], [])).src.append(s)
    for j, c in enumerate(cand):
        comps.setdefault(find(ns + j), Group([], [])).cand.append(c)
    groups = list(comps.values())
    for g in groups:
        g.src.sort(key=lambda x: (x.start, x.row))
        g.cand.sort(key=lambda x: (x.start, x.row))
    groups.sort(key=lambda g: (min([x.start for x in g.src] + [x.start - shift_ms for x in g.cand]),
                               g.cand[0].row if g.cand else -1))
    return groups


def flag_group(g: Group, shift_ms: int, o: CompareOptions):
    flags = []
    if g.src and not g.cand:
        flags.append(Flag("EKSİK"))
    if g.cand and not g.src:
        flags.append(Flag("FAZLA", "", [c.row for c in g.cand]))
    empty = [c.row for c in g.cand if not c.plain]
    if empty:
        flags.append(Flag("BOŞ", "", empty))
    if g.src and g.cand:
        s_txt, c_txt = g.src_text(" "), g.cand_text(" ")
        if c_txt:
            sim = similarity(s_txt, c_txt)
            if sim >= o.sim and len(_norm(c_txt)) >= 4:
                flags.append(Flag("İNGİLİZCE?", f"kaynağa %{int(sim * 100)} benzer"))
            else:
                hits = english_words(c_txt)
                if hits:
                    flags.append(Flag("İNGİLİZCE?", "İngilizce sözcükler: " + ", ".join(dict.fromkeys(hits[:5]))))
            ls, lc = _visible_len(s_txt), _visible_len(c_txt)
            if ls >= o.min_len and ls:
                r = lc / ls
                if r < o.ratio_min or r > o.ratio_max:
                    flags.append(Flag("UZUNLUK", f"oran {r:.2f} ({lc}/{ls})"))
            miss = sorted(numbers_in(s_txt) - numbers_in(c_txt), key=lambda x: (len(x), x))
            if miss:
                flags.append(Flag("SAYI", "eksik: " + ", ".join(miss[:6])))
        d = (min(c.start for c in g.cand) - shift_ms) - min(s.start for s in g.src)
        if abs(d) > o.max_start_diff * 1000:
            flags.append(Flag("ZAMAN", f"{d / 1000:+.2f} sn"))
    order = {k: i for i, k in enumerate(FLAG_ORDER)}
    g.flags = sorted(flags, key=lambda f: order.get(f.code, 99))


def compare(src_events, cand_events, opts: CompareOptions | None = None) -> CompareResult:
    o = opts or CompareOptions()
    src, s_skip = collect(src_events, "src", o)
    cand, c_skip = collect(cand_events, "cand", o)
    warnings = []
    if o.shift == "auto" or o.shift is None:
        shift, score = find_shift(src, cand, int(o.shift_range * 1000))
        auto = True
        if src and cand and score < 0.35:
            warnings.append(f"Zayıf ilinti (%{int(score * 100)}): kayma {o.shift_range:g} sn'den büyük "
                            "ya da dosyalar aynı bölüme ait değil olabilir.")
        if abs(shift) >= int(o.shift_range * 1000) - RES:
            warnings.append("Bulunan kayma arama aralığının sınırında — önce senkronla ya da kaymayı elle ver.")
    else:
        shift, auto = int(round(float(o.shift) * 1000)), False
        score = 0.0
    groups = build_groups(src, cand, shift, o.min_overlap)
    for g in groups:
        flag_group(g, shift, o)
    if not auto:
        n = max(1, len(src))
        score = sum(1 for g in groups if g.src and g.cand) / n
    return CompareResult(groups, shift, auto, score, len(src), len(cand), s_skip, c_skip, warnings)


# ---------------------------------------------------------------------------
# Çıktılar
# ---------------------------------------------------------------------------

def shift_text(res: CompareResult) -> str:
    s = res.shift_ms / 1000
    if res.shift_ms == 0:
        how = "kayma yok"
    else:
        how = f"{s:+.2f} sn (aday kaynaktan {'GEÇ' if s > 0 else 'ERKEN'})"
    return how + (" — otomatik" if res.shift_auto else " — elle verildi")


def summary_lines(res: CompareResult, src_name: str = "kaynak", cand_name: str = "aday") -> list[str]:
    def skipped(d):
        parts = [f"{v} {k}" for k, v in d.items() if v]
        return f" ({', '.join(parts)} atlandı)" if parts else ""
    c = res.counts()
    kinds = res.kinds()
    matched = sum(kinds.values())
    k11 = kinds.get("1:1", 0)
    k12 = kinds.get("1:2", 0)
    k21 = kinds.get("2:1", 0)
    other = matched - k11 - k12 - k21
    out = [
        f"Kaynak: {src_name} — {res.src_count} satır{skipped(res.src_skipped)}",
        f"Aday:   {cand_name} — {res.cand_count} satır{skipped(res.cand_skipped)}",
        f"Bulunan kayma: {shift_text(res)}",
        f"Eşleşen grup: {matched} (1:1 {k11} · 1:2 {k12} · 2:1 {k21} · diğer {other})",
        "Bayraklar: " + " · ".join(f"{k} {c.get(k, 0)}" for k in FLAG_ORDER),
    ]
    out += [f"UYARI: {w}" for w in res.warnings]
    return out


def _times(lines) -> str:
    if not lines:
        return ""
    return f"{format_time(min(x.start for x in lines))} → {format_time(max(x.end for x in lines))}"


def to_csv(res: CompareResult, only_flags: bool = False) -> str:
    """Excel (Türkçe bölge ayarı) için ; ayraçlı, BOM'lu CSV metni."""
    buf = io.StringIO()
    w = csv.writer(buf, delimiter=";", lineterminator="\r\n")
    w.writerow(["Grup", "Tür", "Kaynak zamanı", "Aday satır(lar)ı", "Aday zamanı", "Kaynak", "Aday", "Bayraklar"])
    for k, g in enumerate(res.groups, 1):
        if only_flags and not g.flags:
            continue
        w.writerow([k, g.kind, _times(g.src), ",".join(str(c.row + 1) for c in g.cand), _times(g.cand),
                    g.src_text(), g.cand_text(), " | ".join(f.label() for f in g.flags)])
    return "\ufeff" + buf.getvalue()


_HTML_CSS = """
:root { --bg:#ffffff; --fg:#1d1e22; --muted:#6b6c73; --line:#e3e4e8; --row:#f7f8fa; --head:#eef0f4;
        --chip:#e8e9ec; --err:#b3261e; --warn:#8a5a00; --info:#285ea8; }
@media (prefers-color-scheme: dark) {
  :root { --bg:#16171a; --fg:#e3e3e6; --muted:#9a9ba3; --line:#2c2d33; --row:#1b1c20; --head:#22242a;
          --chip:#2a2b30; --err:#ff9b9b; --warn:#ffd27a; --info:#8ab4ff; } }
body { margin:0; padding:16px; background:var(--bg); color:var(--fg);
       font:14px/1.45 "Segoe UI", system-ui, -apple-system, sans-serif; }
h1 { font-size:20px; margin:0 0 8px; }
.sum { color:var(--muted); margin:0 0 4px; }
.warn { color:var(--warn); font-weight:600; }
.bar { position:sticky; top:0; background:var(--bg); padding:8px 0; border-bottom:1px solid var(--line);
       display:flex; flex-wrap:wrap; gap:6px; z-index:1; }
.bar button { border:1px solid var(--line); background:var(--chip); color:var(--fg); border-radius:14px;
              padding:3px 10px; cursor:pointer; font:inherit; }
.bar button.on { outline:2px solid var(--info); }
table { border-collapse:collapse; width:100%; margin-top:8px; table-layout:fixed; }
th, td { border-bottom:1px solid var(--line); padding:6px 8px; vertical-align:top; text-align:left;
         overflow-wrap:anywhere; }
th { background:var(--head); position:sticky; top:46px; }
tr:nth-child(even) td { background:var(--row); }
td.n { width:3.5em; color:var(--muted); } td.k { width:3em; color:var(--muted); }
td.t { width:9.5em; color:var(--muted); font-variant-numeric:tabular-nums; font-size:12px; }
.chip { display:inline-block; border-radius:10px; padding:1px 8px; margin:0 4px 3px 0; font-size:12px;
        font-weight:600; background:var(--chip); }
.c-EKSİK, .c-FAZLA { color:var(--err); } .c-BOŞ, .c-İNGİLİZCE\\? { color:var(--err); }
.c-UZUNLUK, .c-ZAMAN { color:var(--warn); } .c-SAYI { color:var(--info); }
.d { color:var(--muted); font-size:12px; }
.row { color:var(--muted); font-size:12px; margin-right:4px; }
"""

_HTML_JS = """
const rows = [...document.querySelectorAll('tbody tr')];
let cur = 'all';
function show(code) {
  cur = code;
  document.querySelectorAll('.bar button').forEach(b => b.classList.toggle('on', b.dataset.f === code));
  rows.forEach(r => {
    const f = r.dataset.flags;
    r.style.display = code === 'all' ? '' : code === 'flagged' ? (f ? '' : 'none')
                     : (f.split(' ').includes(code) ? '' : 'none');
  });
}
document.querySelectorAll('.bar button').forEach(b => b.addEventListener('click', () => show(b.dataset.f)));
show(document.body.dataset.start);
"""


def to_html(res: CompareResult, src_name: str, cand_name: str, only_flags: bool = False) -> str:
    esc = html.escape
    c = res.counts()
    out = ["<!doctype html><html lang='tr'><head><meta charset='utf-8'>",
           "<meta name='viewport' content='width=device-width, initial-scale=1'>",
           f"<title>Karşılaştırma — {esc(os.path.basename(cand_name))}</title>",
           f"<style>{_HTML_CSS}</style></head>",
           f"<body data-start='{'flagged' if only_flags else 'all'}'>",
           f"<h1>Kaynak ↔ aday karşılaştırması</h1>"]
    for ln in summary_lines(res, os.path.basename(src_name), os.path.basename(cand_name)):
        cls = "sum warn" if ln.startswith("UYARI") else "sum"
        out.append(f"<p class='{cls}'>{esc(ln)}</p>")
    out.append("<div class='bar'>")
    out.append(f"<button data-f='all'>Tümü ({len(res.groups)})</button>")
    out.append(f"<button data-f='flagged'>Bayraklı ({len(res.flagged())})</button>")
    for k in FLAG_ORDER:
        if c.get(k):
            out.append(f"<button data-f='{esc(k)}' title='{esc(FLAG_HELP[k])}'>{esc(k)} ({c[k]})</button>")
    out.append("</div>")
    out.append("<table><thead><tr><th style='width:3.5em'>#</th><th style='width:3em'>Tür</th>"
               "<th style='width:9.5em'>Zaman (kaynak)</th><th>Kaynak</th><th>Aday</th>"
               "<th style='width:22%'>Bayraklar</th></tr></thead><tbody>")
    for k, g in enumerate(res.groups, 1):
        if only_flags and not g.flags:
            continue
        src = "<br>".join(esc(x.plain) for x in g.src)
        cand = "<br>".join(f"<span class='row'>{x.row + 1}</span>{esc(x.plain) or '<i>(boş)</i>'}"
                           for x in g.cand)
        chips = "".join(f"<div><span class='chip c-{esc(f.code)}'>{esc(f.code)}</span>"
                        f"<span class='d'>{esc(f.detail)}</span></div>" for f in g.flags)
        out.append(f"<tr data-flags='{esc(' '.join(dict.fromkeys(g.codes)))}'><td class='n'>{k}</td>"
                   f"<td class='k'>{g.kind}</td><td class='t'>{esc(_times(g.src) or _times(g.cand))}</td>"
                   f"<td>{src}</td><td>{cand}</td><td>{chips}</td></tr>")
    out.append(f"</tbody></table><script>{_HTML_JS}</script></body></html>")
    return "\n".join(out)


def marked_document(cand_doc, res: CompareResult):
    """Adayın işaretli kopyası: bayraklı satırlara {QC: …}, EKSİK kaynak satırları sona Comment.
    Özgün belgeye dokunulmaz (kopyanın olay listesi ayrıdır; dokunulmayan satırlar bayt bayt aynı)."""
    from .assfile import Event
    from .asstime import round_ms
    doc = cand_doc.clone()
    evs = doc.events
    for row, flags in sorted(res.flags_by_cand_row().items()):
        if 0 <= row < len(evs):
            body = "; ".join(f.label() for f in flags).replace("{", "(").replace("}", ")")
            evs[row] = evs[row].replace(text="{QC: " + body + "}" + evs[row].text)
    styles = {}
    for e in evs:
        if not e.comment:
            styles[e.style] = styles.get(e.style, 0) + 1
    style = max(styles, key=styles.get) if styles else "Default"
    for g in res.groups:
        if g.src and not g.cand:
            for s in g.src:
                evs.append(Event(comment=True, start=round_ms(s.start + res.shift_ms),
                                 end=round_ms(s.end + res.shift_ms), style=style, text="{QC: EKSİK}" + s.text))
    return doc


__all__ = ["CompareOptions", "CompareResult", "Group", "Flag", "Line", "compare", "find_shift",
           "build_groups", "summary_lines", "to_csv", "to_html", "marked_document", "FLAG_ORDER",
           "FLAG_HELP", "english_words", "similarity", "plain_of", "is_typeset"]
