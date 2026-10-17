"""Hareketi ve sesi BİLİNEN sentetik test videoları (ffmpeg ile üretilir, oturum başına bir kez).

  cfr.mkv  640x360 23.976 fps, 10 sn, B-kareli x264, anahtar kare her 48 karede,
           ses: 440 Hz ton yalnız 1.0–2.0 sn ve 5.0–5.5 sn arasında
  cfr.mp4  aynısı MP4'e yeniden paketlenmiş (video 21 ms geç başlar: Aegisub normalize sınaması)
  vfr.mkv  ilk 3 sn 30 fps, sonra 60 fps (gerçek VFR)
  cuts.mkv 4 sahne, kesmeler 48/96/144. karede, tek anahtar kare (sahne algılama sınaması)
  bursts.wav konuşma bandında gürültü patlamaları 1.0–2.5 ve 4.0–6.0 sn (VAD yer gerçeği)
  speech.wav espeak-ng ile İngilizce iki cümle (varsa; Whisper sınaması)
  motion.mkv 640x360 23.976 fps 96 kare, dokulu arka plan üzerinde konumu/ölçeği/açısı
           BİLİNEN bir tabela (motion_truth); x264 crf 14 (gerçekçi sıkıştırma)
  occlude.mkv aynı tabela + önünden soldan sağa geçen 40 px genişliğinde mavi direk
           (occluder_x: kare k'de x aralığı; yaklaşık 33–78. karelerde tabelayı örter)
  color.mkv aynı yol/ölçek + aynı direk, ama tabela düz KIRMIZI (beyaz yazılı) ve DÖNMÜYOR
           (renk anahtarlı izleyici dönüş izlemez) — motion_truth(k, rotate=False)
  hair.mkv  DURAN beyaz tabela (siyah yazı/çerçeve) önünden soldan sağa geçen dalgalı kahverengi
           "saç teli" (siyah çizgi-sanat kenarlı); hair_mask(k) yer gerçeği — renk modeliyle maske
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile

_DIR = None


def have_ffmpeg() -> bool:
    return bool(shutil.which("ffmpeg") and shutil.which("ffprobe"))


def media_dir() -> str:
    global _DIR
    if _DIR and os.path.exists(os.path.join(_DIR, "bursts.wav")):
        return _DIR
    d = os.environ.get("OSISUB_TEST_MEDIA") or os.path.join(tempfile.gettempdir(), "osisub_test_media_v1")
    os.makedirs(d, exist_ok=True)
    cfr, mp4, vfr = (os.path.join(d, n) for n in ("cfr.mkv", "cfr.mp4", "vfr.mkv"))
    if not os.path.exists(cfr):
        subprocess.run([
            "ffmpeg", "-v", "error", "-y",
            "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=24000/1001:duration=10",
            "-f", "lavfi", "-i", "aevalsrc=0.5*sin(2*PI*440*t)*(between(t\\,1\\,2)+between(t\\,5\\,5.5)):s=48000:d=10",
            "-c:v", "libx264", "-preset", "veryfast", "-g", "48", "-keyint_min", "48", "-sc_threshold", "0",
            "-bf", "2", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", cfr], check=True)
    if not os.path.exists(mp4):
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", cfr, "-c", "copy", mp4], check=True)
    if not os.path.exists(vfr):
        subprocess.run([
            "ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=60:duration=6",
            "-vf", "select='if(lt(t,3),not(mod(n,2)),1)'", "-fps_mode", "vfr", "-c:v", "libx264",
            "-preset", "veryfast", "-bf", "0", "-g", "30", vfr], check=True)
    cuts = os.path.join(d, "cuts.mkv")
    if not os.path.exists(cuts):
        # 4 farklı kaynak, 2'şer sn: kesmeler 48/96/144. karelerde; anahtar kare yalnız 0'da
        subprocess.run([
            "ffmpeg", "-v", "error", "-y",
            "-f", "lavfi", "-i", "testsrc2=s=320x240:r=24000/1001:d=2",
            "-f", "lavfi", "-i", "smptebars=s=320x240:r=24000/1001:d=2",
            "-f", "lavfi", "-i", "rgbtestsrc=s=320x240:r=24000/1001:d=2",
            "-f", "lavfi", "-i", "color=c=red:s=320x240:r=24000/1001:d=2",
            "-filter_complex", "[0:v][1:v][2:v][3:v]concat=n=4:v=1:a=0,format=yuv420p",
            "-c:v", "libx264", "-preset", "veryfast", "-g", "999", "-sc_threshold", "0", cuts], check=True)
    bursts = os.path.join(d, "bursts.wav")
    if not os.path.exists(bursts):
        # konuşma bandında (1 kHz çevresi) gürültü patlamaları: 1.0–2.5 sn ve 4.0–6.0 sn
        subprocess.run([
            "ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "anoisesrc=d=8:c=pink:r=22050:a=0.5:seed=7",
            "-af", "bandpass=f=1000:width_type=o:w=2,volume='if(between(t,1,2.5)+between(t,4,6),1,0.003)':eval=frame",
            bursts], check=True)
    speech = os.path.join(d, "speech.wav")
    if not os.path.exists(speech) and shutil.which("espeak-ng"):
        parts = []
        for k, txt in enumerate(("Hello there, how are you today", "I am going to the market tomorrow morning")):
            pth = os.path.join(d, f"s{k}.wav")
            subprocess.run(["espeak-ng", "-v", "en", "-s", "140", "-w", pth, txt], check=True)
            parts.append(pth)
        # 1 sn sessizlik + cümle 1 + 2 sn sessizlik + cümle 2 + 1 sn sessizlik
        subprocess.run([
            "ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-t", "1", "-i", "anullsrc=r=22050:cl=mono",
            "-i", parts[0], "-f", "lavfi", "-t", "2", "-i", "anullsrc=r=22050:cl=mono", "-i", parts[1],
            "-f", "lavfi", "-t", "1", "-i", "anullsrc=r=22050:cl=mono",
            "-filter_complex", "[0][1][2][3][4]concat=n=5:v=0:a=1", speech], check=True)
    motion = os.path.join(d, "motion.mkv")
    if not os.path.exists(motion):
        _make_motion(motion)
    occ = os.path.join(d, "occlude.mkv")
    if not os.path.exists(occ):
        _make_motion(occ, occluder=True)
    hair = os.path.join(d, "hair.mkv")
    if not os.path.exists(hair):
        _make_hair(hair)
    col = os.path.join(d, "color.mkv")
    if not os.path.exists(col):
        _make_motion(col, occluder=True, red=True)
    _DIR = d
    return d


MOTION_N = 96
MOTION_PATCH = (160, 100)           # tabela boyutu (w, h)


def motion_truth(k: int, rotate: bool = True):
    """k. karede tabela: merkez C (karmaşık, piksel) ve c = ölçek·e^{iφ} (görüntü açısı)."""
    import cmath
    import math
    C = complex(170 + 3.2 * k, 180 + 40 * math.sin(k / 14.0))
    s = 1.0 + 0.004 * k
    phi = math.radians(0.3 * k) if rotate else 0.0
    return C, cmath.rect(s, phi)


OCC_W = 40


def occluder_x(k: int) -> tuple[int, int]:
    """Direğin k. karedeki [x0, x1) aralığı (tam yükseklik)."""
    x0 = -100 + 8 * k
    return x0, x0 + OCC_W


def _make_motion(path: str, occluder: bool = False, red: bool = False):
    import numpy as np
    import cv2
    rng = np.random.default_rng(42)
    W, H = 640, 360
    bg = cv2.GaussianBlur(rng.integers(40, 120, (H // 8, W // 8, 3), dtype=np.uint8), (3, 3), 0)
    bg = cv2.resize(bg, (W, H), interpolation=cv2.INTER_CUBIC)
    pw, ph = MOTION_PATCH
    patch = np.full((ph, pw, 3), 235, np.uint8)
    for _ in range(18):
        x, y = int(rng.integers(0, pw - 20)), int(rng.integers(0, ph - 14))
        col = tuple(int(v) for v in rng.integers(0, 200, 3))
        cv2.rectangle(patch, (x, y), (x + int(rng.integers(6, 30)), y + int(rng.integers(5, 22))), col, -1)
    cv2.putText(patch, "OSI", (30, 70), cv2.FONT_HERSHEY_SIMPLEX, 1.6, (200, 20, 20), 4, cv2.LINE_AA)
    cv2.rectangle(patch, (0, 0), (pw - 1, ph - 1), (20, 20, 20), 3)
    if red:
        patch[:] = (210, 30, 30)
        cv2.putText(patch, "OSI", (30, 70), cv2.FONT_HERSHEY_SIMPLEX, 1.6, (250, 250, 250), 4, cv2.LINE_AA)
    qc = complex(pw / 2, ph / 2)
    rng_bar = np.random.default_rng(7)
    proc = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
                             "-s", f"{W}x{H}", "-r", "24000/1001", "-i", "-", "-c:v", "libx264",
                             "-preset", "veryfast", "-crf", "14", "-g", "24", "-pix_fmt", "yuv420p", path],
                            stdin=subprocess.PIPE)
    for k in range(MOTION_N):
        C, c = motion_truth(k, rotate=not red)
        t = C - c * qc
        M = np.array([[c.real, -c.imag, t.real], [c.imag, c.real, t.imag]], np.float64)
        warped = cv2.warpAffine(patch, M, (W, H), flags=cv2.INTER_LINEAR)
        mask = cv2.warpAffine(np.full((ph, pw), 255, np.uint8), M, (W, H), flags=cv2.INTER_LINEAR)
        a = (mask.astype(np.float32) / 255.0)[..., None]
        frame = (bg * (1 - a) + warped * a).astype(np.uint8)
        if occluder:
            x0, x1 = occluder_x(k)
            xa, xb = max(0, x0), min(W, x1)
            if xb > xa:
                bar = np.empty((H, xb - xa, 3), np.uint8)
                bar[...] = (40, 70, 160)
                bar = np.clip(bar.astype(int) + rng_bar.integers(-12, 12, bar.shape), 0, 255).astype(np.uint8)
                frame[:, xa:xb] = bar
        proc.stdin.write(frame.tobytes())
    proc.stdin.close()
    if proc.wait() != 0:
        raise RuntimeError("motion.mkv üretilemedi")


def path(name: str) -> str:
    return os.path.join(media_dir(), name)


HAIR_N = 72
HAIR_SIGN = (220, 125, 200, 110)       # x, y, w, h


def hair_polyline(k: int):
    import numpy as np
    xc = 140 + 5 * k
    ys = np.arange(20, 341, 8)
    xs = xc + 18 * np.sin(ys / 37.0 + k / 9.0)
    return np.stack([xs, ys], axis=1).round().astype(np.int32)


def hair_mask(k: int, W: int = 640, H: int = 360):
    """Telin (çizgi-sanat kenarı DAHİL) k. karedeki maskesi."""
    import cv2
    import numpy as np
    m = np.zeros((H, W), np.uint8)
    cv2.polylines(m, [hair_polyline(k)], False, 1, 22, cv2.LINE_8)
    return m


def _make_hair(path: str):
    import cv2
    import numpy as np
    rng = np.random.default_rng(5)
    W, H = 640, 360
    bg = cv2.GaussianBlur(rng.integers(60, 140, (H // 8, W // 8, 3), dtype=np.uint8), (3, 3), 0)
    bg = cv2.resize(bg, (W, H), interpolation=cv2.INTER_CUBIC)
    # bg'yi mavi-yeşile kaydır (saçla karışmasın, gerçekçi ayrık renkler)
    bg[..., 0] = (bg[..., 0] * 0.4).astype(np.uint8)
    x, y, w, h = HAIR_SIGN
    sign = bg.copy()
    cv2.rectangle(sign, (x, y), (x + w, y + h), (240, 240, 235), -1)
    cv2.rectangle(sign, (x, y), (x + w, y + h), (10, 10, 10), 3)
    cv2.putText(sign, "TABELA", (x + 18, y + 70), cv2.FONT_HERSHEY_SIMPLEX, 1.3, (15, 15, 15), 4, cv2.LINE_AA)
    proc = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
                             "-s", f"{W}x{H}", "-r", "24000/1001", "-i", "-", "-c:v", "libx264",
                             "-preset", "veryfast", "-crf", "14", "-g", "24", "-pix_fmt", "yuv420p", path],
                            stdin=subprocess.PIPE)
    for k in range(HAIR_N):
        fr = sign.copy()
        pl = hair_polyline(k)
        cv2.polylines(fr, [pl], False, (10, 10, 10), 22, cv2.LINE_8)          # çizgi-sanat kenarı
        cv2.polylines(fr, [pl], False, (120, 66, 32), 18, cv2.LINE_8)         # kahverengi tel
        cv2.polylines(fr, [pl + [3, 0]], False, (150, 90, 45), 4, cv2.LINE_8)  # parlama (doku)
        proc.stdin.write(fr.tobytes())
    proc.stdin.close()
    if proc.wait() != 0:
        raise RuntimeError("hair.mkv üretilemedi")
