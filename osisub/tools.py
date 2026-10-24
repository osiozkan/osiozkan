"""Timing and text tools operating on Documents."""
import re
from typing import Iterable, Optional, List
from .ass import Document, Event


def _sel(doc: Document, sel: Optional[Iterable[int]]):
    return range(len(doc.events)) if sel is None else sorted(set(i for i in sel if 0 <= i < len(doc.events)))


def shift(doc: Document, ms: int, sel=None, start=True, end=True):
    for i in _sel(doc, sel):
        e = doc.events[i]
        if start: e.start = max(0, e.start + ms)
        if end: e.end = max(0, e.end + ms)


def stretch(doc: Document, factor: float, sel=None):
    for i in _sel(doc, sel):
        e = doc.events[i]
        e.start = max(0, int(round(e.start * factor))); e.end = max(0, int(round(e.end * factor)))


def sort_events(doc: Document, key="start"):
    k = {"start": lambda e: (e.start, e.end), "end": lambda e: (e.end, e.start),
         "style": lambda e: (e.style, e.start), "actor": lambda e: (e.actor, e.start),
         "layer": lambda e: (e.layer, e.start)}[key]
    doc.events.sort(key=k)


def fix_overlaps(doc: Document, sel=None):
    """Trim end of a line to next line's start when same style overlap."""
    idx = sorted(_sel(doc, sel), key=lambda i: (doc.events[i].start, doc.events[i].end))
    n = 0
    for a, b in zip(idx, idx[1:]):
        ea, eb = doc.events[a], doc.events[b]
        if not ea.comment and not eb.comment and ea.style == eb.style and eb.start < ea.end and eb.start > ea.start:
            ea.end = eb.start; n += 1
    return n


def snap_gaps(doc: Document, threshold_ms: int = 300, sel=None):
    """Close small gaps between consecutive lines (chaining)."""
    idx = sorted(_sel(doc, sel), key=lambda i: (doc.events[i].start, doc.events[i].end))
    n = 0
    for a, b in zip(idx, idx[1:]):
        ea, eb = doc.events[a], doc.events[b]
        if 0 < eb.start - ea.end <= threshold_ms:
            ea.end = eb.start; n += 1
    return n


def snap_to_keyframes(doc: Document, keyframes_ms: List[int], sel=None, before=250, after=250):
    import bisect
    kf = sorted(keyframes_ms)
    if not kf: return 0
    n = 0
    for i in _sel(doc, sel):
        e = doc.events[i]
        j = bisect.bisect_left(kf, e.start); c = [kf[x] for x in (j - 1, j) if 0 <= x < len(kf)]
        if c:
            s = min(c, key=lambda k: abs(k - e.start))
            if -before <= s - e.start <= after: e.start = s; n += 1
        j = bisect.bisect_left(kf, e.end); c = [kf[x] for x in (j - 1, j) if 0 <= x < len(kf)]
        if c:
            t = min(c, key=lambda k: abs(k - e.end))
            if -before <= t - e.end <= after: e.end = t; n += 1
    return n


def frames_to_ms(frame: int, fps: float) -> int:
    return int(round(frame * 1000.0 / fps))


def ms_to_frame(ms: int, fps: float) -> int:
    return int(ms * fps / 1000.0)


def find_replace(doc: Document, find: str, repl: str, regex=False, case=False, field="text", sel=None) -> int:
    flags = 0 if case else re.I
    pat = re.compile(find if regex else re.escape(find), flags)
    n = 0
    for i in _sel(doc, sel):
        e = doc.events[i]
        old = getattr(e, field)
        new, c = pat.subn(repl if regex else repl.replace("\\", "\\\\"), old)
        if c:
            setattr(e, field, new); n += c
    return n


def split_event(doc: Document, index: int, char_pos: int):
    e = doc.events[index]
    mid = (e.start + e.end) // 2
    a = Event(**{**e.__dict__, "end": mid, "text": e.text[:char_pos].rstrip()})
    b = Event(**{**e.__dict__, "start": mid, "text": e.text[char_pos:].lstrip()})
    doc.events[index:index + 1] = [a, b]


def merge_events(doc: Document, indices: List[int], sep="\\N"):
    ix = sorted(indices)
    evs = [doc.events[i] for i in ix]
    m = Event(**{**evs[0].__dict__, "start": min(e.start for e in evs), "end": max(e.end for e in evs),
                 "text": sep.join(e.text for e in evs)})
    for i in reversed(ix[1:]):
        del doc.events[i]
    doc.events[ix[0]] = m


def resample(doc: Document, new_x: int, new_y: int):
    """Scale styles/margins when PlayRes changes."""
    ox, oy = doc.play_res
    fx, fy = new_x / ox, new_y / oy
    for s in doc.styles:
        s.fontsize *= fy; s.outline *= fy; s.shadow *= fy; s.spacing *= fx
        s.margin_l = int(round(s.margin_l * fx)); s.margin_r = int(round(s.margin_r * fx)); s.margin_t = int(round(s.margin_t * fy))
    for e in doc.events:
        e.margin_l = int(round(e.margin_l * fx)); e.margin_r = int(round(e.margin_r * fx)); e.margin_t = int(round(e.margin_t * fy))
    doc.set_info("PlayResX", new_x); doc.set_info("PlayResY", new_y)
