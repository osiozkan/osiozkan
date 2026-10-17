"""Sahne değişimi algılama (ffmpeg `scdet` süzgeci).

Kodlayıcının anahtar kareleri her zaman sahne kesmesine denk gelmez (sabit GOP,
sc_threshold=0, yeniden kodlanmış yayınlar). Bu modül görüntünün kendisine bakar:
kare 160 px genişliğe küçültülür, ardışık kareler arasındaki fark puanı eşiği
geçince kesme sayılır. Sonuç kare numarası listesidir, önbelleğe yazılır.

Maliyet: tüm video çözülür (1080p H.264, 24 dk ≈ 1–2 dk; arka planda).
"""

from __future__ import annotations

import json
import os
import re
import subprocess

from . import cache
from .probe import _NO_WINDOW, tool

_LINE = re.compile(r"lavfi\.scd\.score:\s*([\d.]+),\s*lavfi\.scd\.time:\s*(-?[\d.]+)")
_PROG = re.compile(r"time=(\d+):(\d+):([\d.]+)")
VERSION = 1


def detect_scenes(path: str, index, threshold: float = 10.0, progress=None, cancel=None,
                  use_cache: bool = True) -> list[int]:
    cfile = os.path.join(cache.entry_dir(path), f"scenes_t{threshold:g}_v{VERSION}.json") if use_cache else None
    if cfile and os.path.exists(cfile):
        try:
            with open(cfile, encoding="utf-8") as f:
                return json.load(f)["frames"]
        except (OSError, ValueError, KeyError):
            pass
    exe = tool("ffmpeg")
    if not exe:
        raise RuntimeError("ffmpeg bulunamadı")
    args = [exe, "-hide_banner", "-nostdin", "-v", "info", "-stats_period", "0.5", "-hwaccel", "auto",
            "-i", path, "-map", "0:v:0", "-an", "-sn",
            "-vf", f"scale=160:-2,scdet=threshold={threshold:g}", "-f", "null", "-"]
    proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, creationflags=_NO_WINDOW)
    dur = (index.pts_s[-1] - index.pts_s[0]) if index is not None and len(index) > 1 else 0
    times: list[float] = []
    buf = b""
    while True:
        chunk = proc.stderr.read(4096)
        if not chunk:
            break
        buf += chunk
        *lines, buf = re.split(rb"[\r\n]", buf)
        for ln in lines:
            s = ln.decode("utf-8", "replace")
            m = _LINE.search(s)
            if m:
                times.append(float(m.group(2)))
                continue
            p = _PROG.search(s)
            if p and progress and dur:
                t = int(p.group(1)) * 3600 + int(p.group(2)) * 60 + float(p.group(3))
                progress(min(0.99, t / dur))
        if cancel is not None and cancel():
            proc.kill()
            proc.wait()
            raise RuntimeError("İptal edildi")
    proc.wait()
    proc.stderr.close()
    if proc.returncode not in (0, None) and not times:
        raise RuntimeError("ffmpeg sahne algılama başarısız")
    frames = sorted({index.frame_at_seconds(t) for t in times}) if index is not None else []
    if cfile:
        try:
            with open(cfile, "w", encoding="utf-8") as f:
                json.dump({"frames": frames, "threshold": threshold}, f)
        except OSError:
            pass
    if progress:
        progress(1.0)
    return frames
