"""Timing tools: Aegisub 'Timing Post-Processor' + Subtitle Edit style fixers."""
from .ass import Script
from .tags import cps

def snap_to_keyframes(sc: Script, kf_ms, start_before=250, start_after=250, end_before=250, end_after=250, selected=None):
    kf = sorted(kf_ms); n = 0
    evs = sc.events
    for i, e in enumerate(evs):
        if e.comment or (selected is not None and i not in selected): continue
        for k in kf:
            if e.start - start_before <= k <= e.start + start_after and k != e.start: e.start = k; n += 1; break
        for k in kf:
            if e.end - end_before <= k <= e.end + end_after and k != e.end: e.end = k; n += 1; break
    return n

def lead_in_out(sc: Script, lead_in=0, lead_out=0, selected=None):
    for i, e in enumerate(sc.events):
        if e.comment or (selected is not None and i not in selected): continue
        e.start = max(0, e.start - lead_in); e.end += lead_out

def close_gaps(sc: Script, max_gap=300, selected=None):
    """Join near-adjacent same-style lines: extend previous end to next start when gap <= max_gap."""
    evs = sorted((e for e in sc.events if not e.comment), key=lambda e: e.start); n = 0
    for a, b in zip(evs, evs[1:]):
        if 0 < b.start - a.end <= max_gap: a.end = b.start; n += 1
    return n

def fix_overlaps(sc: Script):
    evs = sorted((e for e in sc.events if not e.comment and e.effect == ""), key=lambda e: (e.layer, e.style, e.start)); n = 0
    for a, b in zip(evs, evs[1:]):
        if a.layer == b.layer and a.style == b.style and b.start < a.end: a.end = b.start; n += 1
    return n

def enforce_min_duration(sc: Script, min_ms=700, max_cps=17.0):
    n = 0
    evs = sorted((e for e in sc.events if not e.comment), key=lambda e: e.start)
    for i, e in enumerate(evs):
        need = max(min_ms, int(len(e.text) / max_cps * 1000)) if max_cps else min_ms
        if e.end - e.start < need:
            lim = evs[i + 1].start if i + 1 < len(evs) else e.start + need
            new_end = min(e.start + need, lim) if lim > e.end else e.end
            if new_end > e.end: e.end = new_end; n += 1
    return n

def split_lines_by_break(sc: Script):
    """Split events containing \\N into separate events with proportional timing (for dual-line -> single)."""
    from .ass import Event
    import copy
    out = []; n = 0
    for x in sc.entries:
        if x.kind == "event" and not x.comment and "\\N" in x.text:
            parts = x.text.split("\\N"); total = sum(max(len(p), 1) for p in parts); t = x.start
            for p in parts:
                e = copy.deepcopy(x); e.text = p; e.start = t
                t += int((x.end - x.start) * max(len(p), 1) / total); e.end = t; out.append(e)
            out[-1].end = x.end; n += 1
        else: out.append(x)
    sc.entries[:] = out; return n
