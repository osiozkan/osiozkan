"""Media helpers backed by ffmpeg/ffprobe: probing, frame grabbing (with libass subtitle burn-in),
audio peaks for the waveform and keyframe detection."""
from __future__ import annotations
import json
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from typing import Optional

FFMPEG = shutil.which("ffmpeg")
FFPROBE = shutil.which("ffprobe")


def available() -> bool:
    return bool(FFMPEG and FFPROBE)


@dataclass
class VideoInfo:
    path: str
    width: int = 0
    height: int = 0
    fps: float = 23.976
    duration_ms: int = 0
    has_audio: bool = False


def _fps(s: str) -> float:
    try:
        if "/" in s:
            a, b = s.split("/")
            return float(a) / float(b) if float(b) else 0.0
        return float(s)
    except ValueError:
        return 0.0


def probe(path: str) -> VideoInfo:
    if not FFPROBE:
        raise RuntimeError("ffprobe not found")
    out = subprocess.run([FFPROBE, "-v", "error", "-print_format", "json", "-show_streams", "-show_format", path],
                         capture_output=True, text=True, check=True).stdout
    j = json.loads(out)
    info = VideoInfo(path)
    for st in j.get("streams", []):
        if st.get("codec_type") == "video" and not info.width:
            info.width, info.height = int(st.get("width", 0)), int(st.get("height", 0))
            info.fps = _fps(st.get("avg_frame_rate", "0")) or _fps(st.get("r_frame_rate", "0")) or 23.976
        elif st.get("codec_type") == "audio":
            info.has_audio = True
    try:
        info.duration_ms = int(float(j["format"]["duration"]) * 1000)
    except (KeyError, ValueError):
        pass
    return info


def grab_frame(path: str, ms: int, ass_path: Optional[str] = None, width: int = 0,
               fonts_dir: Optional[str] = None) -> bytes:
    """Return a PNG of the frame at `ms`; if ass_path is given the subtitles are rendered by libass."""
    if not FFMPEG:
        raise RuntimeError("ffmpeg not found")
    vf = []
    if ass_path:
        esc = ass_path.replace("\\", "/").replace(":", r"\:").replace("'", r"\'")
        f = f"ass='{esc}'"
        if fonts_dir:
            f += ":fontsdir='" + fonts_dir.replace(":", r"\:") + "'"
        vf.append(f)
    if width:
        vf.append(f"scale={width}:-2")
    cmd = [FFMPEG, "-v", "error", "-ss", f"{ms / 1000:.3f}", "-i", path]
    if ass_path:
        # subtitles are timed against the stream; -ss before -i resets timestamps, so shift subtitle clock
        cmd = [FFMPEG, "-v", "error", "-i", path, "-ss", f"{ms / 1000:.3f}"]
    if vf:
        cmd += ["-vf", ",".join(vf)]
    cmd += ["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]
    r = subprocess.run(cmd, capture_output=True)
    if r.returncode != 0 or not r.stdout:
        raise RuntimeError(r.stderr.decode("utf-8", "replace")[:300])
    return r.stdout


def render_blank_with_subs(doc_text: str, ms: int, width: int, height: int) -> bytes:
    """Render a subtitle frame over a black canvas with libass (no video needed)."""
    if not FFMPEG:
        raise RuntimeError("ffmpeg not found")
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "s.ass")
        with open(p, "w", encoding="utf-8") as f:
            f.write(doc_text)
        esc = p.replace(":", r"\:")
        cmd = [FFMPEG, "-v", "error", "-f", "lavfi", "-i", f"color=c=0x202428:s={width}x{height}:r=25:d={ms / 1000 + 2:.3f}",
               "-vf", f"ass='{esc}'", "-ss", f"{ms / 1000:.3f}", "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]
        r = subprocess.run(cmd, capture_output=True)
        if r.returncode != 0 or not r.stdout:
            raise RuntimeError(r.stderr.decode("utf-8", "replace")[:300])
        return r.stdout


def audio_peaks(path: str, rate: int = 100, sample_rate: int = 8000):
    """Return numpy int array of per-bin absolute peaks (bins per second = `rate`)."""
    import numpy as np
    if not FFMPEG:
        raise RuntimeError("ffmpeg not found")
    p = subprocess.run([FFMPEG, "-v", "error", "-i", path, "-vn", "-ac", "1", "-ar", str(sample_rate),
                        "-f", "s16le", "-"], capture_output=True)
    data = np.frombuffer(p.stdout, dtype=np.int16)
    if data.size == 0:
        return np.zeros(0, dtype=np.int16), rate
    step = sample_rate // rate
    n = data.size // step
    data = np.abs(data[: n * step].astype(np.int32)).reshape(n, step).max(axis=1)
    return data.astype(np.int16), rate


def keyframes(path: str) -> list[int]:
    """Keyframe timestamps (ms) via ffprobe packet flags (video stream only)."""
    if not FFPROBE:
        raise RuntimeError("ffprobe not found")
    r = subprocess.run([FFPROBE, "-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags",
                        "-of", "csv=print_section=0", path], capture_output=True, text=True)
    out = []
    for line in r.stdout.splitlines():
        parts = line.strip().split(",")
        if len(parts) >= 2 and "K" in parts[1]:
            try:
                out.append(int(float(parts[0]) * 1000 + 0.5))
            except ValueError:
                pass
    return sorted(set(out))
