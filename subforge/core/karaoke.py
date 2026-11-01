"""Karaoke syllable helpers: split text into syllables, distribute/measure durations, emit \\k tags."""
from __future__ import annotations

import re
from dataclasses import dataclass

from .tags import parse_karaoke, split_blocks, Syllable

_ROMAJI = re.compile(
    r"(?:[bcdfghjklmnpqrstvwxyz]{0,2}(?:ya|yu|yo|[aeiou])[aeiou]{0,1}(?:n(?![aeiouy]))?|n|[^\s])", re.I)
_KANA_SMALL = "ゃゅょぁぃぅぇぉャュョァィゥェォっッー"


def split_text(text: str, mode: str = "char") -> list[str]:
    """mode: char | word | romaji. Spaces stay attached to the preceding syllable (Aegisub style)."""
    if mode == "word":
        return [m.group(0) for m in re.finditer(r"\S+\s*|\s+", text)]
    if mode == "romaji":
        out = []
        for word in re.finditer(r"\S+\s*", text):
            w = word.group(0)
            core = w.rstrip()
            trail = w[len(core):]
            parts = [m.group(0) for m in _ROMAJI.finditer(core)]
            if parts:
                parts[-1] += trail
            out.extend(parts)
        return out or [text]
    out = []
    for ch in text:
        if ch.isspace() and out:
            out[-1] += ch
        elif ch in _KANA_SMALL and out:
            out[-1] += ch
        else:
            out.append(ch)
    return out


def distribute(parts: list[str], total_cs: int) -> list[int]:
    """Equal durations (centiseconds) summing exactly to total_cs, weighting by visible chars."""
    if not parts:
        return []
    weights = [max(1, len(p.strip())) for p in parts]
    tw = sum(weights)
    durs = [int(total_cs * w / tw) for w in weights]
    durs[-1] += total_cs - sum(durs)
    return durs


def build(syllables: list[tuple[str, int]], ktype: str = "k", lead: str = "") -> str:
    """[(text, dur_cs)] -> '{\\k20}ka{\\k30}ra'. `lead` is prepended (non-karaoke tags)."""
    tag = {"k": "k", "kf": "kf", "ko": "ko", "K": "K"}.get(ktype, "k")
    return lead + "".join("{\\%s%d}%s" % (tag, d, t) for t, d in syllables)


def split_marks(text: str) -> list[str]:
    """Text with '|' separators (Aegisub 'split' syntax) -> syllable texts."""
    return text.split("|")


def strip_karaoke(text: str) -> str:
    def blk(m):
        b = re.sub(r"\\(?:kf|ko|kt|k|K)-?\d+(?:\.\d+)?", "", m.group(0))
        return "" if b == "{}" else b
    return re.sub(r"\{[^}]*\}", blk, text)


def to_syllables(text: str) -> list[tuple[str, int]]:
    return [(s.text, s.duration_cs) for s in parse_karaoke(text)]


def scale_to(durs: list[int], total_cs: int) -> list[int]:
    s = sum(durs)
    if s <= 0:
        return distribute([""] * len(durs), total_cs)
    out = [int(round(d * total_cs / s)) for d in durs]
    out[-1] += total_cs - sum(out)
    return out


def tap_durations(tap_times_ms: list[int], line_start_ms: int, line_end_ms: int) -> list[int]:
    """Syllable-end tap times (ms, absolute) -> durations in cs; last syllable ends at line end if no tap."""
    prev = line_start_ms
    durs = []
    for t in tap_times_ms:
        durs.append(max(0, int(round((t - prev) / 10.0))))
        prev = t
    return durs
