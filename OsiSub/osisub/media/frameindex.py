"""Kare zaman damgası dizini.

Paketler ÇÖZÜLMEDEN okunur (PyAV demux; yoksa ffprobe), pts'e göre sıralanır
(B-kareler yüzünden paket sırası gösterim sırası değildir) ve anahtar kare
bayrakları toplanır. 24 dakikalık bölüm birkaç saniyede biter, önbelleğe yazılır.

Zaman tanımları — iki ayrı dünya:
  * HAM saniye (pts * time_base): mpv'nin time-pos'u ve arama hedefi bu.
  * Aegisub ms'si: Aegisub (FFMS2) zamanları tamsayı ms'ye kırpar ve İLK KAREYİ
    0'A ÇEKER (libaegisub vfr.cpp normalize). Örn. MP4'e yeniden paketlenmiş
    dosyada ilk kare 21 ms'de olabilir; Aegisub onu 0 sayar. Altyazı zamanları
    ve Lua'daki frame_from_ms bu ms'yi kullanır; mpv'de önizlerken aradaki fark
    sub-delay ile kapatılır.
"""

from __future__ import annotations

import bisect
import json
import os
from dataclasses import dataclass, field
from fractions import Fraction

from ..core.framemap import FrameMap
from . import cache
from .probe import probe, run, tool

INDEX_VERSION = 1


@dataclass
class FrameIndex:
    pts: list[int]                      # sıralı ham pts (stream time_base)
    time_base: Fraction
    keyframes: list[int] = field(default_factory=list)   # kare numaraları
    width: int = 0
    height: int = 0
    fps: Fraction = Fraction(24000, 1001)
    sar: Fraction = Fraction(1)
    source: str = ""                    # pyav | ffprobe

    def __post_init__(self):
        tb = self.time_base
        self.pts_s = [float(p * tb) for p in self.pts]
        # Aegisub: int(PTS * num / den) ms, sonra ilk kare 0'a çekilir
        raw_ms = [(p * tb.numerator * 1000) // tb.denominator for p in self.pts]
        first = raw_ms[0] if raw_ms else 0
        self.ms = [m - first for m in raw_ms]
        self.offset_s = self.pts_s[0] if self.pts_s else 0.0

    # ------------------------------------------------------------ sorgular
    def __len__(self):
        return len(self.pts)

    @property
    def is_vfr(self) -> bool:
        """MKV zamanları ms'ye yuvarlar: 23.976'da 41/42 salınımı VFR değildir.
        Eşik 2 ms: gerçek VFR'yi ayırır, yuvarlama gürültüsünü ayırmaz."""
        if len(self.ms) < 3:
            return False
        d = [b - a for a, b in zip(self.ms, self.ms[1:])]
        return max(d) - min(d) > 2

    @property
    def avg_fps(self) -> float:
        if len(self.pts_s) < 2:
            return float(self.fps)
        span = self.pts_s[-1] - self.pts_s[0]
        return (len(self.pts_s) - 1) / span if span > 0 else float(self.fps)

    def frame_map(self) -> FrameMap:
        fm = FrameMap(self.ms if len(self.ms) >= 2 else [0, int(1000 / float(self.fps or 24))])
        fm.keyframes = list(self.keyframes)
        fm.fps = self.avg_fps
        return fm

    def frame_at_seconds(self, t: float) -> int:
        """mpv konumu (ham sn) -> kare. Yuvarlama payı 0.5 ms: int(sn*1e6)
        kırpması bir önceki kareyi verirdi (2503.000 ms -> 2502999 µs)."""
        if not self.pts_s:
            return 0
        i = bisect.bisect_right(self.pts_s, t + 0.0005) - 1
        return max(0, min(i, len(self.pts_s) - 1))

    def seconds_of(self, frame: int) -> float:
        """Kareye kesin arama hedefi (karenin başı; orta noktaya arama bir
        SONRAKİ kareye kaçırıyor — bplayer'da ölçüldü)."""
        frame = max(0, min(frame, len(self.pts_s) - 1))
        return self.pts_s[frame]

    def aegisub_ms_to_seconds(self, ms: int) -> float:
        return ms / 1000.0 + self.offset_s

    def seconds_to_aegisub_ms(self, t: float) -> int:
        return int(round((t - self.offset_s) * 1000))

    # ------------------------------------------------------------ önbellek
    def to_json(self) -> dict:
        return {"v": INDEX_VERSION, "pts": self.pts, "tb": [self.time_base.numerator, self.time_base.denominator],
                "kf": self.keyframes, "w": self.width, "h": self.height,
                "fps": [self.fps.numerator, self.fps.denominator],
                "sar": [self.sar.numerator, self.sar.denominator], "src": self.source}

    @classmethod
    def from_json(cls, d: dict) -> "FrameIndex":
        return cls(pts=d["pts"], time_base=Fraction(*d["tb"]), keyframes=d["kf"], width=d["w"],
                   height=d["h"], fps=Fraction(*d["fps"]), sar=Fraction(*d.get("sar", [1, 1])),
                   source=d.get("src", ""))


def _index_pyav(path: str, progress=None):
    import av
    with av.open(path) as c:
        vs = next((s for s in c.streams.video
                   if not (s.disposition & getattr(av.stream.Disposition, "attached_pic", 1 << 10))), None)
        if vs is None:
            raise RuntimeError("Video akışı yok")
        tb = Fraction(vs.time_base.numerator, vs.time_base.denominator)
        dur = float(c.duration / 1_000_000) if c.duration else 0.0
        pts, keys = [], set()
        n = 0
        for pkt in c.demux(vs):
            p = pkt.pts if pkt.pts is not None else pkt.dts
            if p is None or pkt.size == 0:
                continue
            pts.append(int(p))
            if pkt.is_keyframe:
                keys.add(int(p))
            n += 1
            if progress and n % 2000 == 0 and dur:
                progress(min(0.99, float(p * tb) / dur))
        cc = vs.codec_context
        fps = Fraction(vs.average_rate.numerator, vs.average_rate.denominator) if vs.average_rate else Fraction(24000, 1001)
        if vs.guessed_rate:
            fps = Fraction(vs.guessed_rate.numerator, vs.guessed_rate.denominator)
        sar = Fraction(1)
        if vs.sample_aspect_ratio:
            sar = Fraction(vs.sample_aspect_ratio.numerator, vs.sample_aspect_ratio.denominator)
        return pts, keys, tb, cc.width, cc.height, fps, sar


def _index_ffprobe(path: str, progress=None):
    info = probe(path)
    v = info.video
    if v is None:
        raise RuntimeError("Video akışı yok")
    r = run([tool("ffprobe"), "-v", "error", "-select_streams", f"{v.index}", "-show_entries",
             "packet=pts,dts,flags", "-of", "csv=p=0", path])
    pts, keys = [], set()
    for ln in r.stdout.decode("ascii", "replace").splitlines():
        parts = ln.strip().split(",")
        if len(parts) < 3:
            continue
        p = parts[0] if parts[0] not in ("", "N/A") else parts[1]
        if p in ("", "N/A"):
            continue
        p = int(p)
        pts.append(p)
        if "K" in parts[2]:
            keys.add(p)
    return pts, keys, v.time_base, v.width, v.height, v.fps, v.sar


def build_index(path: str, progress=None, use_cache: bool = True) -> FrameIndex:
    cpath = os.path.join(cache.entry_dir(path), "frames.json") if use_cache else None
    if cpath and os.path.exists(cpath):
        try:
            with open(cpath, encoding="utf-8") as f:
                d = json.load(f)
            if d.get("v") == INDEX_VERSION:
                return FrameIndex.from_json(d)
        except (OSError, ValueError, KeyError):
            pass
    try:
        pts, keys, tb, w, h, fps, sar = _index_pyav(path, progress)
        src = "pyav"
    except ImportError:
        pts, keys, tb, w, h, fps, sar = _index_ffprobe(path, progress)
        src = "ffprobe"
    pts = sorted(set(pts))
    kf_frames = [i for i, p in enumerate(pts) if p in keys]
    if not kf_frames and pts:
        kf_frames = [0]
    idx = FrameIndex(pts=pts, time_base=tb, keyframes=kf_frames, width=w, height=h, fps=fps,
                     sar=sar, source=src)
    if cpath:
        try:
            with open(cpath, "w", encoding="utf-8") as f:
                json.dump(idx.to_json(), f)
        except OSError:
            pass
    if progress:
        progress(1.0)
    return idx
