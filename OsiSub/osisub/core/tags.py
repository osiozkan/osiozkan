"""Override etiketlerini okuma/yazma (görsel tipografi araçları ve betikler için).

Kurallar (Aegisub görsel araçlarıyla aynı):
  * Araçlar satırın İLK override bloğunu düzenler; satır { ile başlamıyorsa
    başa yeni bir blok açılır.
  * \\t(...) içindeki etiketler okunmaz/değiştirilmez (yalnız üst düzey etiketler).
  * Aynı etiket blokta birden çok kez varsa sonuncusu geçerlidir (ASS anlamı);
    yazarken hepsi silinip tek bir tane bloğun sonuna eklenir.
  * Eş adlar: \\fr = \\frz, \\c = \\1c.
"""

from __future__ import annotations

import math
import re

# Uzun adlar önce (fscx, fs'den önce; clip, c'den önce ...)
_NAMES = sorted([
    "iclip", "clip", "move", "pos", "org", "fscx", "fscy", "fsp", "fs", "frx", "fry", "frz", "fr",
    "fax", "fay", "fn", "fe", "an", "alpha", "a", "1c", "2c", "3c", "4c", "1a", "2a", "3a", "4a", "c",
    "xbord", "ybord", "bord", "xshad", "yshad", "shad", "be", "blur", "b", "i", "u", "s", "q", "pbo",
    "p", "t", "fade", "fad", "kf", "ko", "kt", "k", "K", "r",
], key=len, reverse=True)

ALIASES = {"fr": "frz", "c": "1c"}


def canon(name: str) -> str:
    return ALIASES.get(name, name)


def first_block(text: str) -> tuple[int, int] | None:
    """İlk override bloğunun [başlangıç, bitiş) aralığı; satır { ile başlamıyorsa None."""
    if not text.startswith("{"):
        return None
    end = text.find("}")
    return (0, end + 1) if end > 0 else None


def tokens(block_inner: str) -> list[tuple[str, int, int, str]]:
    """Üst düzey etiketler: (ad, başlangıç, bitiş, argüman). \\t( ... ) içi tek token."""
    out = []
    i, n = 0, len(block_inner)
    while i < n:
        if block_inner[i] != "\\":
            i += 1
            continue
        start = i
        i += 1
        name = None
        for nm in _NAMES:
            if block_inner.startswith(nm, i):
                name = nm
                break
        if name is None:
            m = re.match(r"[A-Za-z0-9]+", block_inner[i:])
            name = m.group(0) if m else ""
        i += len(name)
        # argüman: parantezli ise eşleşen ')' ye kadar, değilse sonraki '\' ye kadar
        j = i
        while j < n and block_inner[j] == " ":
            j += 1
        if j < n and block_inner[j] == "(":
            depth = 0
            k = j
            while k < n:
                if block_inner[k] == "(":
                    depth += 1
                elif block_inner[k] == ")":
                    depth -= 1
                    if depth == 0:
                        k += 1
                        break
                k += 1
            i = k
        else:
            k = j
            while k < n and block_inner[k] != "\\":
                k += 1
            i = k
        out.append((name, start, i, block_inner[start + 1 + len(name):i]))
    return out


def get(text: str, name: str) -> str | None:
    """İlk bloktaki etiketin ham argümanı (sonuncusu)."""
    fb = first_block(text)
    if fb is None:
        return None
    inner = text[fb[0] + 1: fb[1] - 1]
    want = canon(name)
    val = None
    for nm, _s, _e, arg in tokens(inner):
        if canon(nm) == want:
            val = arg
    return val


def remove(text: str, name: str) -> str:
    fb = first_block(text)
    if fb is None:
        return text
    inner = text[fb[0] + 1: fb[1] - 1]
    want = canon(name)
    keep, pos = [], 0
    for nm, s, e, _arg in tokens(inner):
        if canon(nm) == want:
            keep.append(inner[pos:s])
            pos = e
    keep.append(inner[pos:])
    new_inner = "".join(keep)
    if not new_inner.strip():
        return text[fb[1]:]
    return "{" + new_inner + "}" + text[fb[1]:]


def set_tag(text: str, name: str, arg: str) -> str:
    """\\name+arg yaz (ör. set_tag(t, "pos", "(10,20)"), set_tag(t, "frz", "45"))."""
    name = canon(name)
    # eş adlı tüm biçimleri (\\fr, \\c) temizle
    text = remove(text, name)
    for alias, target in ALIASES.items():
        if target == name:
            text = remove(text, alias)
    if name in ("clip", "iclip"):
        text = remove(text, "iclip" if name == "clip" else "clip")
    tag = f"\\{name}{arg}"
    fb = first_block(text)
    if fb is None:
        return "{" + tag + "}" + text
    return text[: fb[1] - 1] + tag + text[fb[1] - 1:]


# ---------------------------------------------------------------------------
# Sayı biçimi ve ayrıştırıcılar
# ---------------------------------------------------------------------------

def fmt(v: float, decimals: int = 2) -> str:
    v = round(float(v), decimals)
    if v == int(v):
        s = str(int(v))
    else:
        s = f"{v:.{decimals}f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def _nums(arg: str | None) -> list[float]:
    if not arg:
        return []
    return [float(x) for x in re.findall(r"-?\d+(?:\.\d+)?", arg)]


def get_pos(text):
    v = _nums(get(text, "pos"))
    return (v[0], v[1]) if len(v) >= 2 else None


def get_move(text):
    v = _nums(get(text, "move"))
    if len(v) >= 4:
        return tuple(v[:4]) + ((int(v[4]), int(v[5])) if len(v) >= 6 else (None, None))
    return None


def set_move(text, x1, y1, x2, y2, t1=None, t2=None):
    arg = f"({fmt(x1)},{fmt(y1)},{fmt(x2)},{fmt(y2)}"
    if t1 is not None and t2 is not None:
        arg += f",{int(t1)},{int(t2)}"
    return set_tag(text, "move", arg + ")")


def get_org(text):
    v = _nums(get(text, "org"))
    return (v[0], v[1]) if len(v) >= 2 else None


def get_float(text, name, default=0.0):
    v = _nums(get(text, name))
    return v[0] if v else default


def get_clip(text):
    """("rect", inverse, (x1,y1,x2,y2)) | ("vector", inverse, (scale, drawing)) | None"""
    for name, inv in (("clip", False), ("iclip", True)):
        arg = get(text, name)
        if arg is None:
            continue
        inner = arg.strip()[1:-1] if arg.strip().startswith("(") else arg
        nums = inner.split(",")
        if len(nums) == 4 and all(re.fullmatch(r"\s*-?\d+(\.\d+)?\s*", x) for x in nums):
            return "rect", inv, tuple(float(x) for x in nums)
        m = re.match(r"\s*(\d+)\s*,(.*)$", inner, re.S)
        if m:
            return "vector", inv, (int(m.group(1)), m.group(2).strip())
        return "vector", inv, (1, inner.strip())
    return None


def set_rect_clip(text, x1, y1, x2, y2, inverse=False):
    x1, x2 = sorted((x1, x2))
    y1, y2 = sorted((y1, y2))
    arg = f"({fmt(x1, 0)},{fmt(y1, 0)},{fmt(x2, 0)},{fmt(y2, 0)})"
    return set_tag(text, "iclip" if inverse else "clip", arg)


# ---------------------------------------------------------------------------
# Vektör çizim (m/l/b/n/s/p/c)
# ---------------------------------------------------------------------------

def parse_drawing(d: str) -> list[list]:
    """[[komut, x, y], ...] — her nokta onu getiren komutu taşır."""
    pts = []
    cmd = None
    toks = re.findall(r"[mnlbspc]|-?\d+(?:\.\d+)?", d)
    nums = []
    for t in toks:
        if t.isalpha():
            cmd = t
            continue
        nums.append(float(t))
        if len(nums) == 2:
            pts.append([cmd or "m", nums[0], nums[1]])
            nums = []
    return pts


def drawing_str(pts: list[list], decimals: int = 0) -> str:
    out = []
    prev = None
    run = 0
    # b koşuları 3'ün katı değilse l'ye çevir
    i = 0
    fixed = [list(p) for p in pts]
    while i < len(fixed):
        if fixed[i][0] == "b":
            j = i
            while j < len(fixed) and fixed[j][0] == "b":
                j += 1
            if (j - i) % 3:
                for k in range(i, j):
                    fixed[k][0] = "l"
            i = j
        else:
            i += 1
    for c, x, y in fixed:
        if c != prev or c == "m":
            out.append(c)
            prev = c
            run = 0
        out.append(fmt(x, decimals))
        out.append(fmt(y, decimals))
        run += 1
    return " ".join(out)


def set_vector_clip(text, pts, inverse=False, scale=1):
    d = drawing_str(pts)
    arg = f"({scale},{d})" if scale and scale != 1 else f"({d})"
    return set_tag(text, "iclip" if inverse else "clip", arg)


# ---------------------------------------------------------------------------
# Konum
# ---------------------------------------------------------------------------

def alignment(text: str, style_align: int) -> int:
    an = get(text, "an")
    if an and an.strip().isdigit() and 1 <= int(an) <= 9:
        return int(an)
    a = get(text, "a")                      # eski SSA \a
    if a and a.strip().isdigit():
        v = int(a)
        legacy = {1: 1, 2: 2, 3: 3, 5: 7, 6: 8, 7: 9, 9: 4, 10: 5, 11: 6}
        if v in legacy:
            return legacy[v]
    return style_align or 2


def default_pos(ev, style, play_res) -> tuple[float, float]:
    """\\pos yokken satırın çapa noktası (stil hizası + kenar boşlukları)."""
    w, h = play_res
    al = alignment(ev.text, style.align if style else 2)
    ml = ev.margin_l or (style.margin_l if style else 10)
    mr = ev.margin_r or (style.margin_r if style else 10)
    mv = ev.margin_v or (style.margin_v if style else 10)
    col = (al - 1) % 3
    row = (al - 1) // 3          # 0 alt, 1 orta, 2 üst
    x = ml if col == 0 else (w - mr if col == 2 else (ml + (w - mr)) / 2)
    y = h - mv if row == 0 else (h / 2 if row == 1 else mv)
    return float(x), float(y)


def position(ev, style, play_res, t_ms: int | None = None) -> tuple[float, float]:
    """Satırın t anındaki konumu (\\pos, \\move ara değeri ya da varsayılan)."""
    p = get_pos(ev.text)
    if p:
        return p
    mv = get_move(ev.text)
    if mv:
        x1, y1, x2, y2, t1, t2 = mv
        if t_ms is None:
            return x1, y1
        rel = t_ms - ev.start
        if t1 is None:
            t1, t2 = 0, ev.end - ev.start
        if rel <= t1:
            k = 0.0
        elif rel >= t2 or t2 <= t1:
            k = 1.0
        else:
            k = (rel - t1) / (t2 - t1)
        return x1 + (x2 - x1) * k, y1 + (y2 - y1) * k
    return default_pos(ev, style, play_res)


def norm_angle(a: float) -> float:
    a = math.fmod(a, 360.0)
    if a > 180:
        a -= 360
    if a <= -180:
        a += 360
    return a
