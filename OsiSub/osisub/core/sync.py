"""Konuşmaya göre senkron (bsync'in uygulama içi hali).

Osi'nin bsync kuralları (aynen):
  * Kayma bölüm boyunca SABİT DEĞİLDİR.
  * Kayma yalnız gerçek sahne aralarında (altyazıda ≥ 4 sn boşluk) değişir.
  * Ekran yazısı satırları (sonu `\\N \\N \\N`) hizalama çapası olmaz ama kendi parçasıyla birlikte kayar.
  * OP/ED vokalleri bastırılır (şarkı sesi konuşma sanılmasın).
  * Hiçbir satır 0'ın önüne ya da sesin sonundan sonraya düşmez.
  * Yalnız Start/End değişir (bayt koruması belge modelinde zaten var).

Boru hattı:
  1. Konuşma etkinliği (10 ms çözünürlük): VAD topluluğu (birkaç eşiğin çoğunluk oyu, media.vad).
  2. OP/ED bastırma: stil/aktör/efekt adı OP/ED/şarkı olan satırların aralıkları ve `oped_max_ms`'den
     uzun kesintisiz "konuşma" koşuları (şarkı) etkinlikten silinir.
  3. Parçalar: satırlar ≥ `scene_gap_ms` boşluklarda bölünür; bloklar ≥ `block_gap_ms` boşluklarda.
  4. Kaba kayma: her blok için çapa satırlarının etkinliği ile konuşmanın çapraz ilintisi (±`coarse_range_ms`).
  5. İnce hizalama (DP): her parçaya blok kaymasının ±`fine_range_ms` çevresinde `step_ms` adımlı bir kayma;
     puan = çapa satırlarının konuşmayla örtüşmesi (sessizlik cezalı), kayma değişimi `change_penalty` ile
     cezalı, parçaların sırası bozulamaz (bir parça sonrakinin içine kayamaz).
  6. Güven: en iyi puan ile ≥ 300 ms uzaktaki en iyi puan farkı / çapa süresi. Çapasız parçalar komşularını izler.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

import numpy as np

RES = 10        # ms / örnek


@dataclass
class SyncOptions:
    coarse_range_ms: int = 60000
    fine_range_ms: int = 8000
    step_ms: int = 40
    scene_gap_ms: int = 4000
    block_gap_ms: int = 60000
    change_penalty: float = 0.15     # 1 ms kayma değişimi = bu kadar ms örtüşme kaybı
    silence_weight: float = 0.3      # satır sessizliğe düşerse ms başına ceza
    blur_ms: int = 150               # konuşma etkinliğinin Gauss yumuşatması (σ)
    oped_regex: str = r"(?i)(^|[^a-z])(op|ed|opening|ending|song|şarkı|insert|karaoke)([^a-z]|$)"
    oped_max_ms: int = 45000
    rows: list | None = None         # yalnız bu satırlar (None: tüm belge)


@dataclass
class Segment:
    rows: list
    start: int
    end: int
    anchors: list
    anchor_ms: int = 0
    block: int = 0
    shift: int = 0
    confidence: float = 0.0
    notes: list = field(default_factory=list)


def is_screen_text(ev) -> bool:
    t = ev.text.rstrip()
    return bool(re.search(r"(\\N\s*){3}$", t))


def is_typeset(ev) -> bool:
    return bool(re.search(r"\\(pos|move|p[1-9])", ev.text))


def is_oped(ev, rx) -> bool:
    return bool(rx.search(ev.style or "") or rx.search(ev.actor or "") or rx.search(ev.effect or ""))


def activity_from_segments(segs, total_ms: int) -> np.ndarray:
    n = total_ms // RES + 1
    a = np.zeros(n, np.float32)
    for s, e in segs:
        a[max(0, s // RES):max(0, min(n, e // RES))] = 1.0
    return a


def suppress(act: np.ndarray, ranges, max_run_ms: int) -> tuple[np.ndarray, list]:
    """OP/ED aralıklarını ve aşırı uzun kesintisiz etkinlik koşularını sil."""
    a = act.copy()
    removed = []
    for s, e in ranges:
        a[max(0, s // RES):max(0, e // RES)] = 0
        removed.append((s, e, "OP/ED satırları"))
    if max_run_ms:
        # 300 ms'den kısa boşlukları birleştirerek koşuları bul
        on = a > 0.5
        n = len(on)
        i = 0
        gap_ok = 300 // RES
        while i < n:
            if not on[i]:
                i += 1
                continue
            j = i
            last = i
            while j < n:
                if on[j]:
                    last = j
                elif j - last > gap_ok:
                    break
                j += 1
            if (last - i + 1) * RES >= max_run_ms:
                a[i:last + 1] = 0
                removed.append((i * RES, (last + 1) * RES, "kesintisiz uzun ses (şarkı?)"))
            i = last + 1
    return a, removed


def build_segments(events, rows, opts: SyncOptions) -> list[Segment]:
    rx = re.compile(opts.oped_regex)
    items = sorted((r for r in rows if not events[r].comment), key=lambda r: (events[r].start, events[r].end))
    segs: list[Segment] = []
    cur, cur_end = [], None
    for r in items:
        ev = events[r]
        if cur and ev.start - cur_end >= opts.scene_gap_ms:
            segs.append(Segment(cur, events[cur[0]].start, cur_end, []))
            cur, cur_end = [], None
        cur.append(r)
        cur_end = ev.end if cur_end is None else max(cur_end, ev.end)
    if cur:
        segs.append(Segment(cur, events[cur[0]].start, cur_end, []))
    block = 0
    for i, sg in enumerate(segs):
        if i and sg.start - segs[i - 1].end >= opts.block_gap_ms:
            block += 1
        sg.block = block
        sg.anchors = [r for r in sg.rows if not (is_screen_text(events[r]) or is_typeset(events[r])
                                                 or is_oped(events[r], rx))]
        sg.anchor_ms = sum(events[r].end - events[r].start for r in sg.anchors)
    return segs


def _weighted(act, w):
    return act - w * (1.0 - act)


def _coarse(events, segs, speech_w, total_ms, opts):
    """Blok başına çapraz ilinti kayması (ms)."""
    out = {}
    R = opts.coarse_range_ms // RES
    n = len(speech_w)
    for b in sorted({s.block for s in segs}):
        bs = [s for s in segs if s.block == b]
        sub = np.zeros(n, np.float32)
        for s in bs:
            for r in s.anchors:
                ev = events[r]
                sub[max(0, ev.start // RES):max(0, min(n, ev.end // RES))] = 1.0
        if sub.sum() == 0:
            out[b] = 0
            continue
        # ilinti: c[lag] = sum sub[t] * speech[t + lag]; FFT ile
        m = 1 << int(np.ceil(np.log2(2 * n)))
        fs = np.fft.rfft(sub, m)
        fp = np.fft.rfft(speech_w, m)
        corr = np.fft.irfft(np.conj(fs) * fp, m)
        lags = np.arange(-R, R + 1)
        vals = corr[lags % m]
        out[b] = int(lags[int(np.argmax(vals))] * RES)
    return out


def _scores(events, seg, cands, cum, n, w_total):
    """Her aday kayma için parçanın puanı (ms): örtüşme − ağırlıklı sessizlik."""
    if not seg.anchors:
        return np.zeros(len(cands))
    sc = np.zeros(len(cands))
    c = (cands // RES).astype(np.int64)
    for r in seg.anchors:
        ev = events[r]
        a = np.clip(ev.start // RES + c, 0, n)
        b = np.clip(ev.end // RES + c, 0, n)
        sc += (cum[b] - cum[a])
    return sc * RES


def plan(events, speech_act: np.ndarray, total_ms: int, opts: SyncOptions | None = None,
         progress=None) -> tuple[list[Segment], list]:
    """Döndürür: (parçalar — her birinde önerilen `shift`, bastırılan aralıklar)."""
    opts = opts or SyncOptions()
    rows = opts.rows if opts.rows is not None else list(range(len(events)))
    segs = build_segments(events, rows, opts)
    if not segs:
        return [], []
    rx = re.compile(opts.oped_regex)
    oped = [(events[r].start, events[r].end) for r in rows if not events[r].comment and is_oped(events[r], rx)]
    act, removed = suppress(speech_act, oped, opts.oped_max_ms)
    # yumuşatma: örtüşme platosunu (satırlar konuşmadan biraz geniş zamanlanır) tepeye çevirir;
    # en iyi kayma platonun ORTASINA düşer (giriş/çıkış payı ortalanır)
    if opts.blur_ms > 0:
        sig = opts.blur_ms / RES
        x = np.arange(-int(3 * sig), int(3 * sig) + 1)
        k = np.exp(-0.5 * (x / sig) ** 2)
        act = np.convolve(act, k / k.sum(), mode="same").astype(np.float32)
    speech_w = _weighted(act, opts.silence_weight).astype(np.float64)
    n = len(speech_w)
    cum = np.concatenate([[0.0], np.cumsum(speech_w)])
    coarse = _coarse(events, segs, speech_w, total_ms, opts)
    offs = np.arange(-opts.fine_range_ms, opts.fine_range_ms + 1, opts.step_ms)
    cand = [coarse[s.block] + offs for s in segs]
    scores = []
    for i, s in enumerate(segs):
        scores.append(_scores(events, s, cand[i], cum, n, None))
        if progress:
            progress(0.5 * (i + 1) / len(segs))
    # DP (en iyi toplam: puan − ceza)
    lam = opts.change_penalty
    best = scores[0].copy()
    back = []
    for i in range(1, len(segs)):
        prev_c, cur_c = cand[i - 1], cand[i]
        trans = best[:, None] - lam * np.abs(cur_c[None, :] - prev_c[:, None])
        gap = segs[i].start - segs[i - 1].end
        # sıra: önceki parçanın sonu + s_prev ≤ bu parçanın başı + s_cur  <=>  s_cur ≥ s_prev − gap
        bad = cur_c[None, :] < prev_c[:, None] - gap
        trans = np.where(bad, -np.inf, trans)
        arg = np.argmax(trans, axis=0)
        best = trans[arg, np.arange(len(cur_c))] + scores[i]
        back.append(arg)
        if progress:
            progress(0.5 + 0.5 * i / len(segs))
    idx = [int(np.argmax(best))]
    for arg in reversed(back):
        idx.append(int(arg[idx[-1]]))
    idx.reverse()
    for i, s in enumerate(segs):
        s.shift = int(cand[i][idx[i]])
        sc = scores[i]
        if s.anchors and s.anchor_ms > 0:
            far = np.abs(cand[i] - s.shift) >= 300
            second = sc[far].max() if far.any() else sc.min()
            s.confidence = float(max(0.0, (sc[idx[i]] - second) / s.anchor_ms))
        else:
            s.notes.append("çapa satırı yok — komşu parçaları izledi")
        if abs(s.shift - coarse[s.block]) >= opts.fine_range_ms - opts.step_ms:
            s.notes.append("kayma ince aralığın sınırında — aralığı büyütmek gerekebilir")
    return segs, removed


def changes(events, segs, total_ms: int) -> tuple[dict, list]:
    """Parça kaymalarını satırlara uygula; 0..ses sonu sınırına kırp. Döndürür: ({satır: {start,end}}, uyarılar)."""
    out = {}
    warns = []
    clamped = 0
    for s in segs:
        if not s.shift:
            continue
        for r in s.rows:
            ev = events[r]
            st, en = ev.start + s.shift, ev.end + s.shift
            if st < 0 or en > total_ms:
                clamped += 1
                st = min(max(0, st), max(0, total_ms - 10))
                en = min(max(en, st + 10), total_ms)
            out[r] = {"start": st, "end": en}
    if clamped:
        warns.append(f"{clamped} satır 0 ile ses sonu arasına kırpıldı")
    return out, warns
