"""Önden geçen nesneleri bulma (assmotion `occlude` yöntemi) ve görünürlük.

Yöntem (warp-fark):
  0. Temiz görünüm modeli: izin karelerinden en çok 31 tanesi referans kareye geri
     çarpıtılır ve piksel başına ORTANCA alınır (CIELAB). Önden geçen nesne her pikselde
     karelerin yarısından azında bulunduğu sürece modelden silinir; böylece referans
     karede örtme olsa bile çalışır (assmotion'ın tek-referans yönteminden sağlam).
     model="ref" seçilirse eskisi gibi yalnız referans kare kullanılır.
  1. Model, izin o kareye ait benzerlik dönüşümüyle çarpıtılır: tabela
     örtülmeseydi bu karede nasıl görüneceğinin tahmini.
  2. Gerçek kare ile tahmin arasındaki renk farkı (CIELAB ΔE, hafif bulanıklaştırılmış)
     izlenen alanın içinde eşik üstündeyse orası "örtülü". Fark, tahminin ±`shift_tol` piksel
     kaydırılmışlarının EN KÜÇÜĞÜdür: izin alt-piksel hatası keskin kenarlarda (tabelanın yazısı)
     büyük ΔE üretip yanlış alarm veriyordu (temiz videoda kare 0–1 %1 "örtülü" — x264 sürümüne
     ve alanın ±2 px yerine göre değişen sınırda bir sonuç); gerçek örten hiçbir kaydırmayla eşleşmez.
  3. Açma (kenar hizalama hatalarından gelen ince çizgileri atar), kapama, delik doldurma,
     küçük bileşenleri atma, `grow` piksel genişletme.
  4. Maske dış konturları Ramer-Douglas-Peucker ile sadeleşir -> \\iclip çizimi.

Görünürlük = (alanın kare içinde kalan kısmı) × (1 − örtülü oran).

Bilinen sınırlar (assmotion FINDINGS'teki ölçümlerle aynı doğada):
  * Ortanca model: bir piksel karelerin yarısından fazlasında örtülüyse (uzun süre duran
    örten) model onu tabelanın parçası sanar; o durumda referansı temiz bir kareye alın
    ve model="ref" kullanın.
  * Önden geçen nesnenin rengi tabelanınkine çok yakınsa delik kalır; delik doldurma ve
    `grow` bunu kısmen kapatır.
  * Işık/renk değişimi (parlama, solma) örtme sanılabilir: `threshold` yükseltilmeli;
    güvenlik için örtülü oran `max_ratio`'yu aşarsa kare "belirsiz" sayılır (maske uygulanmaz,
    görünürlük ölçülemez olarak işaretlenir).
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np


@dataclass
class OcclusionOptions:
    threshold: float = 22.0      # ΔE (CIELAB) eşiği
    blur: int = 3                # fark öncesi Gauss çekirdeği (tek sayı; 0 = kapalı)
    open_px: int = 2             # açma yarıçapı
    close_px: int = 3            # kapama yarıçapı
    min_area: float = 0.004      # alanın bu oranından küçük bileşenler atılır
    grow: int = 2                # son genişletme (px)
    edge_margin: int = 2         # alan kenarından içeri bu kadar piksel yok sayılır (ara değer hatası)
    max_ratio: float = 0.85      # bundan fazla örtülü görünüyorsa kare belirsiz (ışık değişimi olabilir)
    epsilon: float = 1.0         # kontur sadeleştirme (px)
    model: str = "median"        # "median" (temiz görünüm modeli) | "ref" (yalnız referans kare)
    samples: int = 31            # ortanca için en çok kare
    shift_tol: int = 1           # fark ±bu kadar piksel kaydırmaya toleranslı (alt-piksel iz hatası)


@dataclass
class FrameOcclusion:
    frame: int
    polys: list = field(default_factory=list)    # [[(x, y), ...], ...] video pikseli
    ratio: float = 0.0          # örtülü oran (alan içi)
    inside: float = 1.0         # alanın kare içinde kalan oranı
    ok: bool = True             # False: belirsiz (çok büyük fark)

    @property
    def visibility(self) -> float:
        return self.inside * (1.0 - self.ratio) if self.ok else self.inside


@dataclass
class OcclusionResult:
    frames: dict             # kare -> FrameOcclusion
    ref_frame: int
    options: OcclusionOptions
    notes: list = field(default_factory=list)

    def occluded_frames(self) -> list[int]:
        return sorted(k for k, f in self.frames.items() if f.ok and f.polys)

    def uncertain_frames(self) -> list[int]:
        return sorted(k for k, f in self.frames.items() if not f.ok)

    def summary(self) -> str:
        occ = self.occluded_frames()
        s = f"örtme: {len(occ)} karede önden geçen nesne"
        if occ:
            s += f" ({occ[0]}–{occ[-1]})"
            mx = max(self.frames[k].ratio for k in occ)
            s += f", en çok %{mx * 100:.0f} örtülü"
        unc = self.uncertain_frames()
        if unc:
            s += f"; {len(unc)} kare belirsiz (fark çok büyük)"
        if self.notes:
            s += "; " + "; ".join(self.notes)
        return s

    def to_json(self) -> dict:
        return {"ref_frame": self.ref_frame,
                "frames": [{"f": k, "ok": f.ok, "ratio": round(f.ratio, 4), "inside": round(f.inside, 4),
                            "polys": [[[round(x, 2), round(y, 2)] for x, y in p] for p in f.polys]}
                           for k, f in sorted(self.frames.items())]}

    @classmethod
    def from_json(cls, d: dict) -> "OcclusionResult":
        fr = {}
        for x in d.get("frames", []):
            fr[int(x["f"])] = FrameOcclusion(int(x["f"]), [[tuple(p) for p in poly] for poly in x.get("polys", [])],
                                             float(x.get("ratio", 0)), float(x.get("inside", 1)),
                                             bool(x.get("ok", True)))
        return cls(fr, int(d.get("ref_frame", 0)), OcclusionOptions())


def _quad(roi, tf):
    x, y, w, h = roi
    return np.array([tf.map(px, py) for px, py in ((x, y), (x + w, y), (x + w, y + h), (x, y + h))], np.float64)


def detect(get_rgb, track, roi=None, opts: OcclusionOptions | None = None, frames=None,
           progress=None, cancel=None) -> OcclusionResult:
    """İzin her karesinde (ya da `frames`) örtme maskesi."""
    import cv2
    opts = opts or OcclusionOptions()
    roi = roi or track.roi
    if not roi:
        raise ValueError("İzleme alanı yok")
    tr = track.filled()
    ref = tr.ref_frame
    ref_img = get_rgb(ref)
    H, W = ref_img.shape[:2]
    notes = []
    # model kesiti: ROI çevresi (referans koordinatı)
    rx0 = int(max(0, np.floor(roi[0]) - 4))
    ry0 = int(max(0, np.floor(roi[1]) - 4))
    rx1 = int(min(W, np.ceil(roi[0] + roi[2]) + 4))
    ry1 = int(min(H, np.ceil(roi[1] + roi[3]) + 4))
    ref_lab = _lab(ref_img, opts.blur)[ry0:ry1, rx0:rx1]
    model = ref_lab
    if opts.model == "median":
        good = [f for f in tr.frames if f.ok] or tr.frames
        if len(good) >= 5:
            idx = np.unique(np.linspace(0, len(good) - 1, min(opts.samples, len(good))).round().astype(int))
            stack = []
            r0 = complex(rx0, ry0)
            for i in idx:
                f = good[int(i)]
                img = ref_img if f.frame == ref else get_rgb(f.frame)
                tt = f.t + f.c * r0
                Mi = np.array([[f.c.real, -f.c.imag, tt.real], [f.c.imag, f.c.real, tt.imag]], np.float64)
                lab = _lab(img, opts.blur)
                stack.append(cv2.warpAffine(lab, Mi, (rx1 - rx0, ry1 - ry0),
                                            flags=cv2.INTER_LINEAR | cv2.WARP_INVERSE_MAP,
                                            borderMode=cv2.BORDER_REPLICATE))
                if cancel is not None and cancel():
                    from .tracker import Cancelled
                    raise Cancelled()
            model = np.median(np.stack(stack), axis=0).astype(np.float32)
            # referans karenin kendisi örtülü müydü?
            rd = (_shift_min_de(ref_lab, model, opts.shift_tol) > opts.threshold).astype(np.uint8)
            if opts.open_px:
                rd = cv2.morphologyEx(rd, cv2.MORPH_OPEN, _disk(max(2, opts.open_px)))
            rr = float(rd.mean())
            if rr > 0.03:
                notes.append(f"referans karenin %{rr * 100:.0f}'i örtülü görünüyor; ortanca model kullanıldı")
    ks = list(frames) if frames is not None else [f.frame for f in tr.frames]
    out = {}
    roi_area = max(1.0, roi[2] * roi[3])
    for n, k in enumerate(ks):
        if cancel is not None and cancel():
            from .tracker import Cancelled
            raise Cancelled()
        tf = tr.at(k)
        q = _quad(roi, tf)
        x0 = int(max(0, np.floor(q[:, 0].min()) - 2))
        y0 = int(max(0, np.floor(q[:, 1].min()) - 2))
        x1 = int(min(W, np.ceil(q[:, 0].max()) + 2))
        y1 = int(min(H, np.ceil(q[:, 1].max()) + 2))
        full_mask = np.zeros((H, W), np.uint8)
        cv2.fillConvexPoly(full_mask, np.round(q).astype(np.int32), 255)
        inside_px = float(np.count_nonzero(full_mask))
        quad_area = abs(_poly_area(q))
        inside = min(1.0, inside_px / max(1.0, quad_area))
        if x1 - x0 < 4 or y1 - y0 < 4 or (k == ref and opts.model == "ref"):
            out[k] = FrameOcclusion(k, [], 0.0, inside, True)
            if progress:
                progress((n + 1) / len(ks), k)
            continue
        # modeli bu karenin kesitine çarpıt: kesit = c·(w + r0) + t − (x0, y0)
        tt = tf.c * complex(rx0, ry0) + tf.t - complex(x0, y0)
        Mx = np.array([[tf.c.real, -tf.c.imag, tt.real],
                       [tf.c.imag, tf.c.real, tt.imag]], np.float64)
        pred = cv2.warpAffine(model, Mx, (x1 - x0, y1 - y0), flags=cv2.INTER_LINEAR,
                              borderMode=cv2.BORDER_REPLICATE)
        cur = _lab(get_rgb(k)[y0:y1, x0:x1], opts.blur)
        d = _shift_min_de(cur, pred, opts.shift_tol)
        region = full_mask[y0:y1, x0:x1]
        if opts.edge_margin > 0:
            region = cv2.erode(region, _disk(opts.edge_margin))
        m = ((d > opts.threshold) & (region > 0)).astype(np.uint8) * 255
        if opts.open_px:
            m = cv2.morphologyEx(m, cv2.MORPH_OPEN, _disk(opts.open_px))
        if opts.close_px:
            m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, _disk(opts.close_px))
        m = _fill_holes(m)
        # küçük bileşenleri at
        nlab, lab, st, _ = cv2.connectedComponentsWithStats(m)
        keep = np.zeros_like(m)
        min_px = opts.min_area * roi_area * tf.scale ** 2
        for j in range(1, nlab):
            if st[j, cv2.CC_STAT_AREA] >= min_px:
                keep[lab == j] = 255
        m = keep
        reg_px = max(1.0, float(np.count_nonzero(region)))
        ratio = float(np.count_nonzero(m & region)) / reg_px
        if ratio > opts.max_ratio:
            out[k] = FrameOcclusion(k, [], ratio, inside, False)
        else:
            if opts.grow:
                m = cv2.dilate(m, _disk(opts.grow))
            polys = _polys(m, x0, y0, opts.epsilon)
            out[k] = FrameOcclusion(k, polys, ratio, inside, True)
        if progress:
            progress((n + 1) / len(ks), k)
    return OcclusionResult(out, ref, opts, notes)


def _shift_min_de(cur, pred, tol: int):
    """ΔE(cur, pred) ile ΔE(cur, pred'in ±tol piksel kaydırılmışları) arasında piksel başına en küçüğü."""
    d = np.sqrt(((cur - pred) ** 2).sum(axis=2))
    if tol <= 0:
        return d
    import cv2
    H, W = d.shape
    pad = cv2.copyMakeBorder(pred, tol, tol, tol, tol, cv2.BORDER_REPLICATE)
    for dy in range(-tol, tol + 1):
        for dx in range(-tol, tol + 1):
            if dx or dy:
                sh = pad[tol + dy:tol + dy + H, tol + dx:tol + dx + W]
                np.minimum(d, np.sqrt(((cur - sh) ** 2).sum(axis=2)), out=d)
    return d


def _lab(rgb, blur):
    import cv2
    img = rgb
    if blur and blur > 1:
        b = blur | 1
        img = cv2.GaussianBlur(img, (b, b), 0)
    lab = cv2.cvtColor(img.astype(np.float32) / 255.0, cv2.COLOR_RGB2LAB)
    return lab


_disks: dict = {}


def _disk(r):
    import cv2
    k = _disks.get(r)
    if k is None:
        k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * r + 1, 2 * r + 1))
        _disks[r] = k
    return k


def _fill_holes(m):
    import cv2
    cnts, hier = cv2.findContours(m, cv2.RETR_CCOMP, cv2.CHAIN_APPROX_SIMPLE)
    if hier is None:
        return m
    out = m.copy()
    for i, c in enumerate(cnts):
        if hier[0][i][3] >= 0:          # iç kontur = delik
            cv2.drawContours(out, cnts, i, 255, -1)
    return out


def _polys(m, x0, y0, eps):
    import cv2
    cnts, _ = cv2.findContours(m, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    polys = []
    for c in cnts:
        if cv2.contourArea(c) < 4:
            continue
        a = cv2.approxPolyDP(c, eps, True).reshape(-1, 2)
        if len(a) >= 3:
            # piksel merkezleri -> piksel kenarlarını kapsasın diye +0.5
            polys.append([(float(x) + x0 + 0.5, float(y) + y0 + 0.5) for x, y in a])
    return polys


def _poly_area(q):
    x, y = q[:, 0], q[:, 1]
    return 0.5 * float(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))


def polys_to_drawing(polys, kx: float = 1.0, ky: float = 1.0, decimals: int = 1) -> str:
    """Video pikseli çokgenleri -> ASS çizimi (betik koordinatı = video / k)."""
    from ..core import tags as T
    parts = []
    for p in polys:
        pts = [["m", p[0][0] / kx, p[0][1] / ky]] + [["l", x / kx, y / ky] for x, y in p[1:]]
        parts.append(T.drawing_str(pts, decimals))
    return " ".join(parts)
