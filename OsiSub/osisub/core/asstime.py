"""ASS zaman biçimi: H:MM:SS.cc  <->  milisaniye (int).

Aegisub içeride milisaniye tutar ama dosyaya santisaniye yazar. Lua'ya
verilen değerler de milisaniyedir. Bir betik 1234 ms atarsa Aegisub bunu
en yakın 10 ms'ye yuvarlar (AssTime: (t + 5) - (t + 5) % 10); burada da öyle.
"""

from __future__ import annotations

MAX_MS = 10 * 3600 * 1000 - 10  # 9:59:59.99


def round_ms(ms: float | int) -> int:
    """Aegisub AssTime ile aynı yuvarlama ve sınırlama."""
    t = int(ms)
    t = (t + 5) - (t + 5) % 10
    if t < 0:
        return 0
    if t > MAX_MS:
        return MAX_MS
    return t


def parse_time(s: str) -> int:
    """'0:01:02.34' -> 62340. Hatalı/eksik biçimlerde Aegisub kadar hoşgörülü.

    Kabul edilenler: 'H:MM:SS.cc', 'H:MM:SS.c', 'H:MM:SS.ccc', 'MM:SS.cc', 'SS.cc'.
    """
    s = s.strip()
    if not s:
        return 0
    neg = s.startswith("-")
    if neg:
        s = s[1:]
    parts = s.split(":")
    try:
        sec_part = parts[-1]
        if "." in sec_part:
            sec_s, frac_s = sec_part.split(".", 1)
        else:
            sec_s, frac_s = sec_part, ""
        sec = int(sec_s or 0)
        frac_s = (frac_s + "000")[:3]
        ms = int(frac_s) if frac_s.isdigit() else 0
        minutes = int(parts[-2]) if len(parts) >= 2 and parts[-2] else 0
        hours = int(parts[-3]) if len(parts) >= 3 and parts[-3] else 0
    except ValueError:
        return 0
    total = ((hours * 60 + minutes) * 60 + sec) * 1000 + ms
    if neg:
        return 0
    return round_ms(total)


def format_time(ms: int) -> str:
    """62340 -> '0:01:02.34' (her zaman santisaniye)."""
    ms = round_ms(ms)
    cs = ms // 10
    h, cs = divmod(cs, 360000)
    m, cs = divmod(cs, 6000)
    s, cs = divmod(cs, 100)
    return f"{h}:{m:02d}:{s:02d}.{cs:02d}"


def format_duration(ms: int) -> str:
    """Süre kutusu için: '2.35' gibi saniye gösterimi değil, zaman biçimi."""
    return format_time(max(0, ms))
