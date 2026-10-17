"""Subtitle Edit'ten alınan toplu araçlar (arayüzsüz çekirdek).

Hepsi saf işlevdir: belgeyi değiştirmez, ya {satır: {alan: değer}} değişiklik sözlüğü ya da
yeni olay listesi döndürür; arayüz bunu tek geri al adımıyla uygular.

Metin araçları YALNIZ görünen metne dokunur: {…} etiket blokları korunur, \\p çizimli
satırlara hiç dokunulmaz. Türkçe büyük/küçük harf (i↔İ, ı↔I) doğru yapılır.

  auto_break / unbreak        Osi'nin CPL kuralıyla dengeli satır kırma (42) / kırmaları kaldırma
  change_case                 cümle düzeni, BÜYÜK, küçük, Baş Harfler
  remove_hi                   işitme engelli metni: [ses], (ses), ♪ şarkı ♪, KONUŞMACI: etiketleri
  adjust_durations            okuma hızına göre / yüzde / sabit; en az-en çok süre, sonraki satıra taşmaz
  bridge_gaps                 küçük boşlukları kapat (en az boşluk payı bırakarak)
  merge_short / split_long    kısa ardışık satırları birleştir / uzun satırları ikiye böl
  change_fps / change_speed / point_sync     kare hızı, hız (%), iki (ya da daha çok) noktalı doğrusal senkron
  sort_rows                   çok anahtarlı kararlı sıralama
  statistics                  sayımlar, süreler, CPS/CPL dağılımı, sık kelimeler
"""

from __future__ import annotations

import re
import unicodedata
from collections import Counter
from dataclasses import dataclass

from .asstime import round_ms
from .qc import map_plain, tr_lower, tr_upper
from .textutil import CPL_LIMIT, char_count, cpl_lengths, cps, cps_chars, strip_tags, visible_lines

_DRAW = re.compile(r"\\p[1-9]")
_TOKEN = re.compile(r"\{[^}]*\}|\\[Nnh]|.", re.S)


def has_drawing(text: str) -> bool:
    return bool(_DRAW.search(text))


# ---------------------------------------------------------------------------
# Etiket maskeleme: {…} blokları özel kullanım alanı karakterleriyle değiştirilir,
# böylece düzenli ifadeler etiketlerin içine (\pos(…), \t(…)) bakmaz.
# ---------------------------------------------------------------------------

_PUA0 = 0xE000


def mask_tags(text: str) -> tuple[str, list[str]]:
    blocks: list[str] = []

    def rep(m):
        blocks.append(m.group(0))
        return chr(_PUA0 + len(blocks) - 1)
    return re.sub(r"\{[^}]*\}", rep, text), blocks


def unmask(masked: str, blocks: list[str]) -> str:
    return "".join(blocks[ord(ch) - _PUA0] if _PUA0 <= ord(ch) < _PUA0 + len(blocks) else ch for ch in masked)


_MASKCH = "-"


# ---------------------------------------------------------------------------
# Satır kırma
# ---------------------------------------------------------------------------

# Türkçe: soru eki ve bağlaç "de/da", "ki" bir önceki sözcüğe bağlıdır — önünden kırılmaz
_NO_BREAK_BEFORE = frozenset("""mi mı mu mü misin mısın musun müsün miyim mıyım muyum müyüm miyiz mıyız muyuz
müyüz misiniz mısınız musunuz müsünüz midir mıdır mudur müdür de da ki""".split())
# bağlaçların ÖNÜNDEN kırmak doğal
_BREAK_BEFORE = frozenset("ve ama fakat ancak çünkü yoksa veya ya eğer yani hatta oysa lakin and but or because".split())
# sonraki sözcüğü niteleyenlerden SONRA kırmak kötü
_NO_BREAK_AFTER = frozenset("bir bu şu o çok en daha her hiç pek ne biraz the a an to of".split())


def _segments(text: str):
    """(tür, ham) dizisi: tag | nl (\\N, \\n) | ch (görünen karakter, \\h dahil)."""
    out = []
    for m in _TOKEN.finditer(text):
        t = m.group(0)
        if t.startswith("{"):
            out.append(("tag", t))
        elif t in ("\\N", "\\n"):
            out.append(("nl", t))
        else:
            out.append(("ch", t))
    return out


def _visible(segs) -> str:
    return "".join(" " if r == "\\h" else r for k, r in segs if k == "ch")


def _is_dialog_break(segs, i) -> bool:
    """i. öğe bir \\N ve arkasından (etiketler/boşluk geçilerek) '-' geliyorsa."""
    for k, r in segs[i + 1:]:
        if k == "tag" or (k == "ch" and r == " "):
            continue
        return k == "ch" and r in "-–—"
    return False


def unbreak(text: str) -> str:
    """\\N / \\n kırmalarını boşluğa çevirir (diyalog çizgisiyle başlayan satırların kırması kalır)."""
    if has_drawing(text):
        return text
    segs = _segments(text)
    out = []
    for i, (k, r) in enumerate(segs):
        if k == "nl" and not _is_dialog_break(segs, i):
            out.append(("ch", " "))
        else:
            out.append((k, r))
    return _collapse_spaces("".join(r for _k, r in out))


def _collapse_spaces(text: str) -> str:
    def fn(p, first, last):
        p = re.sub(r" {2,}", " ", p)
        p = re.sub(r" *\\N *", r"\\N", p)
        if first:
            p = p.lstrip(" ")
        if last:
            p = p.rstrip(" ")
        return p
    return map_plain(text, fn)


def _word_at(vis: str, i: int, forward: bool) -> str:
    if forward:
        m = re.match(r"\s*([^\s]+)", vis[i:])
    else:
        m = re.search(r"([^\s]+)\s*$", vis[:i])
    return (m.group(1) if m else "").strip(".,!?;:…\"'“”()").casefold()


def break_cost(left: str, right: str, limit: int) -> float:
    """Kırma yerinin maliyeti (küçük = iyi). Ödül ve cezalar toplam uzunlukla orantılıdır:
    noktalama ödülü, satırların biraz dengesiz olmasını göze aldırır ama aşırı dengesizliği değil."""
    la, lb = char_count(left.rstrip()), char_count(right.lstrip())
    total = la + lb
    cost = float(abs(la - lb))
    over = max(0, la - limit) + max(0, lb - limit)
    if over:
        cost += 1000 + 10 * over
    if la > lb:
        cost += 0.5                       # eşitlikte alt satırın uzun olması (piramit) yeğlenir
    lt, rt = left.rstrip(), right.lstrip()
    if rt[:1] in "-–—":
        cost -= total + 25                # ikinci konuşmacının çizgisinden önce
    if lt.endswith(("...", "…")) or lt[-1:] in ".!?":
        cost -= 0.45 * total
    elif lt[-1:] in ",;:":
        cost -= 0.3 * total
    nxt = _word_at(right, 0, True)
    prv = _word_at(left, len(left), False)
    if nxt in _NO_BREAK_BEFORE:
        cost += 0.5 * total
    elif nxt in _BREAK_BEFORE:
        cost -= 0.08 * total
    if prv in _NO_BREAK_AFTER:
        cost += 0.15 * total
    return cost


def auto_break(text: str, limit: int = CPL_LIMIT, force: bool = False) -> str:
    """Osi'nin CPL kuralına göre dengeli kırma.

    * Hiçbir satır `limit`'i aşmıyorsa dokunulmaz (force=True: yeniden dengelenir; tek satıra
      sığıyorsa kırma kaldırılır).
    * Diyalog çizgili (\\N-) satırların yapısı bozulmaz.
    * Kırma yeri: iki satırın uzunluk farkı en az; noktalamadan sonra, bağlaçtan önce yeğlenir;
      soru eki/de-da/ki'nin önünden ve niteleyicilerin arkasından kırılmaz.
    """
    if has_drawing(text):
        return text
    lens = cpl_lengths(text)
    if not force and max(lens or [0]) <= limit:
        return text
    segs = _segments(text)
    if any(k == "nl" and _is_dialog_break(segs, i) for i, (k, _r) in enumerate(segs)):
        return text
    flat = unbreak(text)
    if char_count(_visible(_segments(flat)).strip()) <= limit:
        return flat
    segs = _segments(flat)
    vis_idx = []              # her görünen karakterin segs içindeki konumu
    for i, (k, r) in enumerate(segs):
        if k == "ch":
            vis_idx.append(i)
    vis = _visible(segs)
    best, best_cost = None, None
    for vi, ch in enumerate(vis):
        if ch != " ":
            continue
        c = break_cost(vis[:vi], vis[vi + 1:], limit)
        if best_cost is None or c < best_cost:
            best, best_cost = vi, c
    if best is None:
        return text
    i = vis_idx[best]
    segs[i] = ("nl", "\\N")
    return _collapse_spaces("".join(r for _k, r in segs))


# ---------------------------------------------------------------------------
# Büyük / küçük harf
# ---------------------------------------------------------------------------

CASE_MODES = {"sentence": "Cümle düzeni", "upper": "BÜYÜK HARF", "lower": "küçük harf", "title": "Baş Harfler Büyük"}


def change_case(text: str, mode: str) -> str:
    if has_drawing(text):
        return text
    if mode == "upper":
        return map_plain(text, lambda p, f, l: _keep_escapes(p, tr_upper))
    if mode == "lower":
        return map_plain(text, lambda p, f, l: _keep_escapes(p, tr_lower))
    if mode == "title":
        def title(p):
            return re.sub(r"[^\W\d_]+(?:['’][^\W\d_]+)*",
                          lambda m: tr_upper(m.group(0)[0]) + tr_lower(m.group(0)[1:]), p)
        return map_plain(text, lambda p, f, l: _keep_escapes(p, title))
    if mode == "sentence":
        state = {"cap": True}

        def sent(p):
            out = []
            for ch in tr_lower(p):
                if ch.isalpha():
                    out.append(tr_upper(ch) if state["cap"] else ch)
                    state["cap"] = False
                else:
                    out.append(ch)
                    if ch in ".!?…":
                        state["cap"] = True
            return "".join(out)
        return map_plain(text, lambda p, f, l: _keep_escapes(p, sent))
    raise ValueError(mode)


def _keep_escapes(p: str, fn) -> str:
    """\\N \\n \\h kaçışları harf dönüşümünden korunur."""
    parts = re.split(r"(\\[Nnh])", p)
    return "".join(x if re.fullmatch(r"\\[Nnh]", x or "") else fn(x) for x in parts)


# ---------------------------------------------------------------------------
# İşitme engelli (HI) metni
# ---------------------------------------------------------------------------

@dataclass
class HiOptions:
    brackets: bool = True            # [kapı çarpar]
    parens: bool = True              # (güler)
    music: bool = True               # ♪ şarkı sözü ♪ / # … #
    speaker: bool = True             # ALİ: Merhaba
    speaker_mixed_case: bool = False  # Ali: Merhaba (Türkçe cümlede "Not: …" gibi yanlış alarm riski)
    remove_empty: bool = True


_UP = "A-ZÇĞİÖŞÜ"


def remove_hi(text: str, o: HiOptions | None = None) -> str:
    """HI öğelerini atar; satır tamamen boşalırsa '' döner (çağıran satırı silebilir)."""
    o = o or HiOptions()
    if has_drawing(text):
        return text
    m, blocks = mask_tags(text)
    if o.brackets:
        m = re.sub(r"\[[^\[\]]*\]", "", m)
    if o.parens:
        m = re.sub(r"\([^()]*\)", "", m)
    if o.music:
        m = re.sub(r"[♪♫#][^♪♫#]*[♪♫#]", "", m)
        m = re.sub(r"[♪♫]", "", m)
    if o.speaker:
        name = (rf"[{_UP}][\w .'’\-]{{0,29}}?" if o.speaker_mixed_case
                else rf"[{_UP}][{_UP}0-9 .'’\-]{{0,29}}[{_UP}0-9]|[{_UP}]")
        m = re.sub(rf"(^|\\N)([{_MASKCH}\s]*-?\s*)(?:{name})\s*:(?!\d)\s*", r"\1\2", m)
    # temizlik: boş satırlar, yalnız kalan diyalog çizgileri
    lines = m.split("\\N")
    kept = [ln for ln in lines if re.sub(rf"[{_MASKCH}\s\-–—]", "", ln.replace("\\h", ""))]
    if not kept:
        tags = "".join(ch for ch in m if _PUA0 <= ord(ch) < _PUA0 + len(blocks))
        return unmask(tags, blocks) if not o.remove_empty else ""
    if len(kept) == 1:
        kept[0] = re.sub(rf"^([{_MASKCH}\s]*)[-–—]\s*", r"\1", kept[0])
    # silinen satırlardaki etiketler kaybolmasın: baştaki bloklar ilk satıra taşınır
    lost = "".join(ch for ln in lines if ln not in kept for ch in ln if _PUA0 <= ord(ch) < _PUA0 + len(blocks))
    out = unmask("\\N".join(kept), blocks)
    if lost and not out.startswith(unmask(lost, blocks)):
        out = unmask(lost, blocks) + out
    return _collapse_spaces(out)


# ---------------------------------------------------------------------------
# Zamanlama araçları
# ---------------------------------------------------------------------------

def _order(events, rows):
    return sorted((r for r in rows if not events[r].comment), key=lambda r: (events[r].start, r))


@dataclass
class DurationOptions:
    mode: str = "cps"                # cps | percent | fixed
    cps: float = 15.0                # hedef okuma hızı (mode=cps)
    percent: float = 100.0
    fixed_ms: int = 0
    min_ms: int = 700
    max_ms: int = 7000
    min_gap_ms: int = 84             # sonraki satıra en az bu kadar boşluk
    only_extend: bool = True         # süreyi asla kısaltma


def adjust_durations(events, rows, o: DurationOptions, frame_map=None) -> dict[int, dict]:
    order = _order(events, rows)
    out = {}
    for k, r in enumerate(order):
        ev = events[r]
        dur = ev.end - ev.start
        if o.mode == "cps":
            want = int(round(cps_chars(ev.text) * 1000.0 / max(0.1, o.cps)))
        elif o.mode == "percent":
            want = int(round(dur * o.percent / 100.0))
        else:
            want = dur + int(o.fixed_ms)
        want = max(o.min_ms, min(o.max_ms, want))
        if o.only_extend:
            want = max(want, dur)
        end = ev.start + want
        if k + 1 < len(order):
            lim = events[order[k + 1]].start - o.min_gap_ms
            if end > lim:
                end = lim
                if o.only_extend:
                    end = max(end, ev.end)        # zaten taşıyorsa kısaltılmaz
        if end <= ev.start:
            continue
        if frame_map is not None:
            end = frame_map.snap(end)
        end = round_ms(end)
        if end != ev.end and end > ev.start:
            out[r] = {"end": end}
    return out


def bridge_gaps(events, rows, max_gap_ms: int = 500, min_gap_ms: int = 0, bias: float = 1.0) -> dict[int, dict]:
    """Ardışık iki satır arasındaki boşluk max_gap_ms'den küçükse kapatır; min_gap_ms kadar boşluk bırakır.
    bias: boşluğun ne kadarını önceki satır alır (1 = önceki uzar, 0 = sonraki erken başlar)."""
    order = _order(events, rows)
    out: dict[int, dict] = {}
    for a, b in zip(order, order[1:]):
        ea, eb = events[a], events[b]
        end_a = out.get(a, {}).get("end", ea.end)
        gap = eb.start - end_a
        if gap <= min_gap_ms or gap >= max_gap_ms:
            continue
        close = gap - min_gap_ms
        new_end = round_ms(end_a + close * bias)
        new_start = round_ms(new_end + min_gap_ms)
        if new_end != end_a:
            out.setdefault(a, {})["end"] = new_end
        if new_start != eb.start and new_start < eb.end:
            out.setdefault(b, {})["start"] = new_start
    return out


def change_fps(events, rows, src_fps: float, dst_fps: float) -> dict[int, dict]:
    """Kare hızı dönüşümü: aynı kare numaraları yeni hızda gösterilir (t' = t · src/dst)."""
    k = src_fps / dst_fps
    return _scale(events, rows, k, 0)


def change_speed(events, rows, percent: float) -> dict[int, dict]:
    """Hız (%): 100 = değişmez; 104.271 ≈ 23.976 → 25 (PAL hızlanması). t' = t · 100 / hız."""
    return _scale(events, rows, 100.0 / percent, 0)


def _scale(events, rows, k, b):
    out = {}
    for r in rows:
        ev = events[r]
        s, e = round_ms(ev.start * k + b), round_ms(ev.end * k + b)
        if (s, e) != (ev.start, ev.end):
            out[r] = {"start": s, "end": max(s, e)}
    return out


def point_sync(events, rows, points: list[tuple[int, int]]) -> dict[int, dict]:
    """Parçalı doğrusal eşleme: points = [(eski_ms, yeni_ms), …] (en az 1).
    Tek nokta: düz kaydırma. İki ya da daha çok: noktalar arasında doğrusal, dışında uç parçaların uzantısı."""
    pts = sorted(set(points))
    if not pts:
        return {}
    if len(pts) == 1:
        d = pts[0][1] - pts[0][0]
        return _scale(events, rows, 1.0, d)

    def f(t):
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            if t <= x1 or (x1, y1) == pts[-1]:
                if x1 == x0:
                    return y0 + (t - x0)
                return y0 + (t - x0) * (y1 - y0) / (x1 - x0)
        return t
    out = {}
    for r in rows:
        ev = events[r]
        s, e = round_ms(max(0, f(ev.start))), round_ms(max(0, f(ev.end)))
        if (s, e) != (ev.start, ev.end):
            out[r] = {"start": s, "end": max(s, e)}
    return out


# ---------------------------------------------------------------------------
# Yapısal araçlar: birleştir / böl / sırala
# ---------------------------------------------------------------------------

@dataclass
class MergeOptions:
    max_gap_ms: int = 250
    max_chars: int = 2 * CPL_LIMIT     # birleşik görünen metin (kırmalar dahil değil)
    max_duration_ms: int = 7000
    same_style: bool = True
    same_actor: bool = True
    limit: int = CPL_LIMIT


def merge_short(events, rows, o: MergeOptions | None = None) -> tuple[list, int]:
    """Kısa ardışık satırları birleştirir; birleşen metin Osi kuralıyla dengeli kırılır.
    Döndürür: (yeni olay listesi, birleştirme sayısı)."""
    o = o or MergeOptions()
    order = _order(events, rows)
    absorbed: dict[int, list[int]] = {}
    gone: set[int] = set()
    head = None
    for r in order:
        ev = events[r]
        if head is not None:
            h = events[head]
            parts = [head] + absorbed.get(head, [])
            last = events[parts[-1]]
            text = " ".join(strip_tags(events[p].text).replace("\\N", " ") for p in parts + [r])
            ok = (0 <= ev.start - last.end <= o.max_gap_ms
                  and char_count(text) <= o.max_chars
                  and ev.end - h.start <= o.max_duration_ms
                  and (not o.same_style or ev.style == h.style)
                  and (not o.same_actor or ev.actor == h.actor)
                  and not has_drawing(ev.text) and not has_drawing(h.text))
            if ok:
                absorbed.setdefault(head, []).append(r)
                gone.add(r)
                continue
        head = r
    out = []
    for i, ev in enumerate(events):
        if i in gone:
            continue
        if i in absorbed:
            parts = [i] + absorbed[i]
            joined = " ".join(_join_piece(events[p].text, k == 0) for k, p in enumerate(parts))
            ev = ev.replace(text=auto_break(_collapse_spaces(joined), o.limit),
                            end=max(events[p].end for p in parts))
        out.append(ev)
    return out, sum(len(v) for v in absorbed.values())


def _join_piece(text: str, first: bool) -> str:
    t = unbreak(text)
    return t if first else re.sub(r"^\{[^}]*\}", "", t)      # sonraki satırların baş etiketleri düşer


@dataclass
class SplitOptions:
    limit: int = CPL_LIMIT
    max_lines: int = 2
    min_ms: int = 500


def split_long(events, rows, o: SplitOptions | None = None, frame_map=None) -> tuple[list, int]:
    """İki satıra sığmayan (görünen metin > 2·limit ya da max_lines'tan çok satır) altyazıları
    cümle sınırından ikiye böler; süre karakter oranıyla paylaşılır, parçalar dengeli kırılır."""
    o = o or SplitOptions()
    target = set(rows)
    out = []
    n = 0
    for i, ev in enumerate(events):
        if i not in target or ev.comment or has_drawing(ev.text):
            out.append(ev)
            continue
        vis = " ".join(x.strip() for x in visible_lines(ev.text))
        nlines = len(visible_lines(ev.text))
        if char_count(vis) <= 2 * o.limit and nlines <= o.max_lines:
            out.append(ev)
            continue
        pieces = _split_text(ev.text)
        if pieces is None:
            out.append(ev)
            continue
        a, b = pieces
        la, lb = char_count(strip_tags(a)), char_count(strip_tags(b))
        mid = ev.start + (ev.end - ev.start) * la // max(1, la + lb)
        mid = max(ev.start + o.min_ms, min(ev.end - o.min_ms, mid))
        if frame_map is not None:
            mid = frame_map.snap(mid)
        mid = round_ms(mid)
        if not (ev.start < mid < ev.end):
            out.append(ev)
            continue
        out.append(ev.replace(text=auto_break(a, o.limit), end=mid))
        out.append(ev.fresh().replace(text=auto_break(b, o.limit), start=mid))
        n += 1
    return out, n


def _split_text(text: str) -> tuple[str, str] | None:
    """Metni ortaya en yakın cümle sınırından (yoksa virgül, yoksa boşluk) ikiye ayırır.
    Baştaki etiket bloğu ikinci parçaya da kopyalanır."""
    flat = unbreak(text)
    m, blocks = mask_tags(flat)
    lead = re.match(rf"^[{_MASKCH}]*", m).group(0)
    body = m[len(lead):]
    if not body.strip():
        return None
    half = len(body) / 2
    best = None
    for rx, weight in ((r"(?<=[.!?…])\s+", 0), (r"(?<=[,;:])\s+", 0.15), (r"\s+", 0.35)):
        for mm in re.finditer(rx, body):
            score = abs(mm.start() - half) / max(1, len(body)) + weight
            if best is None or score < best[0]:
                best = (score, mm.start(), mm.end())
        if best is not None and best[0] < 0.3:
            break
    if best is None:
        return None
    a, b = body[:best[1]], body[best[2]:]
    return unmask(lead + a, blocks), unmask(lead + b, blocks)


SORT_KEYS = {
    "start": ("Başlangıç", lambda e: e.start),
    "end": ("Bitiş", lambda e: e.end),
    "duration": ("Süre", lambda e: e.end - e.start),
    "style": ("Stil", lambda e: e.style.casefold()),
    "actor": ("Aktör", lambda e: e.actor.casefold()),
    "effect": ("Efekt", lambda e: e.effect.casefold()),
    "layer": ("Katman", lambda e: e.layer),
    "text": ("Metin", lambda e: strip_tags(e.text).casefold()),
    "cps": ("CPS", lambda e: cps(e.text, e.end - e.start)),
}


def sort_rows(events, rows, keys: list[str], reverse: bool = False) -> list:
    """Seçili satırları kendi konumları içinde (kararlı) sıralar; diğer satırlar yerinde kalır."""
    rows = sorted(set(rows))
    funcs = [SORT_KEYS[k][1] for k in keys]
    picked = sorted((events[r] for r in rows), key=lambda e: tuple(f(e) for f in funcs), reverse=reverse)
    out = list(events)
    for r, ev in zip(rows, picked):
        out[r] = ev
    return out


# ---------------------------------------------------------------------------
# İstatistik
# ---------------------------------------------------------------------------

_TR_STOP = frozenset("""ve ile de da ki bu şu o bir ben sen biz siz onlar mi mı mu mü ne için gibi daha çok en ama
fakat ya hem her hiç şey diye kadar sonra önce bana sana ona bize size onu bunu şunu beni seni bizi sizi
evet hayır yok var değil artık zaten bile hep tüm olan olarak""".split())


def statistics(events, cpl_limit: int = CPL_LIMIT, cps_limit: float = 17.0) -> dict:
    dia = [e for e in events if not e.comment]
    words = Counter()
    n_words = n_chars = 0
    total_ms = 0
    cps_vals = []
    over_cpl = two_lines = 0
    styles = Counter()
    actors = Counter()
    for e in dia:
        plain = " ".join(visible_lines(e.text))
        ws = re.findall(r"[^\W\d_]+(?:['’][^\W\d_]+)?", plain)
        n_words += len(ws)
        n_chars += char_count(plain.replace(" ", ""))
        for w in ws:
            lw = tr_lower(w)
            if len(lw) > 2 and lw not in _TR_STOP:
                words[lw] += 1
        d = max(0, e.end - e.start)
        total_ms += d
        if d > 0 and strip_tags(e.text).strip():
            cps_vals.append(cps(e.text, d))
        lens = cpl_lengths(e.text)
        if max(lens or [0]) > cpl_limit:
            over_cpl += 1
        if len(lens) >= 2:
            two_lines += 1
        styles[e.style] += 1
        if e.actor:
            actors[e.actor] += 1
    cps_vals.sort()
    span = (max(e.end for e in dia) - min(e.start for e in dia)) if dia else 0
    return {
        "lines": len(events), "dialogue": len(dia), "comments": len(events) - len(dia),
        "words": n_words, "chars": n_chars, "total_ms": total_ms, "span_ms": span,
        "avg_ms": total_ms / len(dia) if dia else 0,
        "cps_avg": sum(cps_vals) / len(cps_vals) if cps_vals else 0.0,
        "cps_median": cps_vals[len(cps_vals) // 2] if cps_vals else 0.0,
        "cps_max": cps_vals[-1] if cps_vals else 0.0,
        "cps_over": sum(1 for v in cps_vals if v > cps_limit),
        "cpl_over": over_cpl, "two_lines": two_lines,
        "styles": styles.most_common(), "actors": actors.most_common(),
        "top_words": words.most_common(30),
    }


def normalize(s: str) -> str:
    return unicodedata.normalize("NFC", s)
