"""Media helpers: probing, keyframes and audio decoding via ffmpeg/ffprobe.

Audio is decoded once to a 16 kHz mono s16 PCM cache file and memory-mapped, so waveform
and spectrogram rendering stay fast and RAM usage stays flat even for feature-length files.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
from dataclasses import dataclass, field

import numpy as np

SAMPLE_RATE = 16000
CACHE_DIR = os.path.expanduser("~/.subforge/cache")


def have_ffmpeg() -> bool:
    return bool(shutil.which("ffmpeg") and shutil.which("ffprobe"))


@dataclass
class MediaInfo:
    path: str
    duration: float = 0.0
    width: int = 0
    height: int = 0
    fps: float = 0.0
    has_video: bool = False
    has_audio: bool = False
    audio_tracks: list = field(default_factory=list)


def _fps(s: str) -> float:
    try:
        a, b = s.split("/")
        return float(a) / float(b) if float(b) else 0.0
    except Exception:
        try:
            return float(s)
        except Exception:
            return 0.0


def probe(path: str) -> MediaInfo:
    out = subprocess.run(["ffprobe", "-v", "error", "-print_format", "json", "-show_format", "-show_streams", path],
                         capture_output=True, text=True, check=True).stdout
    j = json.loads(out)
    info = MediaInfo(path, duration=float(j.get("format", {}).get("duration", 0) or 0))
    for s in j.get("streams", []):
        if s.get("codec_type") == "video" and not info.has_video and s.get("disposition", {}).get("attached_pic", 0) == 0:
            info.has_video = True
            info.width, info.height = int(s.get("width", 0)), int(s.get("height", 0))
            info.fps = _fps(s.get("avg_frame_rate", "0/0")) or _fps(s.get("r_frame_rate", "0/0"))
        elif s.get("codec_type") == "audio":
            info.has_audio = True
            info.audio_tracks.append({"index": s.get("index"), "codec": s.get("codec_name"),
                                      "lang": s.get("tags", {}).get("language", ""),
                                      "channels": s.get("channels", 0)})
    return info


def keyframes(path: str) -> list[int]:
    """Keyframe timestamps in ms (video stream packets flagged K)."""
    p = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-skip_frame", "nokey",
                        "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0", path],
                       capture_output=True, text=True)
    res = []
    for ln in p.stdout.splitlines():
        parts = ln.split(",")
        if len(parts) >= 2 and "K" in parts[1]:
            try:
                res.append(int(round(float(parts[0]) * 1000)))
            except ValueError:
                pass
    return sorted(set(res))


def _cache_path(path: str, track: int) -> str:
    st = os.stat(path)
    key = hashlib.sha1(f"{os.path.abspath(path)}|{st.st_size}|{int(st.st_mtime)}|{track}".encode()).hexdigest()[:20]
    os.makedirs(CACHE_DIR, exist_ok=True)
    return os.path.join(CACHE_DIR, key + f".{SAMPLE_RATE}.s16")


class AudioData:
    """Memory-mapped mono PCM with peak/spectrum accessors."""

    def __init__(self, pcm_path: str):
        self.pcm_path = pcm_path
        self.samples = np.memmap(pcm_path, dtype=np.int16, mode="r")
        self.rate = SAMPLE_RATE

    @property
    def duration_ms(self) -> int:
        return int(len(self.samples) * 1000 / self.rate)

    def peaks(self, start_ms: float, end_ms: float, buckets: int):
        """(min[], max[]) normalised to -1..1 for `buckets` columns across [start,end)."""
        a = max(0, int(start_ms * self.rate / 1000))
        b = min(len(self.samples), int(end_ms * self.rate / 1000))
        if b <= a or buckets <= 0:
            z = np.zeros(max(buckets, 0), dtype=np.float32)
            return z, z
        seg = np.asarray(self.samples[a:b])
        n = len(seg)
        edges = np.linspace(0, n, buckets + 1).astype(np.int64)
        edges[-1] = n
        mins = np.empty(buckets, dtype=np.float32)
        maxs = np.empty(buckets, dtype=np.float32)
        if n >= buckets:
            idx = edges[:-1]
            mins = np.minimum.reduceat(seg, idx).astype(np.float32)
            maxs = np.maximum.reduceat(seg, idx).astype(np.float32)
        else:
            for i in range(buckets):
                j = min(n - 1, int(i * n / buckets))
                mins[i] = maxs[i] = seg[j]
        return mins / 32768.0, maxs / 32768.0

    def spectrogram(self, start_ms: float, end_ms: float, columns: int, rows: int = 128) -> np.ndarray:
        """Log-magnitude spectrogram (rows x columns), values 0..1."""
        a = max(0, int(start_ms * self.rate / 1000))
        b = min(len(self.samples), int(end_ms * self.rate / 1000))
        out = np.zeros((rows, columns), dtype=np.float32)
        if b - a < 64 or columns <= 0:
            return out
        seg = np.asarray(self.samples[a:b]).astype(np.float32) / 32768.0
        win = 512
        hop = max(1, (len(seg) - win) // max(1, columns)) if len(seg) > win else 1
        window = np.hanning(win).astype(np.float32)
        for c in range(columns):
            s = c * hop
            frame = seg[s:s + win]
            if len(frame) < win:
                frame = np.pad(frame, (0, win - len(frame)))
            mag = np.abs(np.fft.rfft(frame * window))[:win // 2]
            # compress to `rows` log-spaced bins
            out[:, c] = mag[np.clip(np.geomspace(1, win // 2 - 1, rows).astype(int), 0, win // 2 - 1)]
        out = np.log1p(out * 40)
        mx = out.max()
        return (out / mx) if mx > 0 else out


def load_audio(path: str, track: int = 0, progress=None) -> AudioData:
    pcm = _cache_path(path, track)
    if not os.path.exists(pcm):
        tmp = pcm + ".part"
        cmd = ["ffmpeg", "-v", "error", "-y", "-i", path, "-map", f"0:a:{track}", "-vn", "-ac", "1",
               "-ar", str(SAMPLE_RATE), "-f", "s16le", tmp]
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            raise RuntimeError(r.stderr.strip() or "ffmpeg failed")
        os.replace(tmp, pcm)
    return AudioData(pcm)


def read_keyframe_file(path: str, fps: float | None = None) -> list[int]:
    """Aegisub keyframe log (`# keyframe format v1` + fps + frame numbers) or plain ms/frame list."""
    ks: list[int] = []
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        lines = [l.strip() for l in fh if l.strip()]
    use_fps = fps
    for l in lines:
        if l.lower().startswith("fps"):
            try:
                use_fps = float(l.split()[1])
            except Exception:
                pass
            continue
        if l.startswith("#") or l[0].isalpha():
            continue
        try:
            v = int(l.split()[0])
        except ValueError:
            continue
        ks.append(int(round(v * 1000.0 / use_fps)) if use_fps else v)
    return sorted(ks)


def video_timecodes(path: str) -> list[float]:
    """Presentation timestamps (ms) of every video frame, via packet pts (fast, no decoding)."""
    p = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time",
                        "-of", "csv=p=0", path], capture_output=True, text=True)
    ts = []
    for ln in p.stdout.splitlines():
        ln = ln.strip().rstrip(",")
        if ln and ln != "N/A":
            try:
                ts.append(float(ln) * 1000.0)
            except ValueError:
                pass
    ts.sort()
    return ts


def is_vfr(ts: list[float], tol_ms: float = 2.5) -> bool:
    if len(ts) < 3:
        return False
    d = [b - a for a, b in zip(ts, ts[1:])]
    med = sorted(d)[len(d) // 2]
    # allow rounding jitter of container timestamps (ms resolution)
    return sum(1 for x in d if abs(x - med) > tol_ms) > max(2, len(d) // 200)
