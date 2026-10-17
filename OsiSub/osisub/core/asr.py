"""Konuşma tanıma (Whisper, faster-whisper / CTranslate2) ile yazıya dökme.

Kullanım alanları:
  * Japonca (ya da İngilizce) konuşmayı yazıya döküp REFERANS olarak yüklemek:
    çevirirken her satırın yanında özgün söz görünür.
  * Sıfırdan zamanlama: konuşmadan zamanlanmış satırlar üretmek.

Sözcük zaman damgaları satırlara bölünür: sözcükler arası sessizlik > `gap`,
satır süresi > `max_dur`, metin > `max_chars` ya da cümle sonu (. ? ! 。 ？ ！) +
süre ≥ `min_dur` olunca yeni satır. İsteğe bağlı olarak sınırlar VAD konuşma
bölümlerine yaslanır (Whisper sözcük zamanları kaba olabilir, ±200 ms).

Model ilk kullanımda indirilir (<kullanıcı klasörü>/models): tiny ~75 MB, small ~460 MB,
medium ~1.5 GB, large-v3 ~3 GB. CUDA'lı NVIDIA kart varsa otomatik kullanılır.
Not: faster-whisper'ın kendi ses okuyucusu PyAV 19 ile uyumsuz; sesi biz çözüp
16 kHz float dizi olarak veriyoruz.
"""

from __future__ import annotations

import os

import numpy as np

MODELS = ["tiny", "base", "small", "medium", "large-v3"]
LANGS = {"ja": "Japonca", "en": "İngilizce", "tr": "Türkçe", "ko": "Korece", "zh": "Çince", None: "Otomatik"}
_SENT_END = (".", "?", "!", "。", "？", "！", "…")


def available() -> tuple[bool, str]:
    try:
        import faster_whisper  # noqa: F401
        return True, ""
    except ImportError:
        return False, "faster-whisper kurulu değil (pip install faster-whisper)"


def models_dir() -> str:
    from .. import config
    d = os.path.join(config.user_dir(), "models")
    os.makedirs(d, exist_ok=True)
    return d


def _device():
    try:
        import ctranslate2
        if ctranslate2.get_cuda_device_count() > 0:
            return "cuda", "float16"
    except Exception:  # noqa: BLE001
        pass
    return "cpu", "int8"


def resample_16k(pcm: np.ndarray, rate: int) -> np.ndarray:
    x = pcm.astype(np.float32) / 32768.0
    if rate == 16000:
        return x
    n = int(len(x) * 16000 / rate)
    t_new = np.arange(n, dtype=np.float64) * (rate / 16000.0)
    return np.interp(t_new, np.arange(len(x)), x).astype(np.float32)


def words_to_lines(words, gap: float = 0.6, max_dur: float = 5.0, max_chars: int = 84,
                   min_dur: float = 0.8):
    """[(start_s, end_s, word)] -> [(start_s, end_s, text)]"""
    lines = []
    cur = []
    for w in words:
        s, e, t = w
        if cur:
            prev_e = cur[-1][1]
            text = "".join(x[2] for x in cur)
            dur = cur[-1][1] - cur[0][0]
            ends = text.strip().endswith(_SENT_END)
            if (s - prev_e > gap or dur > max_dur or len(text) + len(t) > max_chars
                    or (ends and dur >= min_dur)):
                lines.append((cur[0][0], cur[-1][1], text.strip()))
                cur = []
        cur.append((s, e, t))
    if cur:
        lines.append((cur[0][0], cur[-1][1], "".join(x[2] for x in cur).strip()))
    return [ln for ln in lines if ln[2]]


def transcribe(pcm: np.ndarray, rate: int, language: str | None = "ja", model: str = "small",
               start_ms: int = 0, end_ms: int | None = None, progress=None, cancel=None,
               speech_segs=None, snap_ms: int = 400):
    """Döndürür: [(start_ms, end_ms, metin)] — Aegisub ekseninde."""
    from faster_whisper import WhisperModel
    device, ctype = _device()
    if progress:
        progress(0.0, f"Model yükleniyor ({model}, {device})…")
    m = WhisperModel(model, device=device, compute_type=ctype, download_root=models_dir())
    a = int(start_ms * rate / 1000)
    b = len(pcm) if end_ms is None else int(end_ms * rate / 1000)
    audio = resample_16k(pcm[a:b], rate)
    total = max(1e-6, len(audio) / 16000.0)
    segs, _info = m.transcribe(audio, language=language, vad_filter=True, word_timestamps=True,
                               condition_on_previous_text=False)
    words = []
    for seg in segs:
        if cancel is not None and cancel():
            raise RuntimeError("İptal edildi")
        if seg.words:
            words += [(w.start, w.end, w.word) for w in seg.words]
        else:
            words.append((seg.start, seg.end, seg.text))
        if progress:
            progress(min(0.99, seg.end / total), seg.text.strip())
    lines = words_to_lines(words, gap=0.6 if language not in ("ja", "zh") else 0.5)
    out = []
    for s, e, t in lines:
        sm, em = start_ms + int(s * 1000), start_ms + int(e * 1000)
        if speech_segs:
            sm, em = _snap(sm, em, speech_segs, snap_ms)
        out.append((sm, max(sm + 100, em), t))
    if progress:
        progress(1.0, f"{len(out)} satır")
    return out


def _snap(s, e, segs, win):
    on = [a for a, _ in segs]
    off = [b for _, b in segs]
    bs = min(on, key=lambda x: abs(x - s))
    be = min(off, key=lambda x: abs(x - e))
    if abs(bs - s) <= win:
        s = bs
    if abs(be - e) <= win and be > s:
        e = be
    return s, e
