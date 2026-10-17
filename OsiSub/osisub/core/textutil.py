"""Metin yardımcıları: etiket ayıklama, CPL (satır başına karakter), CPS.

CPL kuralları (Osi):
  * Sayım Türkçe karakterlerde doğru olmalı (NFC; birleşik işaretler ayrıca sayılmaz).
  * Kelimeler arası boşluk ve noktalama SAYILIR.
  * Tek satırlık altyazı 43 karaktere ulaşınca UYARI.
  * \\N ile bölünmüş altyazıda bir parça hâlâ 42'yi aşıyorsa HATA.
  * Gösterim: parça uzunlukları "+" ile: "43+16".
"""

from __future__ import annotations

import re
import unicodedata
from functools import lru_cache

CPL_LIMIT = 42          # izin verilen en uzun satır (varsayılan)
STATUS_OK, STATUS_WARN, STATUS_ERROR = "ok", "warn", "error"
_limit = CPL_LIMIT      # Ayarlar > Denetim > CPL sınırı (set_cpl_limit)


def set_cpl_limit(n: int):
    global _limit
    _limit = int(n) if n and int(n) > 0 else CPL_LIMIT


def cpl_limit() -> int:
    return _limit

_BLOCK_RE = re.compile(r"\{[^}]*\}")
_DRAW_RE = re.compile(r"\\p(\d+)")


def strip_tags(text: str) -> str:
    """{...} bloklarını atar; \\p1 çizim kipindeki komutları da atar."""
    out = []
    pos = 0
    drawing = False
    for m in _BLOCK_RE.finditer(text):
        if not drawing:
            out.append(text[pos:m.start()])
        for d in _DRAW_RE.finditer(m.group(0)):
            drawing = int(d.group(1)) > 0
        pos = m.end()
    if not drawing:
        out.append(text[pos:])
    return "".join(out)


def visible_lines(text: str) -> list[str]:
    """Ekranda görünecek satırlar. \\N satır böler; \\n (WrapStyle 2 değilse)
    ve \\h boşluk sayılır."""
    plain = strip_tags(text)
    plain = plain.replace("\\h", "\u00a0").replace("\\n", " ")
    return plain.split("\\N")


def char_count(s: str) -> int:
    """Türkçe doğru karakter sayımı: NFC + birleştirici işaretleri sayma."""
    s = unicodedata.normalize("NFC", s)
    return sum(1 for ch in s if unicodedata.category(ch) not in ("Mn", "Me", "Cf"))


def cpl_lengths(text: str) -> list[int]:
    return [char_count(seg) for seg in visible_lines(text)]


def cpl_info(text: str) -> dict:
    """{'lengths': [43, 16], 'total': 59, 'longest': 43, 'status': 'warn', 'label': '43+16'}
    Önbellekli (ızgara, CPL bölmesi, düzenleme kutusu ve denetim aynı metni tekrar tekrar sorar);
    sonuç paylaşılan sözlüktür — değiştirilmemelidir."""
    return _cpl_info(text, _limit)


@lru_cache(maxsize=65536)
def _cpl_info(text: str, limit: int) -> dict:
    lengths = cpl_lengths(text)
    total = sum(lengths)
    longest = max(lengths) if lengths else 0
    return {
        "lengths": lengths,
        "total": total,
        "longest": longest,
        "status": cpl_status(lengths, limit),
        "label": "+".join(str(n) for n in lengths),
    }


def cpl_status(lengths: list[int], limit: int | None = None) -> str:
    limit = _limit if limit is None else limit
    if not lengths:
        return STATUS_OK
    if len(lengths) > 1:
        return STATUS_ERROR if any(n > limit for n in lengths) else STATUS_OK
    return STATUS_WARN if lengths[0] > limit else STATUS_OK


def cps_chars(text: str) -> int:
    """CPS için sayılan karakterler: boşluk ve noktalama hariç (Aegisub varsayılanı)."""
    n = 0
    for seg in visible_lines(text):
        seg = unicodedata.normalize("NFC", seg)
        for ch in seg:
            cat = unicodedata.category(ch)
            if cat[0] in ("Z", "P", "C") or cat in ("Mn", "Me"):
                continue
            n += 1
    return n


def cps(text: str, duration_ms: int) -> float:
    if duration_ms <= 0:
        return 0.0
    return cps_chars(text) * 1000.0 / duration_ms


def collapse_tags(text: str, marker: str = "\u2600") -> str:
    """Izgarada gösterim için {...} bloklarını tek simgeye indirger (Aegisub ☀)."""
    return _BLOCK_RE.sub(marker, text)


def split_at_linebreak(text: str) -> tuple[str, str] | None:
    """İlk \\N'den ikiye böler (etiketler ilk parçada kalır)."""
    i = text.find("\\N")
    if i < 0:
        return None
    return text[:i], text[i + 2:]
