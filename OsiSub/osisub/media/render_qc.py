"""libass ile yakarak görünürlük denetimi (assmotion `qc --method render`).

Her satır, ekran görüntüsü yerine KENDİ çizimiyle ölçülür: satırlar ffmpeg'in `ass`
süzgeciyle (libass — mpv ve çoğu oynatıcıyla aynı çizici) önce siyah, sonra beyaz zemine
yakılır; iki görüntünün farkından piksel başına ÖRTÜCÜLÜK (alfa) çıkar:
    alfa = 1 − (beyaz − siyah) / 255
Bir kez satırın kendisiyle (\\clip/\\iclip dahil), bir kez clip'ler kaldırılmış haliyle
yakılır; görünürlük = alfa toplamı (clip'li) / alfa toplamı (clip'siz). Böylece \\iclip
örtmesinin, ekran dışına taşmanın ve \\clip'in satırdan gerçekte ne bıraktığı ölçülür.

Satırlar zamanda çakışmayan katmanlara ayrılır (her katman ayrı yakılır); her satır kendi
süresinin ORTASINDAKİ anda ölçülür. Hız için görüntü en çok `max_side` piksele küçültülerek
yakılır (oran ölçümü bundan pek etkilenmez). Çok seyrek zamanlar ayrı ffmpeg çağrılarına
bölünür (aralarındaki boşluk yakılmaz).
"""

from __future__ import annotations

import os
import re
import subprocess
import tempfile
from dataclasses import dataclass

import numpy as np

_CLIP_RE = re.compile(r"\\i?clip\((?:[^()]|\([^()]*\))*\)")


@dataclass
class LineVisibility:
    row: int
    t_ms: int
    full: float          # clip'siz alfa toplamı (piksel)
    visible: float       # clip'li alfa toplamı

    @property
    def ratio(self) -> float:
        return self.visible / self.full if self.full > 0 else 0.0


def strip_clips(text: str) -> str:
    return _CLIP_RE.sub("", text).replace("{}", "")


def _layers(items):
    """[(row, start, end)] -> zamanda çakışmayan katmanlar (açgözlü)."""
    layers: list[list] = []
    ends: list[int] = []
    for it in sorted(items, key=lambda x: (x[1], x[2])):
        for i, e in enumerate(ends):
            if it[1] >= e:
                layers[i].append(it)
                ends[i] = it[2]
                break
        else:
            layers.append([it])
            ends.append(it[2])
    return layers


def _clusters(times, gap_ms=1500):
    out = []
    for t in sorted(times):
        if out and t - out[-1][-1] <= gap_ms:
            out[-1].append(t)
        else:
            out.append([t])
    return out


def _write_ass(doc, events, path):
    d = doc.clone()
    d.ensure_section("events").items[:] = [e.fresh() for e in events]
    with open(path, "wb") as fh:
        fh.write(d.to_bytes())


def _ff_escape(p: str) -> str:
    # ffmpeg süzgeç argümanı: \ : ' , [ ] kaçışları (Windows yolları dahil)
    p = p.replace("\\", "/")
    for ch in (":", "'", ",", "[", "]"):
        p = p.replace(ch, "\\" + ch)
    return p


def _render(ffmpeg, ass_path, w, h, t0, t1, bg, step_ms, fontsdir=None):
    """[t0, t1] aralığını step_ms adımla yak; (t_ms, gri kare) üretir."""
    dur = max(step_ms, t1 - t0 + step_ms) / 1000.0
    rate = 1000.0 / step_ms
    vf = f"setpts=PTS+{t0 / 1000.0:.3f}/TB,ass='{_ff_escape(ass_path)}'"
    if fontsdir:
        vf = vf[:-1] + f":fontsdir='{_ff_escape(fontsdir)}'"
    cmd = [ffmpeg, "-v", "error", "-f", "lavfi", "-i", f"color=c={bg}:s={w}x{h}:r={rate:g}:d={dur:.3f}",
           "-vf", vf, "-f", "rawvideo", "-pix_fmt", "gray", "-"]
    from .probe import _NO_WINDOW
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=_NO_WINDOW)
    size = w * h
    n = 0
    try:
        while True:
            buf = proc.stdout.read(size)
            if len(buf) < size:
                break
            yield t0 + n * step_ms, np.frombuffer(buf, np.uint8)
            n += 1
    finally:
        proc.stdout.close()
        err = proc.stderr.read().decode("utf-8", "replace")
        proc.wait()
        if proc.returncode not in (0, None) and n == 0:
            raise RuntimeError(f"ffmpeg ass süzgeci çalışmadı: {err.strip()[:300]}")


def measure(doc, rows, video_size, max_side: int = 640, step_ms: int = 10, fontsdir=None,
            progress=None, cancel=None) -> list[LineVisibility]:
    """Seçili satırların libass ile ölçülmüş görünürlüğü."""
    from .probe import tool
    ffmpeg = tool("ffmpeg")
    if not ffmpeg:
        raise RuntimeError("ffmpeg bulunamadı")
    vw, vh = video_size
    f = min(1.0, max_side / max(vw, vh))
    w, h = max(16, int(round(vw * f / 2)) * 2), max(16, int(round(vh * f / 2)) * 2)
    evs = doc.events
    items = [(r, evs[r].start, evs[r].end) for r in sorted(set(rows))
             if 0 <= r < len(evs) and not evs[r].comment and evs[r].end > evs[r].start]
    layers = _layers(items)
    out: dict[int, LineVisibility] = {}
    tmp = tempfile.mkdtemp(prefix="osisub_qc_")
    total = max(1, len(items))
    done = 0
    try:
        for li, layer in enumerate(layers):
            mids = {}
            for r, s, e in layer:
                t = (s + e) // 2
                mids[r] = t - (t % step_ms)              # ızgaraya otur (satırın içinde kalır)
            a_path = os.path.join(tmp, f"a{li}.ass")
            b_path = os.path.join(tmp, f"b{li}.ass")
            _write_ass(doc, [evs[r] for r, _s, _e in layer], a_path)
            _write_ass(doc, [evs[r].replace(text=strip_clips(evs[r].text)) for r, _s, _e in layer], b_path)
            by_t: dict[int, list[int]] = {}
            for r, t in mids.items():
                by_t.setdefault(t, []).append(r)
            for cl in _clusters(by_t):
                t0, t1 = cl[0], cl[-1]
                need = set(cl)
                acc = {}
                for key, path, bg in (("ab", a_path, "black"), ("aw", a_path, "white"),
                                      ("bb", b_path, "black"), ("bw", b_path, "white")):
                    for t, fr in _render(ffmpeg, path, w, h, t0, t1, bg, step_ms, fontsdir):
                        if t in need:
                            acc.setdefault(t, {})[key] = fr
                        if cancel is not None and cancel():
                            from .tracker import Cancelled
                            raise Cancelled()
                for t in cl:
                    d = acc.get(t, {})
                    if len(d) < 4:
                        continue
                    alpha_a = 1.0 - (d["aw"].astype(np.int16) - d["ab"].astype(np.int16)).clip(0, 255) / 255.0
                    alpha_b = 1.0 - (d["bw"].astype(np.int16) - d["bb"].astype(np.int16)).clip(0, 255) / 255.0
                    va, vb = float(alpha_a.sum()) / f / f, float(alpha_b.sum()) / f / f
                    for r in by_t[t]:
                        out[r] = LineVisibility(r, t, vb, va)
                        done += 1
                        if progress:
                            progress(done / total, r)
    finally:
        for name in os.listdir(tmp):
            try:
                os.remove(os.path.join(tmp, name))
            except OSError:
                pass
        try:
            os.rmdir(tmp)
        except OSError:
            pass
    return [out[r] for r, _s, _e in sorted(items) if r in out]
