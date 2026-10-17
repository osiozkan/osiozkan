"""Ses: ffmpeg ile mono PCM çözme, tepe zarfı (Peaks) ve spektrogram.

Zaman ekseni Aegisub ile aynıdır: ses örneği 0 = videonun İLK KARESİ
(FFMS2'nin FFMS_DELAY_FIRST_VIDEO_TRACK kipi). Ses akışı videodan geç
başlıyorsa başa sessizlik eklenir, erken başlıyorsa baştan kırpılır.

Bellek: 22050 Hz int16 mono — 24 dakikalık bölüm ~63 MB; tepe zarfı
1 ms'lik kutularda min/max (~6 MB); spektrogram 10 ms adım × 128 log bant uint8.
"""

from __future__ import annotations

import os
import subprocess

import numpy as np

from . import cache
from .probe import _NO_WINDOW, probe, tool

RATE = 22050
CACHE_VERSION = 2


class AudioData:
    def __init__(self, pcm: np.ndarray, rate: int = RATE):
        self.pcm = pcm                  # int16 mono
        self.rate = rate
        self.peaks = Peaks(pcm, rate)
        self.spectrogram: Spectrogram | None = None
        self.cache_dir: str | None = None

    @property
    def duration_ms(self) -> int:
        return int(len(self.pcm) * 1000 // self.rate)

    def slice_bytes(self, start_ms: int, end_ms: int) -> bytes:
        a = max(0, int(start_ms * self.rate // 1000))
        b = max(a, min(len(self.pcm), int(end_ms * self.rate // 1000)))
        return self.pcm[a:b].tobytes()


def decode_audio(path: str, stream: int | None = None, rate: int = RATE,
                 align_to_video: bool = True, progress=None, use_cache: bool = True,
                 with_spectrogram: bool = True) -> AudioData:
    info = probe(path)
    if not info.audio:
        raise RuntimeError("Dosyada ses akışı yok")
    a = info.audio[0] if stream is None else next(s for s in info.audio if s.index == stream)
    cdir = cache.entry_dir(path) if use_cache else None
    cfile = os.path.join(cdir, f"pcm_{a.index}_{rate}_v{CACHE_VERSION}.npy") if cdir else None
    if cfile and os.path.exists(cfile):
        try:
            pcm = np.load(cfile, mmap_mode="r")
            return _finish(AudioData(np.asarray(pcm), rate), cdir, progress, with_spectrogram)
        except (OSError, ValueError):
            pass
    exe = tool("ffmpeg")
    if not exe:
        raise RuntimeError("ffmpeg bulunamadı")
    args = [exe, "-v", "error", "-nostdin", "-i", path, "-map", f"0:{a.index}", "-vn", "-sn", "-dn",
            "-ac", "1", "-ar", str(rate), "-f", "s16le", "-acodec", "pcm_s16le", "-"]
    proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=_NO_WINDOW)
    chunks, total = [], 0
    expect = max(1, int(info.duration * rate * 2))
    while True:
        buf = proc.stdout.read(1 << 20)
        if not buf:
            break
        chunks.append(buf)
        total += len(buf)
        if progress:
            progress(min(0.95, total / expect) * 0.7)
    err = proc.stderr.read().decode("utf-8", "replace")
    proc.wait()
    proc.stdout.close()
    proc.stderr.close()
    if proc.returncode != 0 and total == 0:
        raise RuntimeError(f"Ses çözülemedi: {err.strip()}")
    pcm = np.frombuffer(b"".join(chunks), dtype="<i2").copy()
    # Videonun ilk karesine hizala (Aegisub/FFMS2 davranışı)
    if align_to_video and info.video is not None:
        a_start = _stream_start(path, a.index)
        try:
            from .frameindex import build_index
            v_first = build_index(path).offset_s
        except Exception:  # noqa: BLE001
            v_first = info.video.start
        shift = int(round((a_start - v_first) * rate))
        if shift > 0:
            pcm = np.concatenate([np.zeros(shift, dtype=np.int16), pcm])
        elif shift < 0:
            pcm = pcm[-shift:]
    if cfile:
        try:
            np.save(cfile, pcm)
        except OSError:
            pass
    ad = AudioData(pcm, rate)
    ad.cache_dir = cdir
    return _finish(ad, cdir, progress, with_spectrogram)


def _stream_start(path: str, index: int) -> float:
    from .probe import run
    r = run([tool("ffprobe"), "-v", "error", "-select_streams", str(index), "-show_entries",
             "stream=start_time", "-of", "csv=p=0", path])
    try:
        return float(r.stdout.decode().strip().split("\n")[0])
    except ValueError:
        return 0.0


def _finish(ad: AudioData, cdir, progress, with_spectrogram: bool = True):
    ad.cache_dir = cdir
    if progress:
        progress(0.8)
    sfile = os.path.join(cdir, f"spec_{ad.rate}_v{CACHE_VERSION}.npy") if cdir else None
    if sfile and os.path.exists(sfile):
        try:
            ad.spectrogram = Spectrogram(np.load(sfile), ad.rate)
        except (OSError, ValueError):
            ad.spectrogram = None
    if ad.spectrogram is None and with_spectrogram:
        ensure_spectrogram(ad)
    if progress:
        progress(1.0)
    return ad


def ensure_spectrogram(ad: AudioData) -> "Spectrogram":
    """Spektrogramı hesapla (yoksa) ve önbelleğe yaz. Arka planda çağrılabilir."""
    if ad.spectrogram is not None:
        return ad.spectrogram
    sp = Spectrogram.compute(ad.pcm, ad.rate)
    cdir = getattr(ad, "cache_dir", None)
    if cdir:
        try:
            np.save(os.path.join(cdir, f"spec_{ad.rate}_v{CACHE_VERSION}.npy"), sp.data)
        except OSError:
            pass
    ad.spectrogram = sp
    return sp


def load_wav_array(pcm: np.ndarray, rate: int = RATE) -> AudioData:
    """Testler ve hazır diziler için."""
    ad = AudioData(pcm.astype(np.int16), rate)
    ad.spectrogram = Spectrogram.compute(ad.pcm, rate)
    return ad


class Peaks:
    """1 ms'lik kutularda min/max. window() çizime hazır sütunlar döndürür."""

    def __init__(self, pcm: np.ndarray, rate: int):
        self.rate = rate
        self.pcm = pcm
        spb = rate / 1000.0
        n_bins = int(len(pcm) // spb)
        if n_bins == 0:
            self.mins = np.zeros(1, np.int16)
            self.maxs = np.zeros(1, np.int16)
            return
        edges = np.floor(np.arange(n_bins) * spb).astype(np.int64)
        self.mins = np.minimum.reduceat(pcm, edges)
        self.maxs = np.maximum.reduceat(pcm, edges)

    def window(self, t0_ms: float, t1_ms: float, width: int) -> tuple[np.ndarray, np.ndarray]:
        """[t0, t1) aralığını `width` sütuna indirger; -1..1 arası (min, max)."""
        width = max(1, int(width))
        mins = np.zeros(width, np.float32)
        maxs = np.zeros(width, np.float32)
        if t1_ms <= t0_ms:
            return mins, maxs
        ms_per_px = (t1_ms - t0_ms) / width
        if ms_per_px >= 1.0:
            src_min, src_max, scale = self.mins, self.maxs, 1.0
        else:
            src_min = src_max = self.pcm
            scale = self.rate / 1000.0
        n = len(src_min)
        edges = (t0_ms + np.arange(width + 1) * ms_per_px) * scale
        e = np.floor(edges).astype(np.int64)
        lo = e[:-1]
        hi = np.maximum(e[1:], lo + 1)
        valid = (lo >= 0) & (lo < n)
        if not valid.any():
            return mins, maxs
        lo_c = np.clip(lo, 0, n - 1)
        hi_c = np.clip(hi, 1, n)
        # reduceat yalnız artan indekslerle; ardışık eşit kenarlar tek örneğe düşer
        idx = lo_c[valid]
        mn = np.minimum.reduceat(src_min, idx) if len(idx) else np.array([])
        mx = np.maximum.reduceat(src_max, idx) if len(idx) else np.array([])
        # reduceat son dilimi dizinin sonuna kadar alır; hi ile sınırla
        last = np.flatnonzero(valid)[-1]
        seg = slice(lo_c[last], hi_c[last])
        if seg.stop > seg.start:
            mn[-1] = src_min[seg].min()
            mx[-1] = src_max[seg].max()
        mins[valid] = mn / 32768.0
        maxs[valid] = mx / 32768.0
        return mins, maxs


class Spectrogram:
    """10 ms adım, 1024 noktalı FFT, 128 logaritmik bant, uint8 (0 = -80 dB)."""

    BANDS = 128
    NFFT = 1024

    def __init__(self, data: np.ndarray, rate: int):
        self.data = data            # (frames, BANDS) uint8
        self.rate = rate
        self.hop = max(1, rate // 100)
        self.hop_ms = self.hop * 1000.0 / rate

    @classmethod
    def compute(cls, pcm: np.ndarray, rate: int) -> "Spectrogram":
        hop = max(1, rate // 100)
        nfft = cls.NFFT
        x = pcm.astype(np.float32) / 32768.0
        n_frames = max(0, (len(x) - nfft) // hop + 1)
        if n_frames == 0:
            return cls(np.zeros((1, cls.BANDS), np.uint8), rate)
        win = np.hanning(nfft).astype(np.float32)
        freqs = np.fft.rfftfreq(nfft, 1.0 / rate)
        edges = np.geomspace(40.0, rate / 2.0, cls.BANDS + 1)
        band_of = np.clip(np.searchsorted(edges, freqs) - 1, 0, cls.BANDS - 1)
        M = np.zeros((len(freqs), cls.BANDS), np.float32)
        M[np.arange(len(freqs)), band_of] = 1.0
        M /= np.maximum(M.sum(axis=0, keepdims=True), 1.0)
        # Boş kalan alçak bantları en yakın FFT kutusundan doldur
        empty = np.flatnonzero(M.sum(axis=0) == 0)
        for b in empty:
            centre = np.sqrt(edges[b] * edges[b + 1])
            M[int(np.argmin(np.abs(freqs - centre))), b] = 1.0
        out = np.empty((n_frames, cls.BANDS), np.float16)
        step = 4096
        from numpy.lib.stride_tricks import as_strided
        for s in range(0, n_frames, step):
            k = min(step, n_frames - s)
            base = x[s * hop: s * hop + (k - 1) * hop + nfft]
            frames = as_strided(base, shape=(k, nfft), strides=(base.strides[0] * hop, base.strides[0]))
            spec = np.fft.rfft(frames * win, axis=1)
            power = (spec.real ** 2 + spec.imag ** 2).astype(np.float32)
            out[s:s + k] = 10.0 * np.log10(power @ M + 1e-12)
        top = float(np.percentile(out.astype(np.float32), 99.9)) if out.size else 0.0
        norm = np.clip((out.astype(np.float32) - (top - 80.0)) / 80.0, 0.0, 1.0)
        return cls((norm * 255).astype(np.uint8), rate)

    def window(self, t0_ms: float, t1_ms: float, width: int) -> np.ndarray:
        """(BANDS, width) uint8; yakından bakışta en yakın kare, uzaktan en büyük değer."""
        width = max(1, int(width))
        out = np.zeros((self.BANDS, width), np.uint8)
        if t1_ms <= t0_ms or len(self.data) == 0:
            return out
        # Çerçeve i, [i*hop, i*hop+nfft) örneklerini kapsar; merkezine göre konumla
        centre_ms = self.NFFT / 2 * 1000.0 / self.rate
        f = (t0_ms + (np.arange(width + 1) * (t1_ms - t0_ms) / width) - centre_ms) / self.hop_ms
        lo = np.floor(f[:-1]).astype(np.int64)
        hi = np.maximum(np.floor(f[1:]).astype(np.int64), lo + 1)
        n = len(self.data)
        valid = (lo >= 0) & (lo < n)
        if not valid.any():
            return out
        if (hi - lo).max() <= 1:
            out[:, valid] = self.data[lo[valid]].T
        else:
            idx = np.clip(lo[valid], 0, n - 1)
            stop = int(min(n, hi[valid][-1]))       # son dilim dizinin sonuna taşmasın
            red = np.maximum.reduceat(self.data[:max(stop, idx[-1] + 1)], idx, axis=0)
            out[:, valid] = red.T
        return out
