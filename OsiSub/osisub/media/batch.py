"""Toplu hareket takibi: seçili her satır kendi aralığında, kendi alanıyla izlenir
(assmotion `scan` → `batch` akışının uygulama içi hali).

Satır başına iş:
  aralık   satırın kare aralığı; sahne kesmesi varsa referansın bulunduğu sahneyle sınırlanır
  referans satırın ilk karesi (Aegisub-Motion geleneği: satır ilk karede yerleştirilir) ya da
           istenirse videodaki geçerli kare (satırın içindeyse)
  alan     satırda dikdörtgen \\clip varsa o; yoksa satırın metin kutusu (yazı tipi ölçümü,
           \\fscx/\\fscy, hizalama, çok satır) ya da çizim (\\p) kutusu, `pad` oranında büyütülmüş
"""

from __future__ import annotations

from dataclasses import dataclass, field

from ..core import tags as T
from ..core.motion import line_frames


@dataclass
class Job:
    row: int
    start: int
    end: int
    ref: int
    roi: tuple | None
    notes: list = field(default_factory=list)
    track: object = None
    occlusion: object = None
    error: str | None = None


def line_box(ev, style, play_res) -> tuple[float, float, float, float] | None:
    """Satırın betik koordinatında kabaca kutusu (x, y, w, h) — \\frz yok sayılır."""
    from ..automation.textmetrics import text_extents
    text = ev.text
    fscx = T.get_float(text, "fscx", style.scale_x if style else 100.0)
    fscy = T.get_float(text, "fscy", style.scale_y if style else 100.0)
    p = T.get(text, "p")
    if p and p.strip() not in ("0", ""):
        # çizim: \p kipindeki metnin nokta kutusu
        body = re_drawing_body(text)
        pts = T.parse_drawing(body)
        if not pts:
            return None
        k = 2 ** (int(float(p)) - 1)
        xs = [x / k * fscx / 100 for _c, x, _y in pts]
        ys = [y / k * fscy / 100 for _c, _x, y in pts]
        w, h = max(xs) - min(xs), max(ys) - min(ys)
    else:
        st = {
            "fontname": (T.get(text, "fn") or (style.fontname if style else "Arial")).strip(),
            "fontsize": T.get_float(text, "fs", style.fontsize if style else 48.0),
            "bold": bool(style.bold) if style else False, "italic": bool(style.italic) if style else False,
            "spacing": T.get_float(text, "fsp", style.spacing if style else 0.0),
            "scale_x": fscx, "scale_y": fscy,
        }
        from ..core.textutil import visible_lines
        lines = visible_lines(text) or [""]
        w = h = 0.0
        for ln in lines:
            lw, lh, _d, _e = text_extents(st, ln or " ")
            w = max(w, lw)
            h += lh
        if w <= 0 or h <= 0:
            return None
    x, y = T.position(ev, style, play_res, None)
    an = T.alignment(text, style.align if style else 2)
    col, row = (an - 1) % 3, (an - 1) // 3
    left = x - (0 if col == 0 else (w / 2 if col == 1 else w))
    top = y - (h if row == 0 else (h / 2 if row == 1 else 0))
    return left, top, w, h


def re_drawing_body(text: str) -> str:
    """\\p1 sonrası (ilk override bloğundan sonra) çizim metni."""
    import re
    return re.sub(r"\{[^}]*\}", " ", text).strip()


def line_roi(ev, style, play_res, video_size, pad: float = 0.15):
    """Satırın izleme alanı (video pikseli)."""
    prx, pry = play_res
    vw, vh = video_size
    kx, ky = vw / prx, vh / pry
    c = T.get_clip(ev.text)
    if c and c[0] == "rect":
        x1, y1, x2, y2 = c[2]
        return (x1 * kx, y1 * ky, (x2 - x1) * kx, (y2 - y1) * ky), "clip"
    box = line_box(ev, style, play_res)
    if box is None:
        return None, "kutu ölçülemedi"
    x, y, w, h = box
    x, y, w, h = x - w * pad, y - h * pad, w * (1 + 2 * pad), h * (1 + 2 * pad)
    x0, y0 = max(0.0, x * kx), max(0.0, y * ky)
    x1, y1 = min(vw, (x + w) * kx), min(vh, (y + h) * ky)
    if x1 - x0 < 8 or y1 - y0 < 8:
        return None, "alan görüntü dışında"
    return (x0, y0, x1 - x0, y1 - y0), "metin kutusu"


def plan(doc, rows, fm, n_frames: int, video_size, scenes=(), ref_mode: str = "first",
         current: int | None = None, pad: float = 0.15) -> list[Job]:
    jobs = []
    play_res = doc.play_res()
    cuts = sorted(set(int(c) for c in scenes or ()))
    for r in sorted(set(rows)):
        ev = doc.events[r]
        if ev.comment:
            continue
        f0, f1 = line_frames(ev, fm)
        f1 = min(f1, n_frames - 1)
        f0 = min(f0, f1)
        ref = f0
        if ref_mode == "current" and current is not None and f0 <= current <= f1:
            ref = current
        notes = []
        # referansın sahnesiyle sınırla (kesme karesi = yeni sahnenin ilk karesi)
        lo, hi = f0, f1
        for c in cuts:
            if lo < c <= ref:
                lo = c
            if ref < c <= hi:
                hi = c - 1
                break
        if (lo, hi) != (f0, f1):
            notes.append(f"sahne kesmesi: iz {lo}–{hi} ile sınırlandı (satır {f0}–{f1})")
        roi, how = line_roi(ev, doc.style_by_name(ev.style), play_res, video_size, pad)
        job = Job(r, lo, hi, ref, roi, notes)
        if roi is None:
            job.error = how
        else:
            job.notes.append(f"alan: {how}")
        jobs.append(job)
    return jobs


def run(jobs, get_rgb, method="points", model="similarity", occlusion_opts=None, width=0, height=0,
        fps=None, progress=None, cancel=None):
    """İşleri sırayla izle (iş başına hata yakalanır; iptal hepsini durdurur)."""
    from . import tracker
    total = max(1, sum(j.end - j.start + 1 for j in jobs if j.error is None))
    done = [0]
    for j in jobs:
        if j.error is not None:
            continue
        base = done[0]

        def prog(p, k, base=base, n=j.end - j.start + 1):
            if progress:
                progress(min(1.0, (base + p * n) / total), j.row, k)
        try:
            j.track = tracker.track(get_rgb, j.start, j.end, j.ref, j.roi, method=method, model=model,
                                    width=width, height=height, fps=fps, progress=prog, cancel=cancel)
            if occlusion_opts is not None:
                from . import occlude
                j.occlusion = occlude.detect(get_rgb, j.track, j.roi, occlusion_opts, cancel=cancel)
        except tracker.Cancelled:
            raise
        except Exception as e:  # noqa: BLE001
            j.error = str(e)
        done[0] += j.end - j.start + 1
    return jobs


def apply_jobs(doc, jobs, fm, video_size, opts):
    """Başarılı işleri belgeye uygula. Döndürür: (olaylar, yeni satırlar, uyarılar)."""
    from dataclasses import replace

    from ..core import motion as M
    events = list(doc.events)
    play_res = doc.play_res()
    warns = []
    new_rows: list[int] = []
    for j in sorted(jobs, key=lambda x: x.row, reverse=True):
        if j.error is not None or j.track is None:
            warns.append(f"Satır {j.row + 1}: {j.error or 'iz yok'}")
            continue
        ev = events[j.row]
        o = replace(opts, ref_frame=None, occlusion=j.occlusion)
        try:
            new, w = M.apply_track(ev, doc.style_by_name(ev.style), j.track, fm, play_res, video_size, o)
        except M.MotionError as e:
            warns.append(f"Satır {j.row + 1}: {e}")
            continue
        bad = len(j.track.bad_frames())
        if bad:
            w = list(w) + [f"{bad} kare güvensiz"]
        warns += [f"Satır {j.row + 1}: {x}" for x in list(j.notes) + list(w) if not x.startswith("alan:")]
        events[j.row:j.row + 1] = new
        new_rows = [x + len(new) - 1 for x in new_rows] + list(range(j.row, j.row + len(new)))
    return events, sorted(new_rows), warns
