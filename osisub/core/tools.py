"""Editing tools (timing, text fixes, QC) modelled after Aegisub / Subtitle Edit features."""
from __future__ import annotations
import re
from dataclasses import dataclass
from typing import Iterable, Optional, Sequence
from .ass import Document, Event
from .tags import strip_tags, BLOCK
from .timeutil import FrameRate


def _idx(doc: Document, sel: Optional[Iterable[int]]) -> list[int]:
    return list(range(len(doc.events))) if sel is None else [i for i in sel if 0 <= i < len(doc.events)]


# ------------------------------------------------------------------ timing
def shift(doc: Document, ms: int, sel=None, start=True, end=True) -> None:
    for i in _idx(doc, sel):
        e = doc.events[i]
        if start:
            e.start = max(0, e.start + ms)
        if end:
            e.end = max(e.start, e.end + ms)


def shift_from(doc: Document, ms: int, from_index: int) -> None:
    shift(doc, ms, range(from_index, len(doc.events)))


def stretch(doc: Document, factor: float, sel=None) -> None:
    for i in _idx(doc, sel):
        e = doc.events[i]
        e.start, e.end = int(e.start * factor), int(e.end * factor)


def sync_two_points(doc: Document, a_old: int, a_new: int, b_old: int, b_new: int) -> None:
    """Linear resync from two reference points (typical for framerate/cut drift)."""
    if b_old == a_old:
        return
    k = (b_new - a_new) / (b_old - a_old)
    for e in doc.events:
        e.start = max(0, int(round(a_new + (e.start - a_old) * k)))
        e.end = max(e.start, int(round(a_new + (e.end - a_old) * k)))


def convert_framerate(doc: Document, src_fps: float, dst_fps: float) -> None:
    stretch(doc, src_fps / dst_fps)


def snap_to_frames(doc: Document, fps: float, sel=None) -> None:
    fr = FrameRate(fps)
    for i in _idx(doc, sel):
        e = doc.events[i]
        e.start = fr.frame_to_ms_start(round(e.start * fps / 1000))
        e.end = fr.frame_to_ms_start(round(e.end * fps / 1000))


def snap_to_keyframes(doc: Document, keyframes_ms: Sequence[int], sel=None,
                      before_start=150, after_start=150, before_end=150, after_end=150,
                      end_gap=0) -> int:
    """Aegisub-style keyframe snapping. Returns number of snapped events."""
    kf = sorted(keyframes_ms)
    n = 0
    for i in _idx(doc, sel):
        e = doc.events[i]
        ch = False
        for k in kf:
            if e.start - before_start <= k <= e.start + after_start and k != e.start:
                e.start = k
                ch = True
                break
        for k in kf:
            tgt = k - end_gap
            if e.end - before_end <= tgt <= e.end + after_end and tgt != e.end and tgt > e.start:
                e.end = tgt
                ch = True
                break
        n += ch
    return n


def fix_overlaps(doc: Document, min_gap: int = 0, sel=None) -> int:
    ev = sorted(_idx(doc, sel), key=lambda i: (doc.events[i].start, doc.events[i].end))
    n = 0
    for a, b in zip(ev, ev[1:]):
        ea, eb = doc.events[a], doc.events[b]
        if ea.comment or eb.comment or ea.style != eb.style and False:
            continue
        if ea.end > eb.start - min_gap:
            new_end = max(ea.start + 1, eb.start - min_gap)
            if new_end != ea.end:
                ea.end = new_end
                n += 1
    return n


def enforce_duration(doc: Document, min_ms=1000, max_ms=7000, cps_target: float = 0, sel=None) -> int:
    """Extend short lines (up to next start) and cap long ones; optional reading speed target."""
    order = sorted(range(len(doc.events)), key=lambda i: doc.events[i].start)
    pos = {i: k for k, i in enumerate(order)}
    n = 0
    for i in _idx(doc, sel):
        e = doc.events[i]
        if e.comment:
            continue
        want = min_ms
        if cps_target > 0:
            chars = len(strip_tags(e.text).replace("\n", ""))
            want = max(want, int(chars / cps_target * 1000))
        want = min(want, max_ms)
        if e.duration < want:
            nxt = pos[i] + 1
            limit = doc.events[order[nxt]].start if nxt < len(order) else e.start + want
            e.end = max(e.end, min(e.start + want, max(limit, e.end)))
            n += 1
        elif e.duration > max_ms:
            e.end = e.start + max_ms
            n += 1
    return n


def sort_events(doc: Document, key: str = "start") -> None:
    keys = {
        "start": lambda e: (e.start, e.end), "end": lambda e: (e.end, e.start),
        "style": lambda e: (e.style, e.start), "actor": lambda e: (e.actor, e.start),
        "effect": lambda e: (e.effect, e.start), "layer": lambda e: (e.layer, e.start),
    }
    doc.events.sort(key=keys[key])


# ------------------------------------------------------------------ structure
def split_event(doc: Document, index: int, at_ms: Optional[int] = None, at_char: Optional[int] = None) -> int:
    e = doc.events[index]
    a, b = e.copy(), e.copy()
    if at_ms is None:
        at_ms = (e.start + e.end) // 2
    if at_char is not None:
        a.text, b.text = e.text[:at_char].rstrip(), e.text[at_char:].lstrip()
    a.end = b.start = at_ms
    doc.events[index:index + 1] = [a, b]
    return index + 1


def merge_events(doc: Document, indices: Sequence[int], sep: str = "\\N") -> int:
    idx = sorted(set(indices))
    if len(idx) < 2:
        return idx[0] if idx else -1
    first = doc.events[idx[0]]
    first.text = sep.join(doc.events[i].text for i in idx)
    first.start = min(doc.events[i].start for i in idx)
    first.end = max(doc.events[i].end for i in idx)
    for i in reversed(idx[1:]):
        del doc.events[i]
    return idx[0]


def merge_duplicates(doc: Document) -> int:
    """Merge consecutive events with identical text/style that touch or overlap."""
    out, n = [], 0
    for e in sorted(doc.events, key=lambda e: (e.start, e.end)):
        if out and not e.comment and not out[-1].comment and out[-1].text == e.text \
                and out[-1].style == e.style and e.start <= out[-1].end:
            out[-1].end = max(out[-1].end, e.end)
            n += 1
        else:
            out.append(e)
    doc.events = out
    return n


def remove_empty(doc: Document) -> int:
    before = len(doc.events)
    doc.events = [e for e in doc.events if e.comment or strip_tags(e.text).strip()]
    return before - len(doc.events)


def duplicate_events(doc: Document, indices: Sequence[int]) -> list[int]:
    new = []
    for i in sorted(indices, reverse=True):
        doc.events.insert(i + 1, doc.events[i].copy())
        new.append(i + 1)
    return sorted(new)


# ------------------------------------------------------------------ text tools
def search_replace(doc: Document, find: str, repl: str, field: str = "text", regex=False,
                   ignore_case=False, whole_word=False, sel=None) -> int:
    flags = re.I if ignore_case else 0
    pat = find if regex else re.escape(find)
    if whole_word:
        pat = r"\b" + pat + r"\b"
    rx = re.compile(pat, flags | re.UNICODE)
    n = 0
    for i in _idx(doc, sel):
        e = doc.events[i]
        old = getattr(e, field)
        new, k = rx.subn(repl, old)
        if k:
            setattr(e, field, new)
            n += k
    return n


def strip_all_tags(doc: Document, sel=None, keep_breaks=True) -> None:
    for i in _idx(doc, sel):
        e = doc.events[i]
        e.text = BLOCK.sub("", e.text)
        if not keep_breaks:
            e.text = e.text.replace("\\N", " ").replace("\\n", " ")


def tr_upper(s: str) -> str:
    return s.replace("i", "İ").replace("ı", "I").upper()


def tr_lower(s: str) -> str:
    return s.replace("İ", "i").replace("I", "ı").lower()


def change_case(doc: Document, mode: str, turkish=True, sel=None) -> None:
    """mode: upper | lower | title | sentence. Tags in {} are untouched."""
    up = tr_upper if turkish else str.upper
    lo = tr_lower if turkish else str.lower

    def conv(chunk: str) -> str:
        if mode == "upper":
            return up(chunk)
        if mode == "lower":
            return lo(chunk)
        if mode == "title":
            return re.sub(r"(\w)(\w*)", lambda m: up(m.group(1)) + lo(m.group(2)), chunk, flags=re.UNICODE)
        if mode == "sentence":
            out, start = [], True
            for ch in chunk:
                if start and ch.isalpha():
                    out.append(up(ch))
                    start = False
                else:
                    out.append(lo(ch))
                    if ch in ".!?…" or ch == "\n":
                        start = True
            return "".join(out)
        return chunk

    for i in _idx(doc, sel):
        e = doc.events[i]
        parts = re.split(r"(\{[^}]*\}|\\N|\\n|\\h)", e.text)
        e.text = "".join(p if (p.startswith("{") or p in ("\\N", "\\n", "\\h")) else conv(p) for p in parts)


def fix_common_errors(doc: Document, sel=None) -> int:
    """Whitespace/punctuation clean-ups (SE 'Fix common errors' subset)."""
    n = 0
    rules = [
        (r"[ \t]{2,}", " "),
        (r"\s+([,.;:!?…])", r"\1"),
        (r"\.\.\.(?!\.)", "…"),
        (r"^\s+|\s+$", ""),
        (r"\\N\s+", r"\\N"),
        (r"\s+\\N", r"\\N"),
        (r"(\\N){2,}", r"\\N"),
    ]
    for i in _idx(doc, sel):
        e = doc.events[i]
        old = e.text
        parts = re.split(r"(\{[^}]*\})", e.text)
        for k, p in enumerate(parts):
            if p.startswith("{"):
                continue
            for pat, rep in rules:
                p = re.sub(pat, rep, p)
            parts[k] = p
        e.text = "".join(parts).strip()
        if e.text != old:
            n += 1
    return n


def balance_lines(text: str, max_chars: int = 42) -> str:
    """Auto-break plain text into at most two balanced lines at a space (keeps tags)."""
    plain = strip_tags(text)
    if "\\N" in text or len(plain) <= max_chars:
        return text
    spaces = [m.start() for m in re.finditer(" ", text)]
    if not spaces:
        return text
    mid = len(text) / 2
    best = min(spaces, key=lambda p: abs(p - mid))
    return text[:best] + "\\N" + text[best + 1:]


def auto_break(doc: Document, max_chars=42, sel=None) -> int:
    n = 0
    for i in _idx(doc, sel):
        e = doc.events[i]
        new = balance_lines(e.text, max_chars)
        if new != e.text:
            e.text, n = new, n + 1
    return n


# ------------------------------------------------------------------ QC
@dataclass
class Issue:
    index: int
    kind: str
    message: str


def check(doc: Document, max_cps: float = 20, max_cpl: int = 42, min_dur: int = 500, max_dur: int = 8000,
          max_lines: int = 2, min_gap: int = 0) -> list[Issue]:
    issues: list[Issue] = []
    styles = {s.name for s in doc.styles}
    dialog = [(i, e) for i, e in enumerate(doc.events) if not e.comment]
    for i, e in dialog:
        text = strip_tags(e.text)
        lines = text.split("\n")
        chars = len(text.replace("\n", ""))
        if e.end <= e.start:
            issues.append(Issue(i, "timing", "end time is not after start time"))
        if e.duration and e.duration < min_dur:
            issues.append(Issue(i, "short", f"duration {e.duration}ms < {min_dur}ms"))
        if e.duration > max_dur:
            issues.append(Issue(i, "long", f"duration {e.duration}ms > {max_dur}ms"))
        if e.duration > 0 and chars / (e.duration / 1000) > max_cps:
            issues.append(Issue(i, "cps", f"{chars / (e.duration / 1000):.1f} chars/s > {max_cps}"))
        if any(len(l) > max_cpl for l in lines):
            issues.append(Issue(i, "cpl", f"line longer than {max_cpl} chars"))
        if len(lines) > max_lines:
            issues.append(Issue(i, "lines", f"{len(lines)} lines (max {max_lines})"))
        if not text.strip():
            issues.append(Issue(i, "empty", "empty text"))
        if e.style not in styles:
            issues.append(Issue(i, "style", f"unknown style '{e.style}'"))
        if re.search(r"[ \t]{2,}", text) or text != text.strip():
            issues.append(Issue(i, "whitespace", "extra whitespace"))
        if e.text.count("{") != e.text.count("}"):
            issues.append(Issue(i, "tags", "unbalanced override braces"))
    by_start = sorted(dialog, key=lambda p: (p[1].start, p[1].end))
    for (ia, a), (ib, b) in zip(by_start, by_start[1:]):
        if a.style == b.style and a.layer == b.layer and a.end > b.start and not (a.effect or b.effect):
            issues.append(Issue(ia, "overlap", f"overlaps line {ib + 1}"))
        elif 0 < b.start - a.end < min_gap:
            issues.append(Issue(ia, "gap", f"gap to next line {b.start - a.end}ms"))
    return sorted(issues, key=lambda x: x.index)


# ------------------------------------------------------------------ selection / post-processing
def select_lines(doc: Document, pattern: str, field: str = "text", regex: bool = False, case: bool = False,
                 invert: bool = False, include_comments: bool = True) -> list[int]:
    flags = 0 if case else re.I
    rx = re.compile(pattern if regex else re.escape(pattern), flags)
    out = []
    for i, e in enumerate(doc.events):
        if e.comment and not include_comments:
            continue
        hit = bool(rx.search(getattr(e, field)))
        if hit != invert:
            out.append(i)
    return out


def timing_postprocess(doc: Document, lead_in=0, lead_out=0, join_gap=0, keyframes: Sequence[int] = (),
                       kf_before=0, kf_after=0, sel=None) -> int:
    """Aegisub-style timing post-processor. Returns number of modified events."""
    idx = sorted(_idx(doc, sel), key=lambda i: (doc.events[i].start, doc.events[i].end))
    idx = [i for i in idx if not doc.events[i].comment]
    orig = [(doc.events[i].start, doc.events[i].end) for i in idx]
    for k, i in enumerate(idx):
        e = doc.events[i]
        prev_end = doc.events[idx[k - 1]].end if k else 0
        nxt_start = doc.events[idx[k + 1]].start if k + 1 < len(idx) else None
        e.start = max(prev_end if prev_end <= e.start else 0, e.start - lead_in, 0)
        e.end = e.end + lead_out
        if nxt_start is not None and e.end > nxt_start:
            e.end = max(nxt_start, e.start + 1)
    for k in range(len(idx) - 1):
        a, b = doc.events[idx[k]], doc.events[idx[k + 1]]
        if join_gap and 0 < b.start - a.end <= join_gap:
            mid = (a.end + b.start) // 2
            a.end = b.start = mid
    if keyframes and (kf_before or kf_after):
        snap_to_keyframes(doc, keyframes, idx, kf_before, kf_after, kf_before, kf_after)
    return sum(1 for k, i in enumerate(idx) if (doc.events[i].start, doc.events[i].end) != orig[k])
