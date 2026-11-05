"""Time helpers. All internal times are integer milliseconds."""
import re

def ms_to_ass(ms: int) -> str:
    ms = max(0, int(ms))
    cs = (ms + 5) // 10          # Aegisub rounds to centiseconds
    h, cs = divmod(cs, 360000)
    m, cs = divmod(cs, 6000)
    s, cs = divmod(cs, 100)
    return f"{h}:{m:02d}:{s:02d}.{cs:02d}"

def ass_to_ms(t: str) -> int:
    m = re.match(r"\s*(\d+):(\d+):(\d+)[.:](\d+)", t)
    if not m:
        raise ValueError(f"bad ASS time: {t!r}")
    h, mi, s, f = m.groups()
    return ((int(h) * 60 + int(mi)) * 60 + int(s)) * 1000 + (int(f) * 10 if len(f) == 2 else int((f + '00')[:3]))

def ms_to_srt(ms: int, sep: str = ",") -> str:
    ms = max(0, int(ms))
    h, ms = divmod(ms, 3600000)
    m, ms = divmod(ms, 60000)
    s, ms = divmod(ms, 1000)
    return f"{h:02d}:{m:02d}:{s:02d}{sep}{ms:03d}"

def srt_to_ms(t: str) -> int:
    m = re.match(r"\s*(?:(\d+):)?(\d+):(\d+)[,.](\d+)", t)
    if not m:
        raise ValueError(f"bad time: {t!r}")
    h, mi, s, f = m.groups()
    return ((int(h or 0) * 60 + int(mi)) * 60 + int(s)) * 1000 + int((f + "00")[:3])

def frame_to_ms(frame: int, fps: float) -> int:
    """Aegisub-style: time at the middle-ish start of frame (rounded)."""
    return int(round(frame * 1000.0 / fps))

def ms_to_frame(ms: int, fps: float) -> int:
    return int(ms * fps / 1000.0 + 1e-6)
