"""Timing and text batch tools (Aegisub timing post-processor + Subtitle Edit style operations).

All functions take a list of Event objects (use ``doc.events`` or a selection) and mutate in place.
They return the number of changed events.
"""
from __future__ import annotations

import bisect
import re
from typing import Iterable, Sequence

from .ass import ASSFile, Event, Style
from .tags import plain_text, split_blocks
from .textutil import tr_upper, tr_lower


# ------------------------------------------------------------------ timing

def shift(events: Iterable[Event], ms: int, which: str = "both") -> int:
    n = 0
    for e in events:
        if which in ("both", "start"):
            e.start_time = max(0, e.start_time + ms)
        if which in ("both", "end"):
            e.end_time = max(0, e.end_time + ms)
        if e.end_time < e.start_time:
            e.end_time = e.start_time
        n += 1
    return n


def shift_to_first(events: Sequence[Event], target_ms: int) -> int:
    ev = [e for e in events]
    if not ev:
        return 0
    first = min(e.start_time for e in ev)
    return shift(ev, target_ms - first)


def two_point_sync(events: Iterable[Event], src1: int, dst1: int, src2: int, dst2: int) -> int:
    """Linear remap so that src1->dst1 and src2->dst2 (fixes drift / framerate mismatch)."""
    if src2 == src1:
        raise ValueError("sync points must differ")
    a = (dst2 - dst1) / (src2 - src1)
    n = 0
    for e in events:
        e.start_time = max(0, int(round(dst1 + (e.start_time - src1) * a)))
        e.end_time = max(0, int(round(dst1 + (e.end_time - src1) * a)))
        n += 1
    return n


def change_framerate(events: Iterable[Event], src_fps: float, dst_fps: float) -> int:
    r = src_fps / dst_fps
    n = 0
    for e in events:
        e.start_time = int(round(e.start_time * r))
        e.end_time = int(round(e.end_time * r))
        n += 1
    return n


def snap_to_keyframes(events: Iterable[Event], keyframes_ms: Sequence[int], start_before: int = 350,
                      start_after: int = 350, end_before: int = 350, end_after: int = 350,
                      lead_in_keep: bool = True) -> int:
    """Aegisub-style: start snaps to keyframe within [-start_before,+start_after]; end snaps to
    (keyframe - 1ms-ish) within range. keyframes_ms sorted ascending."""
    kfs = list(keyframes_ms)
    if not kfs:
        return 0
    n = 0
    for e in events:
        ch = False
        i = bisect.bisect_left(kfs, e.start_time)
        cands = [k for k in kfs[max(0, i - 1):i + 1] if -start_before <= k - e.start_time <= start_after]
        if cands:
            k = min(cands, key=lambda k: abs(k - e.start_time))
            if k != e.start_time:
                e.start_time, ch = k, True
        i = bisect.bisect_left(kfs, e.end_time)
        cands = [k for k in kfs[max(0, i - 1):i + 1] if -end_before <= k - e.end_time <= end_after]
        if cands:
            k = min(cands, key=lambda k: abs(k - e.end_time))
            if k != e.end_time and k > e.start_time:
                e.end_time, ch = k, True
        n += ch
    return n


def lead_in_out(events: Iterable[Event], lead_in: int = 0, lead_out: int = 0) -> int:
    n = 0
    for e in events:
        if lead_in:
            e.start_time = max(0, e.start_time - lead_in)
        if lead_out:
            e.end_time += lead_out
        n += 1
    return n


def _sorted(events: Iterable[Event]) -> list[Event]:
    return sorted((e for e in events if not e.comment), key=lambda e: (e.start_time, e.end_time))


def bridge_gaps(events: Iterable[Event], max_gap: int = 250, min_gap: int = 0) -> int:
    """Close gaps shorter than max_gap by extending the previous line; keep `min_gap` between lines."""
    ev = _sorted(events)
    n = 0
    for a, b in zip(ev, ev[1:]):
        if a.layer != b.layer or a.style != b.style:
            continue
        gap = b.start_time - a.end_time
        if 0 < gap <= max_gap or (gap < 0 and min_gap):
            new_end = b.start_time - min_gap
            if new_end > a.start_time and new_end != a.end_time:
                a.end_time = new_end
                n += 1
    return n


def fix_overlaps(events: Iterable[Event], min_gap: int = 0) -> int:
    ev = _sorted(events)
    n = 0
    for a, b in zip(ev, ev[1:]):
        if a.layer == b.layer and a.style == b.style and a.end_time > b.start_time - min_gap:
            ne = b.start_time - min_gap
            if ne > a.start_time:
                a.end_time = ne
                n += 1
    return n


def enforce_duration(events: Iterable[Event], min_ms: int = 700, max_ms: int = 8000, optimal_cps: float | None = 15.0,
                     respect_next: bool = True) -> int:
    """Extend too-short lines (up to next line start) and clamp too-long ones."""
    ev = _sorted(events)
    n = 0
    for i, e in enumerate(ev):
        nxt = ev[i + 1].start_time if (i + 1 < len(ev) and respect_next) else None
        want = min_ms
        if optimal_cps:
            want = max(want, int(len(plain_text(e.text).replace("\n", "")) * 1000 / optimal_cps))
        want = min(want, max_ms)
        if e.end_time - e.start_time < want:
            ne = e.start_time + want
            if nxt is not None:
                ne = min(ne, max(nxt, e.end_time))
            if ne > e.end_time:
                e.end_time = ne
                n += 1
        if e.end_time - e.start_time > max_ms:
            e.end_time = e.start_time + max_ms
            n += 1
    return n


# ------------------------------------------------------------------ text

def merge_duplicates(doc: ASSFile, max_gap: int = 100) -> int:
    """Merge consecutive identical lines (same text/style) that touch within max_gap."""
    evs = sorted(doc.events, key=lambda e: (e.start_time, e.end_time))
    kill = []
    last: Event | None = None
    for e in evs:
        if e.comment:
            continue
        if last and e.text == last.text and e.style == last.style and e.layer == last.layer \
                and e.start_time - last.end_time <= max_gap:
            last.end_time = max(last.end_time, e.end_time)
            kill.append(e)
        else:
            last = e
    ids = {id(e) for e in kill}
    doc.lines = [l for l in doc.lines if id(l) not in ids]
    return len(kill)


def merge_events(doc: ASSFile, a: Event, b: Event, sep: str = "\\N") -> Event:
    a.text = a.text + sep + b.text
    a.start_time = min(a.start_time, b.start_time)
    a.end_time = max(a.end_time, b.end_time)
    doc.lines = [l for l in doc.lines if l is not b]
    return a


def split_event(doc: ASSFile, e: Event, at_ms: int | None = None, at_char: int | None = None) -> Event:
    """Split at plain-text char offset (proportional time) or explicit time (middle if neither)."""
    plain = plain_text(e.text).replace("\n", " ")
    if at_char is None:
        at_char = len(plain) // 2
    if at_ms is None:
        frac = at_char / max(1, len(plain))
        at_ms = e.start_time + int((e.end_time - e.start_time) * frac)
    # map plain offset to raw text offset (skip tag blocks)
    raw, count, cut = e.text, 0, len(e.text)
    pos = 0
    for is_tag, part in split_blocks(raw):
        if is_tag:
            pos += len(part)
            continue
        if count + len(part) >= at_char:
            cut = pos + (at_char - count)
            break
        count += len(part)
        pos += len(part)
    left, right = raw[:cut].rstrip(), raw[cut:].lstrip()
    b = e.copy()
    e.text, b.text = left, right
    b.start_time, e.end_time = at_ms, at_ms
    idx = next(i for i, l in enumerate(doc.lines) if l is e)
    doc.lines.insert(idx + 1, b)
    return b


def balance_lines(events: Iterable[Event], max_chars: int = 42) -> int:
    """Re-flow plain (tag-free) text to at most 2 balanced lines when it exceeds max_chars."""
    n = 0
    for e in events:
        if "{" in e.text:
            continue
        flat = e.text.replace("\\N", " ").replace("\\n", " ")
        flat = re.sub(r"\s+", " ", flat).strip()
        if len(flat) <= max_chars and "\\N" not in e.text:
            continue
        if len(flat) <= max_chars:
            e.text = flat
            n += 1
            continue
        best, mid = None, len(flat) / 2
        for m in re.finditer(r" ", flat):
            if best is None or abs(m.start() - mid) < abs(best - mid):
                best = m.start()
        if best is not None:
            e.text = flat[:best] + "\\N" + flat[best + 1:]
            n += 1
    return n


_HI = re.compile(r"[\[\(][^\]\)]*[\]\)]\s*|♪[^♪]*♪?\s*|^[A-ZÇĞİÖŞÜ ]{2,}:\s*", re.M)


def remove_hearing_impaired(doc: ASSFile) -> int:
    n = 0
    keep = []
    for l in doc.lines:
        if isinstance(l, Event) and not l.comment:
            new = _HI.sub("", l.text).strip()
            if new != l.text:
                n += 1
                l.text = new
            if not plain_text(l.text).strip():
                continue
        keep.append(l)
    doc.lines = keep
    return n


def fix_common_errors(events: Iterable[Event], turkish: bool = True) -> int:
    n = 0
    for e in events:
        t = e.text
        parts = []
        for is_tag, part in split_blocks(t):
            if not is_tag:
                part = re.sub(r"[ \t]{2,}", " ", part)
                part = re.sub(r"(?<=\S) +([,;!?]|\.(?!\.))", r"\1", part)
                part = re.sub(r"\.{4,}", "...", part)
                part = part.replace("…", "...")
                part = re.sub(r"\s+\\N", r"\\N", part)
                part = re.sub(r"\\N\s+", r"\\N", part)
            parts.append(part)
        new = "".join(parts).strip()
        new = re.sub(r"^(\{[^}]*\})?\s+", r"\1", new)
        if new != t:
            e.text = new
            n += 1
    return n


def find_replace(events: Iterable[Event], find: str, repl: str, field: str = "text", regex: bool = False,
                 case: bool = False, whole: bool = False) -> int:
    flags = 0 if case else re.I
    pat = find if regex else re.escape(find)
    if whole:
        pat = r"\b" + pat + r"\b"
    rx = re.compile(pat, flags)
    n = 0
    for e in events:
        old = getattr(e, field)
        new, k = rx.subn(repl if regex else repl.replace("\\", "\\\\"), old)
        if k:
            setattr(e, field, new)
            n += 1
    return n


def change_case(events: Iterable[Event], mode: str = "upper") -> int:
    """mode: upper | lower | sentence — Turkish-aware, tags untouched."""
    n = 0
    for e in events:
        out = []
        first = True
        for is_tag, part in split_blocks(e.text):
            if is_tag:
                out.append(part)
                continue
            if mode == "upper":
                part = tr_upper(part)
            elif mode == "lower":
                part = tr_lower(part)
            else:
                part = tr_lower(part)
                if first and part:
                    part = tr_upper(part[0]) + part[1:]
            if part.strip():
                first = False
            out.append(part)
        new = "".join(out)
        # \N etc. must stay intact after casing
        new = new.replace("\\n", "\\N") if "\\N" in e.text and mode == "upper" else new
        if new != e.text:
            e.text = new
            n += 1
    return n


# ------------------------------------------------------------------ styles

def rename_style(doc: ASSFile, old: str, new: str) -> int:
    n = 0
    for l in doc.lines:
        if isinstance(l, Style) and l.name == old:
            l.name = new
            n += 1
        elif isinstance(l, Event) and l.style == old:
            l.style = new
            n += 1
    return n


def resample(doc: ASSFile, new_x: int, new_y: int, scale_tags: bool = True) -> None:
    ox, oy = doc.play_res
    fx, fy = new_x / ox, new_y / oy
    f = fy
    for l in doc.lines:
        if isinstance(l, Style):
            l.fontsize = round(l.fontsize * f, 2)
            l.outline = round(l.outline * f, 2)
            l.shadow = round(l.shadow * f, 2)
            l.spacing = round(l.spacing * fx, 2)
            l.margin_l = int(round(l.margin_l * fx))
            l.margin_r = int(round(l.margin_r * fx))
            l.margin_t = l.margin_b = int(round(l.margin_t * fy))
        elif isinstance(l, Event):
            l.margin_l = int(round(l.margin_l * fx))
            l.margin_r = int(round(l.margin_r * fx))
            l.margin_t = l.margin_b = int(round(l.margin_t * fy))
            if scale_tags:
                l.text = _scale_tags(l.text, fx, fy)
    doc.set_info("PlayResX", new_x)
    doc.set_info("PlayResY", new_y)


def _fmt(v: float) -> str:
    return ("%.2f" % v).rstrip("0").rstrip(".")


def _scale_tags(text: str, fx: float, fy: float) -> str:
    def blk(m):
        b = m.group(0)
        b = re.sub(r"\\(pos|org)\(([-\d.]+),([-\d.]+)\)",
                   lambda k: f"\\{k.group(1)}({_fmt(float(k.group(2)) * fx)},{_fmt(float(k.group(3)) * fy)})", b)
        b = re.sub(r"\\move\(([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)",
                   lambda k: "\\move(%s,%s,%s,%s" % (_fmt(float(k.group(1)) * fx), _fmt(float(k.group(2)) * fy),
                                                     _fmt(float(k.group(3)) * fx), _fmt(float(k.group(4)) * fy)), b)
        for tag in ("fs", "bord", "xbord", "ybord", "shad", "xshad", "yshad", "blur", "be"):
            b = re.sub(r"\\" + tag + r"(?![a-z])(-?[\d.]+)", lambda k, t=tag: "\\" + t + _fmt(float(k.group(1)) * fy), b) \
                if tag not in ("fs",) else re.sub(r"\\fs(?![a-z])(-?[\d.]+)", lambda k: "\\fs" + _fmt(float(k.group(1)) * fy), b)
        b = re.sub(r"\\fsp(-?[\d.]+)", lambda k: "\\fsp" + _fmt(float(k.group(1)) * fx), b)
        return b
    return re.sub(r"\{[^}]*\}", blk, text)
