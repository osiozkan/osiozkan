"""Hareket izleme (OpenCV). Sonuç: core.motion.MotionTrack.

Yöntemler
  points    Seçilen alanda köşe noktaları (Shi-Tomasi) bulunur, Lucas-Kanade optik akışla
            kareden kareye izlenir. Her nokta REFERANS karedeki yerini taşır; dönüşüm her
            karede doğrudan referans->bu kare noktalarından RANSAC'la kestirilir
            (cv2.estimateAffinePartial2D: konum + eşit ölçek + dönüş). Böylece kare kare
            dönüşüm çarpımının kayması birikmez. İleri-geri tutarlılık denetimi (LK'yı geri
            çalıştırıp başlangıca dönüş hatası) kayan noktaları atar. Nokta sayısı yarıya
            inince alanın o anki yerinden yeni noktalar eklenir (referans yerleri ters
            dönüşümle bulunur).
            model="translation": yalnız konum (inlier yer değiştirmelerinin ortancası).
  template  Referans karedeki alan şablon olarak arama penceresinde aranır
            (TM_CCOEFF_NORMED, alt piksel tepe). Yalnız konum. Dokusu az, düz renkli
            tabelalarda noktalardan sağlam olabilir.
  ecc       (assmotion xtrack) Şablon, ECC (Enhanced Correlation Coefficient) ile afin
            olarak hizalanır; afin en yakın benzerliğe indirgenir (konum + ölçek + dönüş).
            Arama penceresi kırpılır ve gerekirse küçültülür (tam karede çok yavaş).
            Korumalar: korelasyon < `ecc_min` ya da kare başına ölçek > %15 / dönüş > 10°
            değişirse kare güvensiz sayılır ve önceki dönüşüm tutulur (koruma olmadan ölçek
            4 kata, dönüş 80°'ye kaçabiliyor — assmotion FINDINGS).
  color     (assmotion track) Renk anahtarı: alandaki baskın doygun renk (HSV, ton ±10)
            ya da verilen `key_rgb`. Her karede tahmin edilen yerin çevresinde o rengin
            bileşeni bulunur: ağırlık merkezi -> konum, √(alan/alan_ref) -> ölçek (dönüş yok).
            Yalnız TAM görünen kareler ölçülür (kutu en/boy oranı, görünen oran ≥ %93, ölçek
            eğilimi); örtülen kareler güvensiz işaretlenir ve ara değerle dolar — renk tek başına
            örtülen kenarı bilemez, tahmin etmek yerine dürüstçe işaretlenir. Uzun örtmede ara
            değer doğrusaldır; eğri yoldaki tabelada Noktalar/ECC daha iyidir.
            Dönüş izlenmez; dönen tabelada Noktalar/ECC kullanın.

Kareler
  İleri geçiş FrameGrabber'ın ardışık çözme yolunu kullanır. Geri geçiş için kareler
  32'lik parçalar halinde ileri çözülüp önbellekten tersten okunur (her kare için
  yeniden arama yapılmaz). İşlem gri tonda ve uzun kenar `max_side` pikseli aşarsa
  küçültülmüş görüntüde yapılır; sonuç tam çözünürlüğe çevrilir.
"""

from __future__ import annotations

import math

import numpy as np

from ..core.motion import MotionTrack, TrackFrame


def available() -> tuple[bool, str]:
    try:
        import cv2  # noqa: F401
        return True, ""
    except ImportError:
        return False, "OpenCV kurulu değil (pip install opencv-python)"


class Cancelled(Exception):
    pass


class _Frames:
    """Gri (ya da HSV), ölçeklenmiş kare kaynağı; geri yönde parça önbelleği."""

    def __init__(self, get_rgb, factor: float, kind: str = "gray"):
        import cv2
        self.cv2 = cv2
        self.get_rgb = get_rgb
        self.f = factor
        self.kind = kind
        self.cache: dict[int, np.ndarray] = {}

    def _conv(self, rgb):
        if self.kind == "hsv":
            g = self.cv2.cvtColor(rgb, self.cv2.COLOR_RGB2HSV)
        else:
            g = self.cv2.cvtColor(rgb, self.cv2.COLOR_RGB2GRAY)
        if self.f != 1.0:
            g = self.cv2.resize(g, None, fx=self.f, fy=self.f, interpolation=self.cv2.INTER_AREA)
        return g

    def get(self, k: int) -> np.ndarray:
        g = self.cache.get(k)
        if g is None:
            g = self._conv(self.get_rgb(k))
        return g

    def prefetch_back(self, k: int, lo: int, chunk: int = 32):
        """k'nin önbellekte olmasını sağla: [max(lo, k-chunk+1), k] aralığını ileri çöz."""
        if k in self.cache:
            return
        self.cache.clear()
        for j in range(max(lo, k - chunk + 1), k + 1):
            self.cache[j] = self._conv(self.get_rgb(j))


def track(get_rgb, start: int, end: int, ref: int, roi, method: str = "points", model: str = "similarity",
          width: int = 0, height: int = 0, fps: float | None = None, max_side: int = 1280,
          progress=None, cancel=None, video: str = "", key_rgb=None, ecc_min: float = 0.6) -> MotionTrack:
    """get_rgb(k) -> (h,w,3) uint8. roi=(x,y,w,h) referans karede, video pikseli."""
    import cv2
    if not (start <= ref <= end):
        raise ValueError("Referans kare aralığın içinde olmalı")
    x, y, w, h = (float(v) for v in roi)
    if w < 4 or h < 4:
        raise ValueError("İzleme alanı çok küçük")
    first = get_rgb(ref)
    H, W = first.shape[:2]
    width, height = width or W, height or H
    f = min(1.0, max_side / max(W, H))
    src = _Frames(get_rgb, f, "hsv" if method == "color" else "gray")
    sroi = (x * f, y * f, w * f, h * f)
    total = end - start + 1
    done = [0]
    res: dict[int, TrackFrame] = {ref: TrackFrame(ref, 1 + 0j, 0j, 1.0, True)}

    def tick(k):
        done[0] += 1
        if cancel is not None and cancel():
            raise Cancelled()
        if progress:
            progress(done[0] / total, k)

    tick(ref)
    runner = None
    if method == "template":
        runner = _TemplateRunner(src.get(ref), sroi)
    elif method == "ecc":
        runner = _EccRunner(src.get(ref), sroi, ecc_min)
    elif method == "color":
        runner = _ColorRunner(src.get(ref), sroi, key_rgb)
    else:
        runner = _PointsRunner(src.get(ref), sroi, model)
    runner.ref_k = ref
    # ileri
    runner.reset()
    for k in range(ref + 1, end + 1):
        tf = runner.step(src.get(k), k)
        res[k] = _unscale(tf, f)
        tick(k)
    # geri
    runner.reset()
    for k in range(ref - 1, start - 1, -1):
        src.prefetch_back(k, start)
        tf = runner.step(src.get(k), k)
        res[k] = _unscale(tf, f)
        tick(k)
    frames = [res[k] for k in range(start, end + 1)]
    tr = MotionTrack(frames, ref, width, height, fps, (x, y, w, h), f"{method}/{model}" if method == "points"
                     else method, video)
    if runner.notes:
        tr.notes += sorted(set(runner.notes))
    return tr


def _unscale(tf: TrackFrame, f: float) -> TrackFrame:
    if f == 1.0:
        return tf
    return TrackFrame(tf.frame, tf.c, tf.t / f, tf.conf, tf.ok)


# ---------------------------------------------------------------------------
# Nokta izleyici
# ---------------------------------------------------------------------------

class _PointsRunner:
    LK = dict(winSize=(21, 21), maxLevel=3,
              criteria=(3, 30, 0.01))           # cv2.TERM_CRITERIA_EPS | COUNT = 3

    def __init__(self, ref_img, roi, model):
        import cv2
        self.cv2 = cv2
        self.ref_img = ref_img
        self.roi = roi
        self.model = model
        self.notes: list[str] = []
        pts = self._detect(ref_img, self._roi_mask(ref_img.shape, 1 + 0j, 0j))
        if pts is None or len(pts) < 6:
            raise ValueError("Seçilen alanda izlenecek yeterli doku yok (en az 6 köşe gerekir). "
                             "Alanı büyütün ya da 'Şablon' yöntemini deneyin.")
        self.ref_pts0 = pts.astype(np.float32)
        self.n0 = len(pts)

    def _roi_mask(self, shape, c, t):
        x, y, w, h = self.roi
        corners = np.array([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], np.float64)
        z = c * (corners[:, 0] + 1j * corners[:, 1]) + t
        poly = np.stack([z.real, z.imag], axis=1).round().astype(np.int32)
        m = np.zeros(shape[:2], np.uint8)
        self.cv2.fillConvexPoly(m, poly, 255)
        return m

    def _detect(self, img, mask, exclude=None):
        x, y, w, h = self.roi
        md = max(3.0, min(w, h) / 25)
        if exclude is not None and len(exclude):
            for px, py in exclude:
                self.cv2.circle(mask, (int(px), int(py)), int(md), 0, -1)
        pts = self.cv2.goodFeaturesToTrack(img, maxCorners=300, qualityLevel=0.01, minDistance=md,
                                           mask=mask, blockSize=5)
        if pts is None:
            return None
        pts = pts.reshape(-1, 2)
        crit = (3, 20, 0.03)
        self.cv2.cornerSubPix(img, pts.reshape(-1, 1, 2), (5, 5), (-1, -1), crit)
        return pts

    def reset(self):
        self.prev = self.ref_img
        self.cur = self.ref_pts0.copy()
        self.refp = self.ref_pts0.copy()
        self.c, self.t = 1 + 0j, 0j
        self.lost = False

    def step(self, img, k) -> TrackFrame:
        cv2 = self.cv2
        if self.lost or len(self.cur) < 3:
            self.lost = True
            return TrackFrame(k, self.c, self.t, 0.0, False)
        p0 = self.cur.reshape(-1, 1, 2)
        p1, st, _err = cv2.calcOpticalFlowPyrLK(self.prev, img, p0, None, **self.LK)
        pb, st2, _ = cv2.calcOpticalFlowPyrLK(img, self.prev, p1, None, **self.LK)
        fb = np.linalg.norm(pb.reshape(-1, 2) - self.cur, axis=1)
        good = (st.ravel() == 1) & (st2.ravel() == 1) & (fb < 1.0)
        p1 = p1.reshape(-1, 2)
        H, W = img.shape[:2]
        good &= (p1[:, 0] >= 0) & (p1[:, 1] >= 0) & (p1[:, 0] < W) & (p1[:, 1] < H)
        refp, cur = self.refp[good], p1[good]
        self.prev = img
        if len(cur) < 6:
            self.cur, self.refp = cur, refp
            self.lost = len(cur) < 3
            return TrackFrame(k, self.c, self.t, len(cur) / max(1, self.n0), False)
        M, inl = cv2.estimateAffinePartial2D(refp, cur, method=cv2.RANSAC, ransacReprojThreshold=1.5,
                                             maxIters=2000, confidence=0.995, refineIters=10)
        if M is None:
            self.cur, self.refp = cur, refp
            return TrackFrame(k, self.c, self.t, 0.0, False)
        inl = inl.ravel().astype(bool)
        refp, cur = refp[inl], cur[inl]
        if self.model == "translation":
            dz = (cur - refp)
            dx, dy = float(np.median(dz[:, 0])), float(np.median(dz[:, 1]))
            c, t = 1 + 0j, complex(dx, dy)
        else:
            # Yalnız inlier'larla en küçük kareler benzerliği (RANSAC sonucunu inceltir)
            c, t = _similarity_lsq(refp, cur)
        ok = len(cur) >= 6 and 0.05 < abs(c) < 20
        conf = min(1.0, len(cur) / max(6.0, 0.5 * self.n0))
        if ok:
            self.c, self.t = c, t
        self.cur, self.refp = cur.astype(np.float32), refp.astype(np.float32)
        if len(self.cur) < max(12, self.n0 // 2) and ok:
            self._reseed(img)
        return TrackFrame(k, self.c, self.t, conf, ok)

    def _reseed(self, img):
        mask = self._roi_mask(img.shape, self.c, self.t)
        new = self._detect(img, mask, exclude=self.cur)
        if new is None or not len(new):
            return
        # referans konumu: z_ref = (z - t) / c
        z = new[:, 0] + 1j * new[:, 1]
        zr = (z - self.t) / self.c
        refn = np.stack([zr.real, zr.imag], axis=1).astype(np.float32)
        need = max(0, self.n0 - len(self.cur))
        self.cur = np.concatenate([self.cur, new[:need].astype(np.float32)])
        self.refp = np.concatenate([self.refp, refn[:need]])
        self.notes.append("izleme sırasında yeni noktalar eklendi")


def _similarity_lsq(src: np.ndarray, dst: np.ndarray) -> tuple[complex, complex]:
    """dst ≈ c·src + t (karmaşık en küçük kareler; Umeyama'nın ölçekli 2B hali)."""
    zs = src[:, 0].astype(np.float64) + 1j * src[:, 1]
    zd = dst[:, 0].astype(np.float64) + 1j * dst[:, 1]
    ms, md = zs.mean(), zd.mean()
    a, b = zs - ms, zd - md
    den = float(np.sum(np.abs(a) ** 2))
    c = complex(np.sum(np.conj(a) * b) / den) if den > 1e-9 else 1 + 0j
    return c, complex(md - c * ms)


# ---------------------------------------------------------------------------
# Şablon izleyici
# ---------------------------------------------------------------------------

class _TemplateRunner:
    def __init__(self, ref_img, roi):
        import cv2
        self.cv2 = cv2
        x, y, w, h = (int(round(v)) for v in roi)
        H, W = ref_img.shape[:2]
        x, y = max(0, x), max(0, y)
        w, h = min(w, W - x), min(h, H - y)
        if w < 4 or h < 4:
            raise ValueError("İzleme alanı görüntünün dışında")
        self.tpl = ref_img[y:y + h, x:x + w].copy()
        if float(self.tpl.std()) < 2.0:
            raise ValueError("Seçilen alan düz renk; şablon izlemek için kenar/desen içermeli")
        self.x0, self.y0, self.w, self.h = x, y, w, h
        self.notes: list[str] = []

    def reset(self):
        self.pos = complex(self.x0, self.y0)
        self.vel = 0j
        self.t = 0j

    def step(self, img, k) -> TrackFrame:
        cv2 = self.cv2
        H, W = img.shape[:2]
        pred = self.pos + self.vel
        mx = max(24, int(self.w * 0.6))
        my = max(24, int(self.h * 0.6))
        sx0 = int(max(0, math.floor(pred.real) - mx))
        sy0 = int(max(0, math.floor(pred.imag) - my))
        sx1 = int(min(W, math.ceil(pred.real) + self.w + mx))
        sy1 = int(min(H, math.ceil(pred.imag) + self.h + my))
        if sx1 - sx0 < self.w or sy1 - sy0 < self.h:
            return TrackFrame(k, 1 + 0j, self.t, 0.0, False)
        win = img[sy0:sy1, sx0:sx1]
        r = cv2.matchTemplate(win, self.tpl, cv2.TM_CCOEFF_NORMED)
        _mn, mxv, _ml, (px, py) = cv2.minMaxLoc(r)
        dx = _parabola(r, px, py, 0)
        dy = _parabola(r, px, py, 1)
        new = complex(sx0 + px + dx, sy0 + py + dy)
        ok = mxv >= 0.5
        if ok:
            self.vel = new - self.pos
            self.pos = new
            self.t = new - complex(self.x0, self.y0)
        return TrackFrame(k, 1 + 0j, self.t, float(max(0.0, mxv)), ok)


def _parabola(r, px, py, axis):
    if axis == 0:
        if px <= 0 or px >= r.shape[1] - 1:
            return 0.0
        a, b, c = r[py, px - 1], r[py, px], r[py, px + 1]
    else:
        if py <= 0 or py >= r.shape[0] - 1:
            return 0.0
        a, b, c = r[py - 1, px], r[py, px], r[py + 1, px]
    den = a - 2 * b + c
    return float(0.5 * (a - c) / den) if abs(den) > 1e-9 else 0.0


# ---------------------------------------------------------------------------
# ECC izleyici (assmotion xtrack)
# ---------------------------------------------------------------------------

class _EccRunner:
    def __init__(self, ref_img, roi, min_cc=0.6):
        import cv2
        self.cv2 = cv2
        H, W = ref_img.shape[:2]
        x, y, w, h = roi
        x0, y0 = int(max(0, round(x))), int(max(0, round(y)))
        x1, y1 = int(min(W, round(x + w))), int(min(H, round(y + h)))
        if x1 - x0 < 8 or y1 - y0 < 8:
            raise ValueError("İzleme alanı görüntünün dışında")
        tpl = ref_img[y0:y1, x0:x1]
        if float(tpl.std()) < 3.0:
            raise ValueError("Seçilen alan düz; ECC için desen gerekir")
        # ECC için kesit en çok ~200 px olsun
        self.s = min(1.0, 200.0 / max(x1 - x0, y1 - y0))
        self.tpl = self._prep(tpl)
        self.o = complex(x0, y0)           # şablonun referans karedeki sol üstü
        self.tw, self.th = x1 - x0, y1 - y0
        self.min_cc = min_cc
        self.notes: list[str] = []

    def _prep(self, img):
        im = img.astype(np.float32)
        if self.s != 1.0:
            im = self.cv2.resize(im, None, fx=self.s, fy=self.s, interpolation=self.cv2.INTER_AREA)
        return im

    def reset(self):
        self.c, self.t = 1 + 0j, 0j        # ref -> kare benzerliği
        self.prev = (self.c, self.t)

    def step(self, img, k) -> TrackFrame:
        cv2 = self.cv2
        H, W = img.shape[:2]
        c, t = self.c, self.t
        # şablonun bu karedeki tahmini kutusu
        corners = [self.o, self.o + self.tw, self.o + complex(self.tw, self.th), self.o + 1j * self.th]
        zs = [c * z + t for z in corners]
        mx = 0.5 * max(self.tw, self.th) * abs(c)
        wx0 = int(max(0, min(z.real for z in zs) - mx))
        wy0 = int(max(0, min(z.imag for z in zs) - mx))
        wx1 = int(min(W, max(z.real for z in zs) + mx))
        wy1 = int(min(H, max(z.imag for z in zs) + mx))
        if wx1 - wx0 < 8 or wy1 - wy0 < 8:
            return TrackFrame(k, c, t, 0.0, False)
        win = self._prep(img[wy0:wy1, wx0:wx1])
        # şablon yerel (küçültülmüş) u -> pencere yerel (küçültülmüş) v:
        # z_ref = o + u/s ; z = c z_ref + t ; v = s (z − w0)  =>  v = c u + s (c o + t − w0)
        w0 = complex(wx0, wy0)
        tt = self.s * (c * self.o + t - w0)
        warp = np.array([[c.real, -c.imag, tt.real], [c.imag, c.real, tt.imag]], np.float32)
        try:
            cc, warp = cv2.findTransformECC(self.tpl, win, warp, cv2.MOTION_AFFINE,
                                            (cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_COUNT, 60, 1e-4),
                                            None, 5)
        except cv2.error:
            return TrackFrame(k, c, t, 0.0, False)
        a00, a01, a10, a11 = float(warp[0, 0]), float(warp[0, 1]), float(warp[1, 0]), float(warp[1, 1])
        cn = complex((a00 + a11) / 2, (a10 - a01) / 2)        # en yakın benzerlik
        ttn = complex(float(warp[0, 2]), float(warp[1, 2]))
        tn = ttn / self.s - cn * self.o + w0
        ds = abs(cn) / max(1e-9, abs(c))
        dr = abs(np.degrees(np.angle(cn / c))) if c != 0 else 0.0
        ok = cc >= self.min_cc and 0.85 < ds < 1.15 and dr < 10.0
        if ok:
            self.c, self.t = cn, tn
        else:
            self.notes.append("ECC bazı karelerde reddedildi (düşük korelasyon ya da ani ölçek/dönüş)")
        return TrackFrame(k, self.c, self.t, float(max(0.0, cc)), ok)


# ---------------------------------------------------------------------------
# Renk anahtarlı izleyici (assmotion track)
# ---------------------------------------------------------------------------

class _ColorRunner:
    HUE_TOL = 10
    S_MIN = 60
    V_MIN = 40

    def __init__(self, hsv, roi, key_rgb=None):
        import cv2
        self.cv2 = cv2
        self._kern = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (5, 5))
        H, W = hsv.shape[:2]
        x, y, w, h = roi
        x0, y0 = int(max(0, round(x))), int(max(0, round(y)))
        x1, y1 = int(min(W, round(x + w))), int(min(H, round(y + h)))
        crop = hsv[y0:y1, x0:x1]
        if key_rgb is not None:
            px = np.uint8([[list(key_rgb)]])
            self.hue = int(cv2.cvtColor(px, cv2.COLOR_RGB2HSV)[0, 0, 0])
        else:
            sat = crop[(crop[..., 1] >= self.S_MIN) & (crop[..., 2] >= self.V_MIN)]
            if len(sat) < 0.05 * crop.shape[0] * crop.shape[1]:
                raise ValueError("Alanda baskın doygun renk yok; renk izlemesi için renkli bir tabela seçin "
                                 "ya da Noktalar/ECC yöntemini kullanın")
            hist = np.bincount(sat[:, 0].astype(int), minlength=180).astype(float)
            k = np.ones(2 * self.HUE_TOL + 1)
            sm = np.convolve(np.concatenate([hist[-self.HUE_TOL:], hist, hist[:self.HUE_TOL]]), k, "valid")
            hue = int(np.argmax(sm))
            # ince ayar: tepe çevresindeki tonların dairesel ortalaması (iki kez)
            hs = sat[:, 0].astype(float)
            for _ in range(2):
                d = (hs - hue + 90) % 180 - 90
                sel = np.abs(d) <= self.HUE_TOL
                if sel.any():
                    hue = int(round(hue + float(np.mean(d[sel])))) % 180
            self.hue = hue
        m = self._mask(crop)
        # biçim: güçlü açmayla (tabela boyunun ~%5'i) kenara değen kırıntılardan ayrılmış en büyük
        # bileşen, delikleri (yazı) doldurulmuş hali
        r = max(2, int(0.025 * min(crop.shape[:2])))
        big = cv2.morphologyEx(m, cv2.MORPH_OPEN, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * r + 1, 2 * r + 1)))
        n, lab, st, cen = cv2.connectedComponentsWithStats(big)
        if n <= 1:
            raise ValueError("Anahtar renk alanda bulunamadı")
        j = 1 + int(np.argmax(st[1:, cv2.CC_STAT_AREA]))
        comp = (lab == j).astype(np.uint8)
        cnts, _ = cv2.findContours(comp, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
        shape = np.zeros_like(comp)
        cv2.drawContours(shape, cnts, -1, 1, -1)
        shape = cv2.dilate(shape, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (3, 3)))
        self.shape = shape
        self.so = complex(x0, y0)                     # biçimin referans karedeki sol üstü
        comp = (m & shape).astype(np.uint8)            # anahtar pikseller (yazı delikleri hariç)
        self.a0 = float(comp.sum())
        ys, xs = np.nonzero(comp)
        self.z0 = complex(xs.mean() + x0, ys.mean() + y0)
        self.size = float(np.sqrt(self.a0))
        ys2, xs2 = np.nonzero(comp)
        L, R = np.percentile(xs2, 1), np.percentile(xs2, 99) + 1
        T_, B = np.percentile(ys2, 1), np.percentile(ys2, 99) + 1
        self.bw0, self.bh0 = float(R - L), float(B - T_)
        self.bc0 = complex((L + R) / 2 + x0, (T_ + B) / 2 + y0)     # kutu merkezi (ref koordinatı)
        self.aspect0 = self.bw0 / max(1.0, self.bh0)
        self.fill0 = self.a0 / max(1.0, float(shape.sum()))      # biçimin anahtar renkli oranı (yazı delikleri)
        self.notes = [f"anahtar ton {self.hue * 2}°"]

    def _mask(self, hsv):
        h = hsv[..., 0].astype(int)
        d = np.abs(h - self.hue)
        d = np.minimum(d, 180 - d)
        m = (d <= self.HUE_TOL) & (hsv[..., 1] >= self.S_MIN) & (hsv[..., 2] >= self.V_MIN)
        m = m.astype(np.uint8)
        # 5x5 açma: tabelaya kenardan değen aynı tonlu arka plan kırıntılarını ince köprülerden ayırır
        return self.cv2.morphologyEx(m, self.cv2.MORPH_OPEN, self._kern)

    def reset(self):
        self.vel = 0j
        self.hist = []          # (k, log ölçek) tam görünen kareler
        self.c, self.t = 1 + 0j, 0j
        rk = getattr(self, "ref_k", None)
        self.goods = [(rk, 0j)] if rk is not None else []      # (kare, t) son tam görünen kareler
        self.misses = 0

    def step(self, hsv, k) -> TrackFrame:
        """Yalnız TAM görünen kareler ölçülür: anahtar maske, tahmin edilen yerdeki tabela
        biçiminin (biraz genişletilmiş) içinde aranır; kutunun en/boy oranı referanstakiyle
        uyuşuyor, görünen oran ≥ %93 ve ölçek eğilime yakınsa ağırlık merkezi -> konum,
        √(alan/alan_ref) -> ölçek. Aksi halde kare güvensizdir ve ara değerle doldurulur
        (kısmi örtmede renk tek başına örtülen kenarı bilemez; kenarı tahmin etmek yerine
        dürüstçe işaretlenir). Tahmin son tam görünen kareden sabit hızla yapılır."""
        cv2 = self.cv2
        H, W = hsv.shape[:2]
        exp = self._expected(k)
        s_pred = float(np.exp(exp)) if exp is not None else abs(self.c)
        t_pred = self._predict(k)
        if self.misses >= 2:
            hit = self._reacquire(hsv, k, s_pred, t_pred)
            if hit is not None:
                return hit
        sh_, sw_ = self.shape.shape
        tl = s_pred * self.so + t_pred
        pad = int(max(4.0, 0.08 * self.size * s_pred) + min(40.0, abs(self.vel) * 2 + 3 * self.misses))
        x0, y0 = int(max(0, tl.real - pad)), int(max(0, tl.imag - pad))
        x1 = int(min(W, tl.real + sw_ * s_pred + pad))
        y1 = int(min(H, tl.imag + sh_ * s_pred + pad))
        if x1 - x0 < 4 or y1 - y0 < 4:
            self.t = t_pred
            self.misses += 1
            return TrackFrame(k, self.c, self.t, 0.0, False)
        tt = tl - complex(x0, y0)
        Mx = np.array([[s_pred, 0, tt.real], [0, s_pred, tt.imag]], np.float64)
        region = cv2.warpAffine(self.shape, Mx, (x1 - x0, y1 - y0), flags=cv2.INTER_NEAREST)
        region = cv2.dilate(region, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * pad + 1, 2 * pad + 1)))
        m = self._mask(hsv[y0:y1, x0:x1]) & region
        n, lab, st, _c = cv2.connectedComponentsWithStats(m)
        if n > 2:
            bigc = st[1:, cv2.CC_STAT_AREA].max()
            m = np.isin(lab, [jj for jj in range(1, n) if st[jj, cv2.CC_STAT_AREA] >= 0.1 * bigc])
        ys, xs = np.nonzero(m)
        if len(xs) < 12:
            self.t = t_pred
            self.misses += 1
            return TrackFrame(k, self.c, self.t, 0.0, False)
        return self._measure(k, xs + x0, ys + y0, s_pred, t_pred, exp)

    def _measure(self, k, xs, ys, s_pred, t_pred, exp):
        area = float(len(xs))
        sc = float(np.sqrt(area / self.a0))
        bw = np.percentile(xs, 99) - np.percentile(xs, 1) + 1
        bh = np.percentile(ys, 99) - np.percentile(ys, 1) + 1
        aspect_ok = abs(np.log((bw / max(1.0, bh)) / self.aspect0)) < 0.04
        vis = area / max(1.0, self.a0 * s_pred ** 2)
        trend_ok = exp is None or abs(np.log(sc) - exp) < 0.05
        if not (aspect_ok and vis >= 0.93 and trend_ok):
            self.t = t_pred
            self.c = complex(s_pred, 0)
            self.misses += 1
            return TrackFrame(k, self.c, self.t, float(min(0.99, vis)), False)
        z = complex(xs.mean(), ys.mean())
        cn = complex(sc, 0)
        tn = z - cn * self.z0
        self.hist.append((k, float(np.log(sc))))
        self.goods.append((k, tn))
        self.goods = self.goods[-8:]
        self.misses = 0
        self.c, self.t = cn, tn
        return TrackFrame(k, self.c, self.t, 1.0, True)

    def _predict(self, k):
        """Son tam görünen karelerin konumuna doğrusal uydurma (gürültüye dayanıklı hız)."""
        g = self.goods
        if not g:
            return self.t
        if len(g) < 3:
            if len(g) == 2:
                (k1, t1), (k2, t2) = g
                return t2 + (t2 - t1) / max(1, abs(k2 - k1)) * abs(k - k2)
            return g[-1][1]
        ks = np.array([x[0] for x in g], float)
        A = np.vstack([ks, np.ones_like(ks)]).T
        out = []
        for part in (np.array([x[1].real for x in g]), np.array([x[1].imag for x in g])):
            (mm, bb), *_ = np.linalg.lstsq(A, part, rcond=None)
            out.append(mm * k + bb)
        self.vel = complex(abs(out[0] - g[-1][1].real), abs(out[1] - g[-1][1].imag)) / max(1, abs(k - g[-1][0]))
        return complex(out[0], out[1])

    def _reacquire(self, hsv, k, s_pred, t_pred):
        """Örtmeden sonra tabelayı geniş pencerede yeniden bul: boyutu ve en/boy oranı tutan,
        tahmine en yakın bileşen."""
        cv2 = self.cv2
        H, W = hsv.shape[:2]
        cp = s_pred * self.z0 + t_pred
        r = 1.5 * self.size * s_pred + 8 * self.misses
        x0, y0 = int(max(0, cp.real - r)), int(max(0, cp.imag - r))
        x1, y1 = int(min(W, cp.real + r)), int(min(H, cp.imag + r))
        if x1 - x0 < 8 or y1 - y0 < 8:
            return None
        m = self._mask(hsv[y0:y1, x0:x1])
        n, lab, st, cen = cv2.connectedComponentsWithStats(m)
        best, bd = None, None
        want = self.a0 * s_pred ** 2
        for j in range(1, n):
            a = st[j, cv2.CC_STAT_AREA]
            if not (0.8 * want <= a <= 1.25 * want):
                continue
            asp = st[j, cv2.CC_STAT_WIDTH] / max(1, st[j, cv2.CC_STAT_HEIGHT])
            if abs(np.log(asp / self.aspect0)) > 0.12:
                continue
            d = abs(complex(*cen[j]) + complex(x0, y0) - cp)
            if bd is None or d < bd:
                best, bd = j, d
        if best is None:
            return None
        ys, xs = np.nonzero(lab == best)
        tf = self._measure(k, xs + x0, ys + y0, s_pred, t_pred, None)
        return tf if tf.ok else None

    def _expected(self, k):
        """Tam görünen son karelerden log-ölçek doğrusal uydurması (en çok 12 kare)."""
        h = self.hist[-12:]
        if not h:
            return None
        if len(h) < 4:
            return h[-1][1]
        ks = np.array([x[0] for x in h], float)
        ls = np.array([x[1] for x in h], float)
        A = np.vstack([ks, np.ones_like(ks)]).T
        (m, b), *_ = np.linalg.lstsq(A, ls, rcond=None)
        return float(m * k + b)
