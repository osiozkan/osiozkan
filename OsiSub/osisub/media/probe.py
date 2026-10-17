"""ffprobe ile medya bilgisi."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from fractions import Fraction

_NO_WINDOW = 0x08000000 if sys.platform.startswith("win") else 0   # CREATE_NO_WINDOW


def tool(name: str) -> str | None:
    """ffmpeg/ffprobe yolu: önce uygulama klasörü (bin/), sonra PATH."""
    import os

    from ..paths import bin_dirs
    exe = name + (".exe" if sys.platform.startswith("win") else "")
    for d in bin_dirs():
        p = os.path.join(d, exe)
        if os.path.isfile(p):
            return p
    return shutil.which(name)


def run(args: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(args, capture_output=True, creationflags=_NO_WINDOW, **kw)


@dataclass
class VideoStream:
    index: int
    codec: str
    width: int
    height: int
    fps: Fraction            # r_frame_rate (nominal)
    time_base: Fraction
    start: float             # saniye
    sar: Fraction = Fraction(1)

    @property
    def dar(self) -> float:
        return float(self.width * self.sar / self.height) if self.height else 0.0


@dataclass
class AudioStream:
    index: int
    codec: str
    rate: int
    channels: int
    language: str = ""
    title: str = ""


@dataclass
class MediaInfo:
    path: str
    duration: float
    format_start: float
    video: VideoStream | None = None
    audio: list[AudioStream] = field(default_factory=list)


def _frac(s: str | None, default=Fraction(0)) -> Fraction:
    try:
        if not s or s in ("0/0", "N/A"):
            return default
        return Fraction(s)
    except (ValueError, ZeroDivisionError):
        return default


def probe(path: str) -> MediaInfo:
    exe = tool("ffprobe")
    if not exe:
        raise RuntimeError("ffprobe bulunamadı (ffmpeg kurulu değil ya da PATH'te değil)")
    r = run([exe, "-v", "error", "-print_format", "json", "-show_format", "-show_streams", path])
    if r.returncode != 0:
        raise RuntimeError(f"ffprobe hata verdi: {r.stderr.decode('utf-8', 'replace').strip()}")
    data = json.loads(r.stdout.decode("utf-8", "replace"))
    fmt = data.get("format", {})
    info = MediaInfo(path=path, duration=float(fmt.get("duration") or 0),
                     format_start=float(fmt.get("start_time") or 0))
    for s in data.get("streams", []):
        kind = s.get("codec_type")
        disp = s.get("disposition", {})
        if kind == "video" and info.video is None and not disp.get("attached_pic"):
            info.video = VideoStream(
                index=int(s["index"]), codec=s.get("codec_name", ""),
                width=int(s.get("width") or 0), height=int(s.get("height") or 0),
                fps=_frac(s.get("r_frame_rate")) or _frac(s.get("avg_frame_rate"), Fraction(24000, 1001)),
                time_base=_frac(s.get("time_base"), Fraction(1, 1000)),
                start=float(s.get("start_time") or 0),
                sar=_frac(s.get("sample_aspect_ratio"), Fraction(1)) or Fraction(1))
        elif kind == "audio":
            tags = s.get("tags", {})
            info.audio.append(AudioStream(
                index=int(s["index"]), codec=s.get("codec_name", ""),
                rate=int(s.get("sample_rate") or 0), channels=int(s.get("channels") or 0),
                language=tags.get("language", ""), title=tags.get("title", "")))
    return info
