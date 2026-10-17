"""Konuşma algılama (VAD) — model gerektirmez, ses bölmesinin PCM'i üzerinde çalışır.

Yöntem (bsync'teki fikirlerin sade hali):
  * 10 ms adımla 512 örneklik pencereler, log enerji (dB) ve 250–3400 Hz bant oranı.
  * Gürültü tabanı = enerjinin 10. yüzdeliği; konuşma = taban + `on_db` üstü VE bant oranı
    `min_ratio` üstü; çıkış için taban + `off_db` (histerezis).
  * 5 karelik ortanca süzgeç, `min_speech` altı adacıklar atılır, `merge_gap` altı boşluklar birleşir.
Sonuç: [(başlangıç_ms, bitiş_ms), ...]. 24 dakikalık bölüm ~1–2 sn.

Sınır (bilerek söylenir): müzik altında konuşma ve bağırma/fısıltı karışık sahnelerde
eşik tek başına yetmez; sonuçlar YASLAMA önerisi içindir, kör uygulanmamalı.
"""

from __future__ import annotations

import numpy as np


def speech_segments(pcm: np.ndarray, rate: int, on_db: float = 12.0, off_db: float = 7.0,
                    min_ratio: float = 0.35, min_speech: int = 120, merge_gap: int = 200) -> list[tuple[int, int]]:
    hop = max(1, rate // 100)
    win = 512
    x = pcm.astype(np.float32) / 32768.0
    n = (len(x) - win) // hop + 1
    if n <= 0:
        return []
    energy = np.empty(n, np.float32)
    ratio = np.empty(n, np.float32)
    freqs = np.fft.rfftfreq(win, 1.0 / rate)
    band = (freqs >= 250) & (freqs <= 3400)
    w = np.hanning(win).astype(np.float32)
    from numpy.lib.stride_tricks import as_strided
    step = 8192
    for s in range(0, n, step):
        k = min(step, n - s)
        base = x[s * hop: s * hop + (k - 1) * hop + win]
        frames = as_strided(base, shape=(k, win), strides=(base.strides[0] * hop, base.strides[0]))
        spec = np.abs(np.fft.rfft(frames * w, axis=1)) ** 2
        tot = spec.sum(axis=1) + 1e-12
        energy[s:s + k] = 10 * np.log10(tot / win)
        ratio[s:s + k] = spec[:, band].sum(axis=1) / tot
    floor = float(np.percentile(energy, 10))
    on = (energy > floor + on_db) & (ratio > min_ratio)
    off = energy > floor + off_db
    active = np.zeros(n, bool)
    state = False
    for i in range(n):
        state = on[i] or (state and off[i])
        active[i] = state
    # ortanca süzgeç (5 kare)
    if n >= 5:
        pad = np.pad(active.astype(np.int8), 2, mode="edge")
        active = (np.lib.stride_tricks.sliding_window_view(pad, 5).sum(axis=1) >= 3)
    segs = []
    i = 0
    hop_ms = hop * 1000.0 / rate
    centre = win / 2 * 1000.0 / rate - hop_ms / 2      # pencere merkezine göre zaman
    while i < n:
        if active[i]:
            j = i
            while j < n and active[j]:
                j += 1
            segs.append([int(i * hop_ms + centre), int(j * hop_ms + centre)])
            i = j
        else:
            i += 1
    merged = []
    for a, b in segs:
        if merged and a - merged[-1][1] < merge_gap:
            merged[-1][1] = b
        else:
            merged.append([a, b])
    return [(max(0, a), b) for a, b in merged if b - a >= min_speech]


def snap_to_speech(events, rows, segs, max_start: int = 400, max_end: int = 400,
                   lead_in: int = 0, lead_out: int = 0) -> dict[int, dict]:
    """Satır başlangıcını en yakın konuşma başlangıcına, bitişini en yakın konuşma
    sonuna yaslar (pencere içinde ise). Döndürür: {satır: {"start", "end"}}."""
    if not segs:
        return {}
    onsets = np.array([a for a, _ in segs])
    offsets = np.array([b for _, b in segs])
    out = {}
    for r in rows:
        ev = events[r]
        if ev.comment:
            continue
        s, e = ev.start, ev.end
        k = int(np.argmin(np.abs(onsets - s)))
        if abs(onsets[k] - s) <= max_start:
            s = max(0, int(onsets[k]) - lead_in)
        k = int(np.argmin(np.abs(offsets - e)))
        if abs(offsets[k] - e) <= max_end:
            e = int(offsets[k]) + lead_out
        if e > s and (s, e) != (ev.start, ev.end):
            out[r] = {"start": s, "end": e}
    return out


def lines_from_speech(segs, existing, min_ms: int = 500, max_ms: int = 6000, style: str = "Default",
                      only_free: bool = True):
    """Konuşma bölümlerinden boş satırlar (sıfırdan zamanlama için)."""
    from ..core.assfile import Event
    busy = [(e.start, e.end) for e in existing if not e.comment]
    out = []
    for a, b in segs:
        if b - a < min_ms:
            continue
        pieces = max(1, int(np.ceil((b - a) / max_ms)))
        step = (b - a) / pieces
        for k in range(pieces):
            s, e = int(a + k * step), int(a + (k + 1) * step)
            if only_free:
                ov = sum(max(0, min(e, be) - max(s, bs)) for bs, be in busy)
                if ov > 0.5 * (e - s):
                    continue
            out.append(Event(start=s, end=e, style=style, text=""))
    return out


def speech_activity(pcm: np.ndarray, rate: int, thresholds=(8.0, 10.0, 12.0, 14.0, 16.0),
                    res_ms: int = 10) -> np.ndarray:
    """VAD topluluğu (bsync): birkaç açılış eşiğiyle bölümler bulunur, `res_ms` adımda çoğunluk oyu.
    Döndürür: float32 0/1 dizi (uzunluk = süre/res_ms + 1)."""
    total = int(len(pcm) * 1000 // rate)
    n = total // res_ms + 1
    votes = np.zeros(n, np.int16)
    for th in thresholds:
        for a, b in speech_segments(pcm, rate, on_db=th, off_db=max(3.0, th - 5.0)):
            votes[max(0, a // res_ms):min(n, b // res_ms)] += 1
    return (votes * 2 > len(thresholds)).astype(np.float32)
