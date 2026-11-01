"""Quality control: reading speed, line length, timing and syntax checks."""
from __future__ import annotations

import re
from dataclasses import dataclass, field

from .ass import ASSFile, Event
from .tags import plain_text, split_blocks


@dataclass
class QCSettings:
    max_cps: float = 20.0
    max_chars_per_line: int = 42
    max_lines: int = 2
    min_duration: int = 700
    max_duration: int = 8000
    min_gap: int = 0          # ms; 0 disables
    check_overlap: bool = True
    check_style_exists: bool = True
    check_empty: bool = True
    check_whitespace: bool = True
    check_tags: bool = True


@dataclass
class Issue:
    index: int            # index into doc.events
    code: str
    severity: str         # "error" | "warning" | "info"
    message: str


def cps(ev: Event) -> float:
    txt = plain_text(ev.text).replace("\n", "")
    dur = max(1, ev.end_time - ev.start_time)
    return len(txt) * 1000.0 / dur


def check(doc: ASSFile, s: QCSettings | None = None) -> list[Issue]:
    s = s or QCSettings()
    out: list[Issue] = []
    styles = {st.name for st in doc.styles}
    events = doc.events
    order = sorted(range(len(events)), key=lambda i: (events[i].start_time, events[i].end_time))
    prev_i = None
    for rank, i in enumerate(order):
        e = events[i]
        if e.comment:
            continue
        txt = plain_text(e.text)
        dur = e.end_time - e.start_time
        if s.check_empty and not txt.strip() and not re.search(r"\\p[1-9]", e.text):
            out.append(Issue(i, "empty", "warning", "Boş satır"))
            continue
        if dur <= 0:
            out.append(Issue(i, "negative", "error", "Bitiş başlangıçtan önce/aynı"))
        else:
            if dur < s.min_duration:
                out.append(Issue(i, "short", "warning", f"Süre çok kısa ({dur} ms)"))
            if dur > s.max_duration:
                out.append(Issue(i, "long", "warning", f"Süre çok uzun ({dur} ms)"))
            c = cps(e)
            if c > s.max_cps and not e.effect.lower().startswith(("karaoke", "template", "code", "fx")):
                out.append(Issue(i, "cps", "warning", f"Okuma hızı {c:.1f} cps > {s.max_cps:g}"))
        lines = txt.split("\n")
        if len(lines) > s.max_lines:
            out.append(Issue(i, "lines", "warning", f"{len(lines)} satır (> {s.max_lines})"))
        longest = max((len(l) for l in lines), default=0)
        if longest > s.max_chars_per_line:
            out.append(Issue(i, "width", "warning", f"Satır {longest} karakter (> {s.max_chars_per_line})"))
        if s.check_style_exists and e.style not in styles:
            out.append(Issue(i, "style", "error", f"Stil yok: {e.style}"))
        if s.check_whitespace:
            if re.search(r"  +", txt):
                out.append(Issue(i, "dblspace", "info", "Çift boşluk"))
            if txt != txt.strip(" ") and txt.strip():
                out.append(Issue(i, "trimspace", "info", "Baş/son boşluk"))
        if s.check_tags:
            depth = 0
            bad = False
            for ch in e.text:
                if ch == "{":
                    depth += 1
                    if depth > 1:
                        bad = True
                elif ch == "}":
                    depth -= 1
                    if depth < 0:
                        bad = True
            if depth != 0 or bad:
                out.append(Issue(i, "braces", "error", "Dengesiz { } bloğu"))
        if prev_i is not None:
            p = events[prev_i]
            if s.check_overlap and e.start_time < p.end_time and e.layer == p.layer and not p.comment and e.style == p.style:
                out.append(Issue(i, "overlap", "warning", f"Önceki satırla çakışıyor ({p.end_time - e.start_time} ms)"))
            elif s.min_gap and 0 <= e.start_time - p.end_time < s.min_gap:
                out.append(Issue(i, "gap", "info", f"Boşluk {e.start_time - p.end_time} ms < {s.min_gap}"))
        prev_i = i
    return out
