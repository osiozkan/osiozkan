"""ffmpeg/ffprobe helpers: media info, keyframes, audio peaks for the waveform."""
from __future__ import annotations
import json
import shutil
import subprocess
from dataclasses import dataclass
from typing import Optional

import numpy as np


@dataclass
class MediaInfo:
    width: int = 0
    height: int = 0
    fps: float = 0.0
    duration: float = 0.0
    has_audio: bool = False
    has_video: bool = False


def have_ffmpeg() -> bool:
    return bool(shutil.which("ffmpeg") and shutil.which("ffprobe"))


def probe(path: str) -> MediaInfo:
    out = subprocess.run(["ffprobe", "-v", "error", "-print_format", "json", "-show_streams", "-show_format", path],
                         capture_output=True, text=True, check=True).stdout
    j = json.loads(out)
    mi = MediaInfo(duration=float(j.get("format", {}).get("duration", 0) or 0))
    for s in j.get("streams", []):
        if s["codec_type"] == "video" and not mi.has_video and s.get("disposition", {}).get("attached_pic", 0) == 0:
            mi.has_video = True
            mi.width, mi.height = s.get("width", 0), s.get("height", 0)
            num, _, den = (s.get("avg_frame_rate") or s.get("r_frame_rate") or "0/1").partition("/")
            try:
                mi.fps = float(num) / float(den or 1)
            except (ValueError, ZeroDivisionError):
                mi.fps = 0.0
        elif s["codec_type"] == "audio":
            mi.has_audio = True
    return mi


def keyframes(path: str) -> list[int]:
    """Keyframe times in ms (ffprobe packet flags)."""
    p = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "packet=pts_time,flags", "-of", "csv=p=0", path], capture_output=True, text=True)
    res = []
    for ln in p.stdout.splitlines():
        parts = ln.split(",")
        if len(parts) >= 2 and "K" in parts[1]:
            try:
                res.append(int(float(parts[0]) * 1000))
            except ValueError:
                pass
    return sorted(res)


def decode_audio(path: str, rate: int = 8000, start: float = 0.0, duration: Optional[float] = None) -> np.ndarray:
    """Mono float32 PCM in [-1,1]."""
    cmd = ["ffmpeg", "-v", "error", "-ss", str(start)]
    if duration:
        cmd += ["-t", str(duration)]
    cmd += ["-i", path, "-vn", "-ac", "1", "-ar", str(rate), "-f", "s16le", "-"]
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0


def peaks(samples: np.ndarray, rate: int, per_second: int = 100) -> np.ndarray:
    """(n,2) min/max envelope at per_second resolution."""
    step = max(1, rate // per_second)
    n = len(samples) // step
    if n == 0:
        return np.zeros((0, 2), dtype=np.float32)
    a = samples[: n * step].reshape(n, step)
    return np.stack([a.min(axis=1), a.max(axis=1)], axis=1)


def spectrogram(samples: np.ndarray, rate: int, win: int = 512, hop: int = 128) -> np.ndarray:
    """Log-magnitude spectrogram (frames x bins)."""
    if len(samples) < win:
        return np.zeros((0, win // 2))
    w = np.hanning(win).astype(np.float32)
    idx = np.arange(0, len(samples) - win, hop)
    frames = np.stack([samples[i:i + win] * w for i in idx])
    mag = np.abs(np.fft.rfft(frames, axis=1))[:, : win // 2]
    return np.log1p(mag * 50)


def detect_speech(samples: np.ndarray, rate: int, threshold: float = 0.02, min_len_ms=300, min_gap_ms=250):
    """Very simple energy VAD -> list of (start_ms, end_ms). Foundation for auto-timing."""
    hop = rate // 100
    n = len(samples) // hop
    if n == 0:
        return []
    e = np.sqrt((samples[: n * hop].reshape(n, hop) ** 2).mean(axis=1))
    noise = float(np.percentile(e, 15)) * 3
    cap = 0.5 * float(np.percentile(e, 95))
    on = e > max(threshold, min(noise, cap))
    segs, start = [], None
    for i, v in enumerate(on):
        if v and start is None:
            start = i
        elif not v and start is not None:
            segs.append([start * 10, i * 10]); start = None
    if start is not None:
        segs.append([start * 10, n * 10])
    merged = []
    for s in segs:
        if merged and s[0] - merged[-1][1] < min_gap_ms:
            merged[-1][1] = s[1]
        else:
            merged.append(s)
    return [(a, b) for a, b in merged if b - a >= min_len_ms]
