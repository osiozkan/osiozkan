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


class AudioPlayer:
    """Plays an audio range through ffplay (no Qt multimedia dependency). Position is tracked by wall clock."""

    def __init__(self) -> None:
        self.proc: Optional[subprocess.Popen] = None
        self.start_ms = 0
        self.end_ms = 0
        self._t0 = 0.0

    @staticmethod
    def available() -> bool:
        return bool(shutil.which("ffplay"))

    def play(self, path: str, start_ms: int, end_ms: int = 0) -> None:
        import time
        self.stop()
        exe = shutil.which("ffplay")
        if not exe:
            return
        cmd = [exe, "-v", "quiet", "-nodisp", "-autoexit", "-ss", f"{start_ms / 1000:.3f}"]
        if end_ms > start_ms:
            cmd += ["-t", f"{(end_ms - start_ms) / 1000:.3f}"]
        cmd.append(path)
        self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.start_ms, self.end_ms, self._t0 = start_ms, end_ms, time.monotonic()

    def position(self) -> int:
        import time
        return self.start_ms + int((time.monotonic() - self._t0) * 1000)

    @property
    def playing(self) -> bool:
        return self.proc is not None and self.proc.poll() is None

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
        self.proc = None


class VideoStream:
    """Streams rendered frames (video + libass subtitles) from ffmpeg as raw RGB, paced by wall clock."""

    def __init__(self, info: VideoInfo, ass_text: str, start_ms: int, width: int = 640, fps: float = 15.0):
        import tempfile
        self.info = info
        self.start_ms = start_ms
        self.fps = fps
        self.width = max(2, width - width % 2)
        self.height = max(2, int(round(self.width * info.height / max(info.width, 1) / 2)) * 2)
        fd, self.ass_path = tempfile.mkstemp(suffix=".ass")
        os.close(fd)
        with open(self.ass_path, "w", encoding="utf-8") as f:
            f.write(ass_text)
        esc = self.ass_path.replace("\\", "/").replace(":", r"\:").replace("'", r"\'")
        vf = f"ass='{esc}',scale={self.width}:{self.height},fps={fps}"
        self.proc = subprocess.Popen(
            [FFMPEG, "-v", "error", "-i", info.path, "-ss", f"{start_ms / 1000:.3f}", "-vf", vf,
             "-pix_fmt", "rgb24", "-f", "rawvideo", "-"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

    def frames(self):
        """Yield (ms, bytes) paced in real time. Frames that are late are dropped."""
        import time
        size = self.width * self.height * 3
        t0 = time.monotonic()
        n = 0
        while True:
            buf = self.proc.stdout.read(size)
            if not buf or len(buf) < size:
                return
            due = t0 + n / self.fps
            now = time.monotonic()
            if now < due:
                time.sleep(due - now)
            elif now - due > 0.25:  # too late: skip display
                n += 1
                continue
            yield self.start_ms + int(n * 1000 / self.fps), buf
            n += 1

    def close(self):
        try:
            if self.proc.poll() is None:
                self.proc.kill()
        finally:
            try:
                os.unlink(self.ass_path)
            except OSError:
                pass
