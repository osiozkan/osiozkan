"""Hareket izi (motion track) modeli, dosya biçimleri ve satıra uygulama.

İz, her kare için REFERANS KAREDEN o kareye giden bir BENZERLİK dönüşümüdür
(konum + eşit ölçek + dönüş). Karmaşık sayıyla yazılır:

    z' = c · z + t        z = x + i·y  (video pikseli, y aşağı)
    c = a + i·b           ölçek = |c|, görüntü açısı = arg(c) (y aşağıda saat yönü)

ASS'te \\frz saat yönünün TERSİNE pozitiftir, bu yüzden \\frz farkı = -deg(arg c).
Referans karede c = 1, t = 0'dır. Satır referans karede doğru yerleştirilmiş
kabul edilir; diğer karelerdeki \\pos, \\org, \\clip noktaları bu dönüşümle taşınır,
\\fscx/\\fscy/\\bord/\\shad/\\blur ölçekle çarpılır, \\frz'ye açı farkı eklenir.

Dosya biçimleri
  .assmotiontrack  OsiSub'ın JSON biçimi (aşağıda `to_json`). Osi'nin assmotion aracının
                   özgün biçimi elimizde olmadığı için bu biçim burada tanımlandı; özgün
                   araç yüklenirse ona uyarlanır.
  AE Keyframe Data Mocha / After Effects "Adobe After Effects 6.0 Keyframe Data"
                   (Aegisub-Motion'ın okuduğu biçim). İçe ve dışa aktarılır, böylece
                   OsiSub'da izlenen veri gerçek Aegisub'da Aegisub-Motion ile de kullanılabilir.

Uygulama kipleri
  frame    her kareye bir satır (Aegisub-Motion gibi; her durumda doğru)
  compact  doğrusal giden parçalar tek satır: \\move + \\t(\\fscx\\fscy\\frz...) — dosya
           çok daha küçük; parçalar `tol_*` hoşgörüsü içinde gerçek izden sapmaz.
           Vektör clip ya da satırda zaten \\t varsa kare kare kipe düşülür (uyarıyla).
"""

from __future__ import annotations

import cmath
import json
import math
import re
from dataclasses import dataclass, field

from . import tags as T
from .asstime import round_ms

FORMAT = "assmotiontrack"
VERSION = 1


# ---------------------------------------------------------------------------
# Model
# ---------------------------------------------------------------------------

@dataclass
class TrackFrame:
    frame: int
    c: complex = 1 + 0j
    t: complex = 0j
    conf: float = 1.0
    ok: bool = True

    def map(self, x: float, y: float) -> tuple[float, float]:
        z = self.c * complex(x, y) + self.t
        return z.real, z.imag

    @property
    def scale(self) -> float:
        return abs(self.c)

    @property
    def rot(self) -> float:
        """ASS \\frz anlamında açı farkı (derece, saat yönü tersine +)."""
        return -math.degrees(cmath.phase(self.c)) if self.c != 0 else 0.0


@dataclass
class MotionTrack:
    frames: list[TrackFrame]
    ref_frame: int
    width: int = 0
    height: int = 0
    fps: float | None = None
    roi: tuple | None = None            # (x, y, w, h) referans karede, video pikseli
    method: str = ""
    video: str = ""
    notes: list[str] = field(default_factory=list)
    occlusion: dict | None = None       # media.occlude.OcclusionResult.to_json() (isteğe bağlı)

    # ---- erişim
    @property
    def start_frame(self) -> int:
        return self.frames[0].frame if self.frames else 0

    @property
    def end_frame(self) -> int:
        return self.frames[-1].frame if self.frames else -1

    def __len__(self):
        return len(self.frames)

    def at(self, frame: int) -> TrackFrame:
        """Karedeki dönüşüm; iz dışındaysa en yakın uç (tutma)."""
        if not self.frames:
            return TrackFrame(frame)
        k = frame - self.start_frame
        k = max(0, min(k, len(self.frames) - 1))
        return self.frames[k]

    def covers(self, frame: int) -> bool:
        return self.start_frame <= frame <= self.end_frame

    def bad_frames(self) -> list[int]:
        return [f.frame for f in self.frames if not f.ok]

    def summary(self) -> str:
        if not self.frames:
            return "boş iz"
        sc = [f.scale for f in self.frames]
        rt = [f.rot for f in self.frames]
        mv = max(abs(f.t) for f in self.frames)
        s = (f"{len(self.frames)} kare ({self.start_frame}–{self.end_frame}), referans {self.ref_frame}; "
             f"ölçek {min(sc) * 100:.1f}–{max(sc) * 100:.1f}%, dönüş {min(rt):.1f}°…{max(rt):.1f}°, "
             f"en büyük kayma {mv:.1f} px")
        bad = len(self.bad_frames())
        if bad:
            s += f"; {bad} kare güvensiz (ara değerle dolduruldu)"
        return s

    # ---- dönüşümler
    def rebased(self, ref: int) -> "MotionTrack":
        """Referansı başka kareye taşı: M'_k = M_k ∘ M_ref⁻¹."""
        r = self.at(ref)
        if r.c == 0:
            return self
        out = []
        for f in self.frames:
            c = f.c / r.c
            out.append(TrackFrame(f.frame, c, f.t - c * r.t, f.conf, f.ok))
        return self._copy(out, ref_frame=max(self.start_frame, min(ref, self.end_frame)))

    def filled(self) -> "MotionTrack":
        """Güvensiz kareleri komşu güvenli karelerden doğrusal ara değerle doldur."""
        fr = self.frames
        good = [i for i, f in enumerate(fr) if f.ok]
        if not good or len(good) == len(fr):
            return self
        out = []
        for i, f in enumerate(fr):
            if f.ok:
                out.append(f)
                continue
            prev = max((g for g in good if g < i), default=None)
            nxt = min((g for g in good if g > i), default=None)
            if prev is None:
                src = fr[nxt]
                out.append(TrackFrame(f.frame, src.c, src.t, f.conf, False))
            elif nxt is None:
                src = fr[prev]
                out.append(TrackFrame(f.frame, src.c, src.t, f.conf, False))
            else:
                u = (i - prev) / (nxt - prev)
                a, b = fr[prev], fr[nxt]
                out.append(TrackFrame(f.frame, _lerp_c(a.c, b.c, u), a.t + (b.t - a.t) * u, f.conf, False))
        return self._copy(out)

    def smoothed(self, radius: int) -> "MotionTrack":
        """Kayan ortalama (2·radius+1 kare, uçlarda simetrik daralır); referans karesi yeniden kimliğe çekilir."""
        if radius <= 0 or len(self.frames) < 3:
            return self
        src = self.filled().frames
        n = len(src)
        # açı sarmasını önlemek için c'yi log-kutupsal biçimde ortala
        logs = []
        prev_ang = 0.0
        for f in src:
            ang = cmath.phase(f.c) if f.c != 0 else 0.0
            while ang - prev_ang > math.pi:
                ang -= 2 * math.pi
            while ang - prev_ang < -math.pi:
                ang += 2 * math.pi
            prev_ang = ang
            logs.append((math.log(max(1e-9, abs(f.c))), ang))
        out = []
        for i, f in enumerate(src):
            r = min(radius, i, n - 1 - i)          # uçlarda simetrik daralan pencere (kayma yapmaz)
            a, b = i - r, i + r + 1
            k = b - a
            ls = sum(x[0] for x in logs[a:b]) / k
            an = sum(x[1] for x in logs[a:b]) / k
            t = sum((x.t for x in src[a:b]), 0j) / k
            out.append(TrackFrame(f.frame, cmath.rect(math.exp(ls), an), t, f.conf, f.ok))
        return self._copy(out).rebased(self.ref_frame)

    def _copy(self, frames, **kw) -> "MotionTrack":
        d = dict(frames=frames, ref_frame=self.ref_frame, width=self.width, height=self.height,
                 fps=self.fps, roi=self.roi, method=self.method, video=self.video, notes=list(self.notes),
                 occlusion=self.occlusion)
        d.update(kw)
        return MotionTrack(**d)

    # ---- .assmotiontrack (JSON)
    def to_json(self) -> dict:
        return {
            "format": FORMAT, "version": VERSION,
            "video": self.video, "width": self.width, "height": self.height, "fps": self.fps,
            "method": self.method, "ref_frame": self.ref_frame,
            "roi": list(self.roi) if self.roi else None,
            "notes": self.notes,
            # a,b,tx,ty: x' = a·x − b·y + tx ; y' = b·x + a·y + ty  (ref kare -> bu kare)
            # x,y/scale/rot okunabilirlik için (ROI merkezinin konumu, %, \\frz farkı)
            "frames": [self._frame_json(f) for f in self.frames],
            **({"occlusion": self.occlusion} if self.occlusion else {}),
        }

    def _frame_json(self, f: TrackFrame) -> dict:
        d = {"f": f.frame, "a": round(f.c.real, 7), "b": round(f.c.imag, 7),
             "tx": round(f.t.real, 4), "ty": round(f.t.imag, 4),
             "conf": round(f.conf, 3), "ok": f.ok}
        if self.roi:
            cx, cy = self.roi[0] + self.roi[2] / 2, self.roi[1] + self.roi[3] / 2
            x, y = f.map(cx, cy)
            d.update(x=round(x, 3), y=round(y, 3))
        d.update(scale=round(f.scale * 100, 4), rot=round(f.rot, 4))
        return d

    @classmethod
    def from_json(cls, d: dict) -> "MotionTrack":
        if d.get("format") != FORMAT:
            raise ValueError("Bu dosya bir .assmotiontrack değil")
        if int(d.get("version", 1)) > VERSION:
            raise ValueError(f"Daha yeni bir sürümün dosyası (v{d.get('version')})")
        frames = []
        for x in d["frames"]:
            if "a" in x:
                c, t = complex(x["a"], x["b"]), complex(x["tx"], x["ty"])
            else:   # yalnız x/y/scale/rot yazılmış elle hazırlanmış dosyalar
                c = cmath.rect(float(x.get("scale", 100)) / 100, -math.radians(float(x.get("rot", 0))))
                t = complex(x.get("x", 0), x.get("y", 0))
                frames.append(TrackFrame(int(x["f"]), c, t, float(x.get("conf", 1)), bool(x.get("ok", True))))
                continue
            frames.append(TrackFrame(int(x["f"]), c, t, float(x.get("conf", 1)), bool(x.get("ok", True))))
        frames.sort(key=lambda f: f.frame)
        tr = cls(frames, int(d.get("ref_frame", frames[0].frame if frames else 0)), int(d.get("width") or 0),
                 int(d.get("height") or 0), d.get("fps"), tuple(d["roi"]) if d.get("roi") else None,
                 d.get("method", ""), d.get("video", ""), list(d.get("notes") or []), d.get("occlusion"))
        if frames and "a" not in d["frames"][0]:
            # mutlak x/y -> referansa göre dönüşüm
            r = tr.at(tr.ref_frame)
            out = []
            for f in frames:
                c = f.c / r.c
                out.append(TrackFrame(f.frame, c, f.t - c * r.t, f.conf, f.ok))
            tr.frames = out
        return tr

    def save(self, path: str):
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(self.to_json(), fh, ensure_ascii=False, indent=1)
            fh.write("\n")

    @classmethod
    def load(cls, path: str) -> "MotionTrack":
        with open(path, encoding="utf-8-sig") as fh:
            head = fh.read(64)
            fh.seek(0)
            if head.lstrip().startswith("Adobe After Effects"):
                return from_ae(fh.read())
            return cls.from_json(json.load(fh))

    # ---- AE Keyframe Data
    def to_ae(self, anchor: tuple[float, float] | None = None) -> str:
        if anchor is None:
            if self.roi:
                anchor = (self.roi[0] + self.roi[2] / 2, self.roi[1] + self.roi[3] / 2)
            else:
                anchor = (self.width / 2, self.height / 2)
        ax, ay = anchor
        fps = self.fps or 23.976
        lines = ["Adobe After Effects 6.0 Keyframe Data", "",
                 f"\tUnits Per Second\t{_num(fps, 6)}",
                 f"\tSource Width\t{self.width}", f"\tSource Height\t{self.height}",
                 "\tSource Pixel Aspect Ratio\t1", "\tComp Pixel Aspect Ratio\t1", ""]
        f0 = self.start_frame
        lines += ["Anchor Point", "\tFrame\tX pixels\tY pixels\tZ pixels\t"]
        lines += [f"\t{f.frame - f0}\t{_num(ax)}\t{_num(ay)}\t0\t" for f in self.frames]
        lines += ["", "Position", "\tFrame\tX pixels\tY pixels\tZ pixels\t"]
        for f in self.frames:
            x, y = f.map(ax, ay)
            lines.append(f"\t{f.frame - f0}\t{_num(x)}\t{_num(y)}\t0\t")
        lines += ["", "Scale", "\tFrame\tX percent\tY percent\tZ percent\t"]
        lines += [f"\t{f.frame - f0}\t{_num(f.scale * 100)}\t{_num(f.scale * 100)}\t100\t" for f in self.frames]
        lines += ["", "Rotation", "\tFrame\tDegrees\t"]
        lines += [f"\t{f.frame - f0}\t{_num(-f.rot)}\t" for f in self.frames]
        lines += ["", "End of Keyframe Data", ""]
        return "\r\n".join(lines)


def _num(v: float, d: int = 4) -> str:
    return T.fmt(v, d)


def _lerp_c(a: complex, b: complex, u: float) -> complex:
    """Ölçek ve açıyı ayrı ayrı ara değerle (doğrudan karmaşık ara değer ölçeği küçültür)."""
    if a == 0 or b == 0:
        return a + (b - a) * u
    ra, rb = abs(a), abs(b)
    pa, pb = cmath.phase(a), cmath.phase(b)
    d = (pb - pa + math.pi) % (2 * math.pi) - math.pi
    return cmath.rect(ra + (rb - ra) * u, pa + d * u)


def from_ae(text: str, start_frame: int = 0, ref_frame: int | None = None) -> MotionTrack:
    """Adobe After Effects 6.0 Keyframe Data -> MotionTrack.

    Veri karelerinin ilki `start_frame`'e karşılık gelir (Aegisub-Motion gibi: veri
    satırın ilk karesinden başlar). Referans varsayılan olarak ilk karedir.
    """
    if "Adobe After Effects" not in text[:200]:
        raise ValueError("AE Keyframe Data başlığı bulunamadı")
    head = {}
    sections: dict[str, dict[int, list[float]]] = {}
    cur = None
    for raw in text.splitlines():
        if not raw.strip():
            continue
        s = raw.strip()
        if s.startswith("Adobe After Effects") or s.startswith("End of Keyframe Data"):
            cur = None
            continue
        if not raw[0].isspace():
            cur = s
            sections.setdefault(cur, {})
            continue
        parts = s.split("\t") if "\t" in s else s.split()
        parts = [p for p in parts if p.strip()]
        if cur is None:
            if len(parts) >= 2:
                head[parts[0].strip()] = parts[-1].strip()
            continue
        if parts[0].strip().lower() == "frame":
            continue
        try:
            nums = [float(p) for p in parts]
        except ValueError:
            continue
        sections[cur][int(round(nums[0]))] = nums[1:]
    pos = sections.get("Position", {})
    if not pos:
        raise ValueError("AE verisinde Position bölümü yok")
    anchor = sections.get("Anchor Point", {})
    scale = sections.get("Scale", {})
    rot = sections.get("Rotation", {})
    keys = sorted(pos)
    k0 = keys[0]
    notes = []

    def val(sec, k, default):
        if k in sec:
            return sec[k]
        if not sec:
            return default
        # eksik kare: en yakın değer (tutma)
        kk = min(sec, key=lambda x: abs(x - k))
        return sec[kk]

    layers = {}
    nonuni = False
    for k in range(k0, keys[-1] + 1):
        p = val(pos, k, [0, 0])
        a = val(anchor, k, [0, 0])
        sc = val(scale, k, [100, 100])
        r = val(rot, k, [0])
        sx = sc[0] if sc else 100
        sy = sc[1] if len(sc) > 1 else sx
        if abs(sx - sy) > 0.5:
            nonuni = True
        s = (sx + sy) / 200.0
        c = cmath.rect(s, math.radians(r[0] if r else 0))
        layers[k] = (complex(p[0], p[1]), complex(a[0], a[1]), c, k in pos)
    if nonuni:
        notes.append("X/Y ölçeği farklı; ortalaması kullanıldı")
    try:
        fps = float(head.get("Units Per Second", "0")) or None
    except ValueError:
        fps = None
    w = int(float(head.get("Source Width", "0") or 0))
    h = int(float(head.get("Source Height", "0") or 0))
    ref_rel = (ref_frame - start_frame) if ref_frame is not None else 0
    ref_rel = max(0, min(ref_rel, keys[-1] - k0))
    P_r, A_r, c_r, _ = layers[k0 + ref_rel]
    frames = []
    for k in range(k0, keys[-1] + 1):
        P, A, c, present = layers[k]
        # L_k(q) = P + c (q − A);  M_k = L_k ∘ L_ref⁻¹
        cc = c / c_r
        # L_ref⁻¹(z) = A_r + (z − P_r)/c_r ;  L_k(L_ref⁻¹(z)) = P + c(A_r − A) + cc (z − P_r)
        t = P + c * (A_r - A) - cc * P_r
        frames.append(TrackFrame(start_frame + (k - k0), cc, t, 1.0 if present else 0.0, present))
    tr = MotionTrack(frames, start_frame + ref_rel, w, h, fps, None, "ae", "", notes)
    return tr


# ---------------------------------------------------------------------------
# Satıra uygulama
# ---------------------------------------------------------------------------

@dataclass
class ApplyOptions:
    position: bool = True
    scale: bool = True
    rotation: bool = True
    border: bool = True           # \bord \shad \blur ölçekle
    clip: bool = True
    origin: bool = True           # \org'u da taşı
    mode: str = "frame"           # "frame" | "compact"
    ref_frame: int | None = None  # None: izin kendi referansı
    tol_pos: float = 0.5          # px (betik)
    tol_scale: float = 0.5        # % (\fscx birimi)
    tol_rot: float = 0.3          # derece
    decimals: int = 2
    occlusion: object = None      # media.occlude.OcclusionResult: örtülen karelerde \iclip
    hide_below: float = 0.0       # görünürlük bu oranın altındaysa o karede satır üretme (0 = kapalı)


class MotionError(ValueError):
    pass


_SCALE_TAGS = ("fscx", "fscy")
_BORDER_TAGS = ("bord", "xbord", "ybord", "shad", "xshad", "yshad", "blur")


def _split_args(s: str) -> list[str]:
    out, depth, cur = [], 0, []
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append("".join(cur))
    return out


def _is_num(s: str) -> bool:
    return bool(re.fullmatch(r"\s*-?\d+(?:\.\d+)?\s*", s))


def _blocks(text: str):
    """[(is_override, metin)] — {…} blokları ve aradaki düz metin."""
    out = []
    i = 0
    while i < len(text):
        j = text.find("{", i)
        if j < 0:
            out.append((False, text[i:]))
            break
        if j > i:
            out.append((False, text[i:j]))
        k = text.find("}", j)
        if k < 0:
            out.append((False, text[j:]))
            break
        out.append((True, text[j + 1:k]))
        i = k + 1
    return out


def _join(blocks) -> str:
    return "".join("{" + s + "}" if ov else s for ov, s in blocks)


def _map_inner(inner: str, fn, in_t: bool = False) -> str:
    toks = T.tokens(inner)
    if not toks:
        return inner
    out = [inner[:toks[0][1]]]
    for idx, (name, s, e, arg) in enumerate(toks):
        rep = None
        if name == "t":
            rep = _map_t(arg, fn)
        else:
            rep = fn(T.canon(name), name, arg, in_t)
        out.append(inner[s:e] if rep is None else rep)
        nxt = toks[idx + 1][1] if idx + 1 < len(toks) else len(inner)
        out.append(inner[e:nxt])
    return "".join(out)


def _map_t(arg: str, fn):
    a = arg.strip()
    if not (a.startswith("(") and a.endswith(")")):
        return None
    parts = _split_args(a[1:-1])
    nums = []
    while parts and _is_num(parts[0]) and len(nums) < 3:
        nums.append(parts.pop(0).strip())
    body = ",".join(parts)
    res = fn("t", "t", nums, False)          # zaman kaydırma (nums listesini döndürür)
    if res is not None:
        nums = res
    body = _map_inner(body, fn, True)
    return "\\t(" + ",".join(list(nums) + [body]) + ")"


@dataclass
class _Base:
    pos: tuple[float, float]
    fscx: float
    fscy: float
    frz: float
    bord: float
    shad: float
    has_pos: bool
    has_fscx: bool
    has_fscy: bool
    has_frz: bool
    has_bord: bool
    has_shad: bool


def _base(ev, style, play_res) -> _Base:
    text = ev.text
    if T.get_move(text):
        raise MotionError("Satırda \\move var; önce \\pos'a çevirin (izleme konumu kendisi üretir)")
    p = T.get_pos(text)
    pos = p or T.default_pos(ev, style, play_res)
    st = style
    g = T.get
    return _Base(
        pos=pos,
        fscx=T.get_float(text, "fscx", st.scale_x if st else 100.0),
        fscy=T.get_float(text, "fscy", st.scale_y if st else 100.0),
        frz=T.get_float(text, "frz", st.angle if st else 0.0),
        bord=T.get_float(text, "bord", st.outline if st else 2.0),
        shad=T.get_float(text, "shad", st.shadow if st else 2.0),
        has_pos=p is not None, has_fscx=g(text, "fscx") is not None, has_fscy=g(text, "fscy") is not None,
        has_frz=g(text, "frz") is not None,
        has_bord=any(g(text, n) is not None for n in ("bord", "xbord", "ybord")),
        has_shad=any(g(text, n) is not None for n in ("shad", "xshad", "yshad")),
    )


class _Mapper:
    """Betik koordinatı <-> video pikseli + iz dönüşümü."""

    def __init__(self, play_res, vid_size):
        prx, pry = play_res
        vw, vh = vid_size if vid_size and vid_size[0] else play_res
        self.kx = vw / prx
        self.ky = vh / pry

    def map(self, tf: TrackFrame, x: float, y: float) -> tuple[float, float]:
        vx, vy = tf.map(x * self.kx, y * self.ky)
        return vx / self.kx, vy / self.ky


def shift_times(text: str, offset: int, orig_dur: int) -> str:
    """Satır başlangıcı `offset` ms ileri alındığında \\t, \\fad, \\fade zamanlarını düzelt.

    \\t(süresiz) özgün satır süresine açıklanır; \\fad, \\fade(255,0,255,…) olur.
    """
    if offset == 0:
        return text

    def fn(cn, name, arg, in_t):
        if cn == "t":
            nums = list(arg)
            if len(nums) in (0, 1):            # \t(tags) / \t(accel,tags)
                return [str(-offset), str(orig_dur - offset)] + nums
            t1, t2 = float(nums[0]), float(nums[1])
            return [T.fmt(t1 - offset, 0), T.fmt(t2 - offset, 0)] + nums[2:]
        if in_t:
            return None
        if cn == "fad":
            v = T._nums(arg)
            if len(v) >= 2:
                fin, fout = v[0], v[1]
                ts = (-offset, fin - offset, orig_dur - fout - offset, orig_dur - offset)
                return "\\fade(255,0,255," + ",".join(T.fmt(x, 0) for x in ts) + ")"
        if cn == "fade":
            v = T._nums(arg)
            if len(v) >= 7:
                a = [T.fmt(x, 0) for x in v[:3]]
                ts = [T.fmt(x - offset, 0) for x in v[3:7]]
                return "\\fade(" + ",".join(a + ts) + ")"
        return None

    return _join([(ov, _map_inner(s, fn) if ov else s) for ov, s in _blocks(text)])


def _transform_text(text: str, tf: TrackFrame, mp: _Mapper, opts: ApplyOptions, vectorize: bool) -> str:
    s = tf.scale if opts.scale else 1.0
    drot = tf.rot if opts.rotation else 0.0
    d = opts.decimals

    def pt(x, y):
        if not opts.position:
            return x, y
        return mp.map(tf, x, y)

    def fn(cn, name, arg, in_t):
        if cn == "t":
            return None
        if cn in _SCALE_TAGS and opts.scale:
            v = T._nums(arg)
            return f"\\{name}{T.fmt(v[0] * s, d)}" if v else None
        if cn in _BORDER_TAGS and opts.border and opts.scale:
            v = T._nums(arg)
            return f"\\{name}{T.fmt(v[0] * s, d)}" if v else None
        if cn == "frz" and opts.rotation:
            v = T._nums(arg)
            return f"\\{name}{T.fmt(T.norm_angle(v[0] + drot), d)}" if v else None
        if cn == "pos" and not in_t:
            v = T._nums(arg)
            if len(v) >= 2:
                x, y = pt(v[0], v[1])
                return f"\\pos({T.fmt(x, d)},{T.fmt(y, d)})"
        if cn == "org" and opts.origin and not in_t:
            v = T._nums(arg)
            if len(v) >= 2:
                x, y = pt(v[0], v[1])
                return f"\\org({T.fmt(x, d)},{T.fmt(y, d)})"
        if cn in ("clip", "iclip") and opts.clip:
            return _clip_arg(name, arg, pt, vectorize and not in_t)
        return None

    out = []
    for ov, part in _blocks(text):
        out.append((ov, _map_inner(part, fn) if ov else part))
    return _join(out)


def _clip_arg(name, arg, pt, vectorize):
    a = arg.strip()
    inner = a[1:-1] if a.startswith("(") and a.endswith(")") else a
    nums = inner.split(",")
    if len(nums) == 4 and all(_is_num(x) for x in nums):
        x1, y1, x2, y2 = (float(x) for x in nums)
        corners = [pt(x1, y1), pt(x2, y1), pt(x2, y2), pt(x1, y2)]
        if vectorize:
            pts = [["m", *corners[0]]] + [["l", *c] for c in corners[1:]]
            return f"\\{name}({T.drawing_str(pts, 2)})"
        xs = [c[0] for c in corners]
        ys = [c[1] for c in corners]
        return f"\\{name}({round(min(xs))},{round(min(ys))},{round(max(xs))},{round(max(ys))})"
    m = re.match(r"\s*(\d+)\s*,(.*)$", inner, re.S)
    sc, draw = (int(m.group(1)), m.group(2)) if m else (1, inner)
    k = 2 ** (sc - 1) if sc > 1 else 1
    pts = T.parse_drawing(draw)
    if not pts:
        return None
    new = []
    for c, x, y in pts:
        nx, ny = pt(x / k, y / k)
        new.append([c, nx * k, ny * k])
    body = T.drawing_str(new, 2)
    return f"\\{name}({sc},{body})" if sc > 1 else f"\\{name}({body})"


def _insert_missing(text: str, base: _Base, tf: TrackFrame, mp: _Mapper, opts: ApplyOptions,
                    style, any_rot: bool, any_scale: bool) -> str:
    d = opts.decimals
    s = tf.scale if opts.scale else 1.0
    if opts.position and not base.has_pos:
        x, y = mp.map(tf, *base.pos)
        text = T.set_tag(text, "pos", f"({T.fmt(x, d)},{T.fmt(y, d)})")
    if opts.scale and any_scale:
        if not base.has_fscx:
            text = T.set_tag(text, "fscx", T.fmt(base.fscx * s, d))
        if not base.has_fscy:
            text = T.set_tag(text, "fscy", T.fmt(base.fscy * s, d))
        if opts.border:
            if not base.has_bord and base.bord > 0:
                text = T.set_tag(text, "bord", T.fmt(base.bord * s, d))
            if not base.has_shad and base.shad > 0:
                text = T.set_tag(text, "shad", T.fmt(base.shad * s, d))
    if opts.rotation and any_rot and not base.has_frz:
        text = T.set_tag(text, "frz", T.fmt(T.norm_angle(base.frz + tf.rot), d))
    return text


def line_frames(ev, fm) -> tuple[int, int]:
    from .framemap import END, START
    f0 = fm.frame_at_time(ev.start, START)
    f1 = fm.frame_at_time(ev.end, END)
    return f0, max(f0, f1)


def apply_track(ev, style, track: MotionTrack, fm, play_res, video_size, opts: ApplyOptions | None = None):
    """Satıra izi uygula. Döndürür: (yeni_satırlar, uyarılar)."""
    from .framemap import START
    opts = opts or ApplyOptions()
    warnings: list[str] = []
    if not track.frames:
        raise MotionError("İz boş")
    tr = track.filled()
    if opts.ref_frame is not None and opts.ref_frame != tr.ref_frame:
        tr = tr.rebased(opts.ref_frame)
    base = _base(ev, style, play_res)
    mp = _Mapper(play_res, video_size)
    f0, f1 = line_frames(ev, fm)
    outside = sum(1 for k in range(f0, f1 + 1) if not tr.covers(k))
    if outside:
        warnings.append(f"Satırın {outside} karesi izin dışında; uçtaki değer tutuldu")
    frames = list(range(f0, f1 + 1))
    tfs = [tr.at(k) for k in frames]
    any_rot = opts.rotation and any(abs(tf.rot) > 0.005 for tf in tfs)
    any_scale = opts.scale and any(abs(tf.scale - 1) > 1e-5 for tf in tfs)
    clip = T.get_clip(ev.text)
    vectorize = bool(opts.clip and clip and clip[0] == "rect" and any_rot)
    mode = opts.mode
    if mode == "compact":
        why = None
        if opts.clip and clip and (clip[0] == "vector" or vectorize):
            why = "vektör clip"
        elif "\\t(" in ev.text and (opts.scale or opts.rotation):
            why = "satırda zaten \\t var"
        elif re.search(r"\\(?:fscx|fscy|frz|fr|bord|shad|blur)[-\d.]", "".join(
                s for ov, s in _blocks(ev.text)[1:] if ov)):
            why = "sonraki bloklarda ölçek/dönüş etiketi"
        if why:
            warnings.append(f"Sıkıştırılmış kip kullanılamadı ({why}); kare kare uygulandı")
            mode = "frame"
    orig_dur = ev.end - ev.start
    # ---- örtme / görünürlük
    occ = opts.occlusion
    masks: dict[int, str] = {}
    hidden: set[int] = set()
    if occ is not None:
        fr_occ = getattr(occ, "frames", {}) or {}
        use_mask = True
        if clip:
            use_mask = False
            if any(fr_occ.get(k) is not None and fr_occ[k].ok and fr_occ[k].polys for k in frames):
                warnings.append("Satırda zaten \\clip var; örtme \\iclip'i eklenmedi (ASS'te satır başına tek clip)")
        from ..media.occlude import polys_to_drawing
        for k in frames:
            fo = fr_occ.get(k)
            if fo is None:
                continue
            if opts.hide_below > 0 and fo.visibility < opts.hide_below:
                hidden.add(k)
            elif use_mask and fo.ok and fo.polys:
                masks[k] = polys_to_drawing(fo.polys, mp.kx, mp.ky)
        if hidden:
            warnings.append(f"{len(hidden)} karede görünürlük düşük; o karelerde satır üretilmedi")

    def frame_text(tf, new_start, k=None):
        t = shift_times(ev.text, new_start - ev.start, orig_dur)
        t = _transform_text(t, tf, mp, opts, vectorize)
        t = _insert_missing(t, base, tf, mp, opts, style, any_rot, any_scale)
        if k in masks:
            t = T.set_tag(t, "iclip", f"({masks[k]})")
        return t

    out = []
    if mode == "frame":
        for k, tf in zip(frames, tfs):
            if k in hidden:
                continue
            st = round_ms(fm.time_at_frame(k, START))
            en = round_ms(fm.time_at_frame(k + 1, START))
            out.append(ev.fresh().replace(start=st, end=en, text=frame_text(tf, st, k)))
        return out, warnings

    # ---- sıkıştırılmış kip: gizli kareler atlanır, maskeli kareler tek kare kalır,
    # aradaki düz koşular hoşgörülü doğrusal parçalara bölünür
    vals = [_values(base, tf, mp, opts, clip if opts.clip else None) for tf in tfs]
    _unwrap(vals)
    ts = [fm.time_at_frame(k) for k in frames]
    segs = []
    i = 0
    n = len(frames)
    while i < n:
        k = frames[i]
        if k in hidden:
            i += 1
            continue
        if k in masks:
            segs.append((i, i))
            i += 1
            continue
        j = i
        while j + 1 < n and frames[j + 1] not in hidden and frames[j + 1] not in masks:
            j += 1
        segs += [(i + a, i + b) for a, b in _segments(vals[i:j + 1], ts[i:j + 1], opts)]
        i = j + 1
    for a, b in segs:
        st = round_ms(fm.time_at_frame(frames[a], START))
        en = round_ms(fm.time_at_frame(frames[b] + 1, START))
        text = frame_text(tfs[a], st, frames[a])
        if b > a:
            ta, tb = int(ts[a] - st), int(ts[b] - st)
            va, vb = vals[a], vals[b]
            d = opts.decimals
            if opts.position:
                text = T.remove(text, "pos")
                text = T.set_move(text, round(va["x"], d), round(va["y"], d), round(vb["x"], d),
                                  round(vb["y"], d), ta, tb)
            anim = ""
            if opts.scale and any_scale:
                anim += f"\\fscx{T.fmt(vb['fscx'], d)}\\fscy{T.fmt(vb['fscy'], d)}"
                if opts.border:
                    if base.has_bord or base.bord > 0:
                        anim += f"\\bord{T.fmt(vb['bord'], d)}"
                    if base.has_shad or base.shad > 0:
                        anim += f"\\shad{T.fmt(vb['shad'], d)}"
                    blur = T.get(text, "blur")
                    if blur is not None:
                        anim += f"\\blur{T.fmt(T.get_float(ev.text, 'blur') * vb['s'], d)}"
            if opts.rotation and any_rot:
                anim += f"\\frz{T.fmt(vb['frz'], d)}"
            if opts.clip and clip and clip[0] == "rect":
                x1, y1, x2, y2 = vb["clip"]
                anim += f"\\{'iclip' if clip[1] else 'clip'}({x1},{y1},{x2},{y2})"
            if anim:
                fb = T.first_block(text)
                text = text[: fb[1] - 1] + f"\\t({ta},{tb},{anim})" + text[fb[1] - 1:]
        out.append(ev.fresh().replace(start=st, end=en, text=text))
    return out, warnings


def _values(base: _Base, tf: TrackFrame, mp: _Mapper, opts: ApplyOptions, clip=None) -> dict:
    s = tf.scale if opts.scale else 1.0
    x, y = mp.map(tf, *base.pos) if opts.position else base.pos
    v = {"x": x, "y": y, "s": s, "fscx": base.fscx * s, "fscy": base.fscy * s,
         "bord": base.bord * s, "shad": base.shad * s,
         "frz": base.frz + (tf.rot if opts.rotation else 0.0), "clip": None}
    if clip and clip[0] == "rect":
        x1, y1, x2, y2 = clip[2]
        pts = [mp.map(tf, px, py) if opts.position else (px, py) for px, py in ((x1, y1), (x2, y2))]
        v["clip"] = (round(min(pts[0][0], pts[1][0])), round(min(pts[0][1], pts[1][1])),
                     round(max(pts[0][0], pts[1][0])), round(max(pts[0][1], pts[1][1])))
    return v


def _unwrap(vals):
    for i in range(1, len(vals)):
        d = vals[i]["frz"] - vals[i - 1]["frz"]
        while d > 180:
            vals[i]["frz"] -= 360
            d -= 360
        while d < -180:
            vals[i]["frz"] += 360
            d += 360


def _segments(vals, ts, opts: ApplyOptions):
    """Doğrusal ara değerin her karede hoşgörü içinde kaldığı en uzun parçalar."""
    n = len(vals)
    segs = []
    a = 0
    while a < n:
        b = a
        while b + 1 < n and _fits(vals, ts, a, b + 1, opts):
            b += 1
        segs.append((a, b))
        a = b + 1
    return segs


def _fits(vals, ts, a, b, opts):
    va, vb = vals[a], vals[b]
    span = ts[b] - ts[a]
    if span <= 0:
        return False
    for k in range(a + 1, b):
        u = (ts[k] - ts[a]) / span
        vk = vals[k]
        if abs(va["x"] + (vb["x"] - va["x"]) * u - vk["x"]) > opts.tol_pos:
            return False
        if abs(va["y"] + (vb["y"] - va["y"]) * u - vk["y"]) > opts.tol_pos:
            return False
        if abs(va["fscx"] + (vb["fscx"] - va["fscx"]) * u - vk["fscx"]) > opts.tol_scale:
            return False
        if abs(va["fscy"] + (vb["fscy"] - va["fscy"]) * u - vk["fscy"]) > opts.tol_scale:
            return False
        if abs(va["frz"] + (vb["frz"] - va["frz"]) * u - vk["frz"]) > opts.tol_rot:
            return False
        if vk["clip"] is not None:
            for j in range(4):
                if abs(va["clip"][j] + (vb["clip"][j] - va["clip"][j]) * u - vk["clip"][j]) > 1.0:
                    return False
    return True


def apply_to_lines(doc, rows, track, fm, video_size, opts: ApplyOptions | None = None):
    """Seçili satırlara uygula. Döndürür: (yeni olay listesi, yeni satır indeksleri, uyarılar)."""
    events = list(doc.events)
    play_res = doc.play_res()
    rows = sorted(set(rows))
    out = []
    new_rows = []
    warnings = []
    rs = set(rows)
    for i, ev in enumerate(events):
        if i not in rs or ev.comment:
            out.append(ev)
            continue
        style = doc.style_by_name(ev.style)
        try:
            new, w = apply_track(ev, style, track, fm, play_res, video_size, opts)
        except MotionError as e:
            warnings.append(f"Satır {i + 1}: {e}")
            out.append(ev)
            continue
        warnings += [f"Satır {i + 1}: {x}" for x in w]
        new_rows += list(range(len(out), len(out) + len(new)))
        out += new
    return out, new_rows, warnings
