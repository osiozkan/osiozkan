"""Zamanlama son işlemcisi (Aegisub "Timing Post-Processor" karşılığı).

Sıra Aegisub ile aynı:
  1. Giriş / çıkış payı (lead-in / lead-out)
  2. Bitişik satırları birleştir: aradaki boşluk ya da çakışma eşik içindeyse
     iki satır ortak bir noktada birleşir; nokta `bias` ile seçilir
     (0 = öncekinin bitişi, 1 = sonrakinin başlangıcı).
  3. Anahtar kareye yaslama (kare cinsinden eşikler):
     - Başlangıç: en yakın anahtar kare, başlangıç karesinden en çok
       `start_before` kare önce ya da `start_after` kare sonraysa başlangıç oraya taşınır.
     - Bitiş: satırın göründüğü son kareden SONRAKİ kare (sınır) ele alınır;
       anahtar kare sınırdan `end_before` kare önce ya da `end_after` kare sonraysa
       satır o anahtar kareden hemen önceki karede biter.

Saf işlev: belgeye dokunmaz, {satır: {"start":.., "end":..}} döndürür.
"""

from __future__ import annotations

import bisect
from dataclasses import dataclass, field

from .framemap import END, START


@dataclass
class TimingOptions:
    lead_in: int = 0                 # ms
    lead_out: int = 0                # ms
    adjacent: bool = False
    adjacent_threshold: int = 300    # ms (boşluk ya da çakışma)
    adjacent_bias: float = 0.5
    keyframes: bool = False
    start_before: int = 5            # kare (Aegisub varsayılanları)
    start_after: int = 4
    end_before: int = 5
    end_after: int = 11
    styles: set[str] | None = None   # None = tümü
    skip_comments: bool = True


@dataclass
class TimingResult:
    changes: dict[int, dict] = field(default_factory=dict)
    lead: int = 0
    adjacent: int = 0
    keyframe: int = 0


def postprocess(events, rows: list[int], opt: TimingOptions, frame_map=None) -> TimingResult:
    res = TimingResult()
    rows = [r for r in rows if 0 <= r < len(events)]
    if opt.skip_comments:
        rows = [r for r in rows if not events[r].comment]
    if opt.styles is not None:
        rows = [r for r in rows if events[r].style in opt.styles]
    times = {r: [events[r].start, events[r].end] for r in rows}

    # 1) giriş / çıkış payı
    if opt.lead_in or opt.lead_out:
        for r in rows:
            s, e = times[r]
            ns, ne = max(0, s - opt.lead_in), e + opt.lead_out
            if (ns, ne) != (s, e):
                res.lead += 1
            times[r] = [ns, ne]

    # 2) bitişik satırlar (başlangıca göre sıralı komşular)
    if opt.adjacent and len(rows) > 1:
        order = sorted(rows, key=lambda r: (times[r][0], times[r][1]))
        for a, b in zip(order, order[1:]):
            prev_end, cur_start = times[a][1], times[b][0]
            dist = cur_start - prev_end
            if dist == 0 or abs(dist) > opt.adjacent_threshold:
                continue
            split = prev_end + int(dist * opt.adjacent_bias + (0.5 if dist >= 0 else -0.5))
            # bir satırın süresi tersine dönmesin
            split = max(split, times[a][0])
            split = min(split, times[b][1])
            times[a][1] = split
            times[b][0] = split
            res.adjacent += 1

    # 3) anahtar karelere yaslama
    if opt.keyframes and frame_map is not None and frame_map.keyframes:
        kfs = sorted(frame_map.keyframes)
        fm = frame_map

        def nearest(f):
            i = bisect.bisect_left(kfs, f)
            cands = [kfs[j] for j in (i - 1, i) if 0 <= j < len(kfs)]
            return min(cands, key=lambda k: (abs(k - f), k))

        for r in rows:
            s, e = times[r]
            changed = False
            sf = fm.frame_at_time(s, START)
            k = nearest(sf)
            if (k < sf and sf - k <= opt.start_before) or (k > sf and k - sf <= opt.start_after):
                s = fm.time_at_frame(k, START)
                changed = True
            last = fm.frame_at_time(e, END)
            boundary = last + 1
            k = nearest(boundary)
            if (k < boundary and boundary - k <= opt.end_before) or (k > boundary and k - boundary <= opt.end_after):
                if k - 1 >= fm.frame_at_time(s, START):
                    e = fm.time_at_frame(k - 1, END)
                    changed = True
            if changed:
                res.keyframe += 1
            times[r] = [s, max(s, e)]

    for r in rows:
        s, e = times[r]
        if (s, e) != (events[r].start, events[r].end):
            res.changes[r] = {"start": s, "end": e}
    return res
