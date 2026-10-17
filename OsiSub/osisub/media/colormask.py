"""Renk modeliyle nesne maskesi (hairtrack'in uygulama içi hali).

Kullanım alanı: saç, el, bayrak gibi tabelanın ÖNÜNDEN geçen ve kendine özgü renkleri
olan nesneler — özellikle hareket farkı (warp-fark) yönteminin çalışmadığı, tabelanın
\\pos ile durduğu planlar (assmotion `occlude` warp gerektirir).

Yöntem (hairtrack 1.0):
  1. Tohum karede kullanıcı nesnenin üstüne (pozitif) ve çevresine (negatif) fırça darbeleri çizer.
  2. Pozitif ve negatif pikseller CIELAB'da ayrı ayrı k-means ile modellenir. Ağırlığı
     `min_weight` altındaki pozitif modlar atılır (tohumun içine kaçan koyu çizgi-sanat telleri
     ayrı bir mod oluşturup karedeki HER siyah kenarı yakalıyordu — ölçülmüş bulgu); bir negatif
     moda `ambiguous` ΔE'den yakın pozitif modlar da atılır (tohum kenara taşınca).
  3. Her pikselde en yakın pozitif mod uzaklığı dp, negatif dn: dp < dn·`ratio` ve dp < `max_dist`.
  4. Kapama, `max_hole` ile SINIRLI delik doldurma (sınırsız doldurma saçın çevrelediği yüzü
     maskeye katıyordu — ölçülmüş bulgu), küçük bileşen atma.
  5. Bileşen seçimi: tohum karede pozitif tohuma değen, sonraki karelerde önceki maskenin
     `track_px` genişletilmişine değen bileşenler (başka yerdeki aynı renkli şeyler alınmaz).
  6. Kırpma (`crop`): yalnız verilen kutunun içi (satırın alanı) — \\iclip'i küçültür
     (hairtrack ölçümü: 70 satır/673 karakter -> 38/198).
Çıktı, örtme sonucu (occlude.OcclusionResult) biçimindedir; Uygula aynı \\iclip yolunu kullanır.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from .occlude import FrameOcclusion, OcclusionOptions, OcclusionResult, _disk, _polys


@dataclass
class ColorMaskOptions:
    k_pos: int = 4
    k_neg: int = 6
    min_weight: float = 0.05
    ratio: float = 0.85
    max_dist: float = 28.0
    close_px: int = 2
    max_hole: float = 0.01          # kare alanının oranı
    min_area: float = 0.0005        # kare alanının oranı
    track_px: int = 12
    grow: int = 1
    epsilon: float = 1.0
    ambiguous: float = 12.0         # negatif moda bu ΔE'den yakın pozitif mod atılır
    lineart_l: float = 18.0         # bu L'den koyu, baskın olmayan pozitif mod çizgi-sanat sayılır
    scale: float = 1.0              # işlem ölçeği (1080p için 0.5 önerilir: ~4 kat hızlı)


class ColorModel:
    def __init__(self, pos_centers, neg_centers, opts: ColorMaskOptions):
        self.pos = np.asarray(pos_centers, np.float32)
        self.neg = np.asarray(neg_centers, np.float32)
        self.opts = opts
        self.dropped = 0
        self.ambiguous = 0
        self.lineart = 0

    @classmethod
    def fit(cls, lab: np.ndarray, pos_mask: np.ndarray, neg_mask: np.ndarray,
            opts: ColorMaskOptions | None = None) -> "ColorModel":
        import cv2
        opts = opts or ColorMaskOptions()
        P = lab[pos_mask > 0].reshape(-1, 3).astype(np.float32)
        N = lab[neg_mask > 0].reshape(-1, 3).astype(np.float32)
        if len(P) < 20:
            raise ValueError("Pozitif tohum çok az (nesnenin üstüne biraz daha çiz)")
        if len(N) < 20:
            raise ValueError("Negatif tohum çok az (nesnenin çevresine — tabela/arka plan — çiz)")

        def km(X, k):
            k = max(1, min(k, len(X) // 10))
            crit = (cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_MAX_ITER, 30, 0.5)
            cv2.setRNGSeed(1)
            _c, lab_, cen = cv2.kmeans(X, k, None, crit, 3, cv2.KMEANS_PP_CENTERS)
            w = np.bincount(lab_.ravel(), minlength=k) / len(X)
            return cen, w
        pc, pw = km(P, opts.k_pos)
        nc, _nw = km(N, opts.k_neg)
        keep = pw >= opts.min_weight
        # negatif bir moda çok yakın pozitif mod belirsizdir (ör. tohum telin siyah kenarına taşmış,
        # negatif tohum da tabelanın siyah yazısına değmiş): atılır
        dmin = np.sqrt(((pc[:, None, :] - nc[None, :, :]) ** 2).sum(axis=2)).min(axis=1)
        ambiguous = keep & (dmin < opts.ambiguous)
        keep &= ~ambiguous
        # çizgi-sanat koruması: çok koyu (L < lineart_l) ve baskın olmayan pozitif mod = tohumun taştığı
        # siyah kontur; tutulursa karedeki her siyah kenar (tabelanın yazısı dahil) maskeye girer.
        # Saçın kendisi siyahsa o mod baskındır (ağırlık > %50) ve tutulur.
        lineart = keep & (pc[:, 0] < opts.lineart_l) & (pw < 0.5)
        keep &= ~lineart
        if not keep.any():
            raise ValueError("Pozitif ve negatif tohumlar aynı renkleri içeriyor; nesnenin İÇİNE ve "
                             "dışına daha dikkatli çiz")
        m = cls(pc[keep], nc, opts)
        m.dropped = int((~keep).sum())
        m.ambiguous = int(ambiguous.sum())
        m.lineart = int(lineart.sum())
        return m

    def classify(self, lab: np.ndarray) -> np.ndarray:
        X = lab.reshape(-1, 1, 3).astype(np.float32)
        dp = np.sqrt(((X - self.pos[None]) ** 2).sum(axis=2)).min(axis=1)
        dn = np.sqrt(((X - self.neg[None]) ** 2).sum(axis=2)).min(axis=1)
        m = (dp < dn * self.opts.ratio) & (dp < self.opts.max_dist)
        return m.reshape(lab.shape[:2]).astype(np.uint8)

    def to_json(self) -> dict:
        return {"pos": self.pos.round(3).tolist(), "neg": self.neg.round(3).tolist()}


def _lab(rgb):
    import cv2
    return cv2.cvtColor(rgb.astype(np.float32) / 255.0, cv2.COLOR_RGB2LAB)


def _fill_small_holes(m, max_px):
    import cv2
    cnts, hier = cv2.findContours(m, cv2.RETR_CCOMP, cv2.CHAIN_APPROX_SIMPLE)
    if hier is None:
        return m
    out = m.copy()
    for i, c in enumerate(cnts):
        if hier[0][i][3] >= 0 and cv2.contourArea(c) <= max_px:
            cv2.drawContours(out, cnts, i, 1, -1)
    return out


def segment(model: ColorModel, rgb: np.ndarray, guide: np.ndarray | None, opts: ColorMaskOptions) -> np.ndarray:
    """Tek kare: sınıflandır, temizle, `guide`'a (tohum ya da önceki maske) değen bileşenleri tut."""
    import cv2
    H, W = rgb.shape[:2]
    m = model.classify(_lab(rgb))
    if opts.close_px:
        m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, _disk(opts.close_px))
    m = _fill_small_holes(m, opts.max_hole * H * W)
    n, lab, st, _c = cv2.connectedComponentsWithStats(m)
    keep = np.zeros_like(m)
    min_px = opts.min_area * H * W
    for j in range(1, n):
        if st[j, cv2.CC_STAT_AREA] < min_px:
            continue
        comp = lab == j
        if guide is not None and not (guide[comp] > 0).any():
            continue
        keep[comp] = 1
    return keep


def track_masks(get_rgb, start: int, end: int, seed_frame: int, pos_mask, neg_mask, crop=None,
                opts: ColorMaskOptions | None = None, progress=None, cancel=None) -> tuple[OcclusionResult, ColorModel]:
    """Tohum kareden ileri ve geri maske yayılımı. crop=(x,y,w,h) video pikseli (None: tüm kare)."""
    import cv2
    opts = opts or ColorMaskOptions()
    if not (start <= seed_frame <= end):
        raise ValueError("Tohum karesi aralığın içinde olmalı")
    full = get_rgb(seed_frame)
    H, W = full.shape[:2]
    if crop:
        x, y, w, h = crop
        x0, y0 = int(max(0, np.floor(x))), int(max(0, np.floor(y)))
        x1, y1 = int(min(W, np.ceil(x + w))), int(min(H, np.ceil(y + h)))
    else:
        x0, y0, x1, y1 = 0, 0, W, H
    if x1 - x0 < 8 or y1 - y0 < 8:
        raise ValueError("Kırpma alanı çok küçük")
    s = opts.scale

    def prep(img):
        c = img[y0:y1, x0:x1]
        return cv2.resize(c, None, fx=s, fy=s, interpolation=cv2.INTER_AREA) if s != 1.0 else c

    def prep_mask(mk):
        c = mk[y0:y1, x0:x1].astype(np.uint8)
        return cv2.resize(c, None, fx=s, fy=s, interpolation=cv2.INTER_NEAREST) if s != 1.0 else c
    seed_img = prep(full)
    pos = prep_mask(pos_mask)
    neg = prep_mask(neg_mask)
    model = ColorModel.fit(_lab(seed_img), pos, neg, opts)
    sc_opts = ColorMaskOptions(**{**opts.__dict__, "track_px": max(1, int(opts.track_px * s)),
                                  "close_px": max(1, int(round(opts.close_px * s)))})
    area_crop = float((x1 - x0) * (y1 - y0))
    out: dict[int, FrameOcclusion] = {}
    total = end - start + 1
    done = [0]

    def emit(k, m):
        if s != 1.0:
            m = cv2.resize(m, (x1 - x0, y1 - y0), interpolation=cv2.INTER_NEAREST)
        if opts.grow:
            m = cv2.dilate(m, _disk(opts.grow))
        polys = _polys((m > 0).astype(np.uint8) * 255, x0, y0, opts.epsilon)
        ratio = float(m.sum()) / area_crop
        out[k] = FrameOcclusion(k, polys, ratio, 1.0, True)
        done[0] += 1
        if progress:
            progress(done[0] / total, k)
        if cancel is not None and cancel():
            from .tracker import Cancelled
            raise Cancelled()

    m_seed = segment(model, seed_img, pos, sc_opts)
    warn = []
    cover = float(m_seed.mean())
    if cover > 0.5:
        warn.append(f"tohum karesinde maske kırpma alanının %{cover * 100:.0f}'ini kaplıyor — pozitif tohum "
                    "nesnenin dışına taşmış olabilir")
    if neg.any():
        leak = float(m_seed[neg > 0].mean())
        if leak > 0.05:
            warn.append(f"negatif tohumun %{leak * 100:.0f}'i maskeye girdi — renkler ayrışmıyor")
    emit(seed_frame, m_seed)
    kern = _disk(sc_opts.track_px)
    for rng in (range(seed_frame + 1, end + 1), range(seed_frame - 1, start - 1, -1)):
        prev = m_seed
        for k in rng:
            guide = cv2.dilate(prev, kern)
            m = segment(model, prep(get_rgb(k)), guide, sc_opts)
            if m.sum() == 0:
                # nesne çıktı ya da kayboldu: rehberi koru (geri gelirse yakalansın)
                emit(k, m)
                continue
            emit(k, m)
            prev = m
    note = f"renk modeli (hairtrack): {len(model.pos)} pozitif, {len(model.neg)} negatif mod"
    if model.dropped:
        note += (f"; {model.dropped} pozitif mod atıldı (çizgi-sanat: {model.lineart}, "
                 f"negatife çok yakın: {model.ambiguous})")
    res = OcclusionResult(out, seed_frame, OcclusionOptions(), [note] + warn)
    return res, model
