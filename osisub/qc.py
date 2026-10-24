"""Quality-control checks (CPS, line length, overlaps, gaps, style, tags, typography)."""
from dataclasses import dataclass, asdict
from typing import List
from .ass import Document
from .tags import plain_text, strip_tags


@dataclass
class Issue:
    index: int
    kind: str
    severity: str  # error | warn | info
    message: str

    def to_dict(self):
        return asdict(self)


DEFAULTS = dict(max_cps=20.0, max_line_chars=42, max_lines=2, min_duration=500, max_duration=7000,
                min_gap=0, max_overlap=0)


def check(doc: Document, **opt) -> List[Issue]:
    o = {**DEFAULTS, **opt}
    issues: List[Issue] = []
    styles = {s.name for s in doc.styles}
    dlg = [(i, e) for i, e in enumerate(doc.events) if not e.comment]
    for i, e in dlg:
        vis = plain_text(e.text)
        lines = vis.split("\n")
        dur = e.end - e.start
        if dur <= 0:
            issues.append(Issue(i, "duration", "error", "Süre sıfır veya negatif"))
        else:
            if dur < o["min_duration"]:
                issues.append(Issue(i, "duration", "warn", f"Çok kısa ({dur} ms)"))
            if dur > o["max_duration"]:
                issues.append(Issue(i, "duration", "warn", f"Çok uzun ({dur} ms)"))
            chars = len(vis.replace("\n", ""))
            cps = chars / (dur / 1000.0)
            if cps > o["max_cps"]:
                issues.append(Issue(i, "cps", "warn", f"Okuma hızı {cps:.1f} CPS (> {o['max_cps']:g})"))
        if len(lines) > o["max_lines"]:
            issues.append(Issue(i, "lines", "warn", f"{len(lines)} satır (> {o['max_lines']})"))
        for ln in lines:
            if len(ln) > o["max_line_chars"]:
                issues.append(Issue(i, "length", "warn", f"Satır {len(ln)} karakter (> {o['max_line_chars']})"))
                break
        if e.style not in styles:
            issues.append(Issue(i, "style", "error", f"Tanımsız stil: {e.style}"))
        if not vis.strip() and not e.effect:
            issues.append(Issue(i, "empty", "warn", "Boş satır"))
        if e.text.count("{") != e.text.count("}"):
            issues.append(Issue(i, "tags", "error", "Eşleşmeyen { } etiket parantezi"))
        if "  " in strip_tags(e.text):
            issues.append(Issue(i, "typo", "info", "Çift boşluk"))
        if vis != vis.strip(" "):
            issues.append(Issue(i, "typo", "info", "Baş/son boşluk"))
    byt = sorted(dlg, key=lambda p: (p[1].start, p[1].end))
    for (ia, a), (ib, b) in zip(byt, byt[1:]):
        if a.style != b.style or a.layer != b.layer:
            continue
        if b.start < a.end - o["max_overlap"]:
            issues.append(Issue(ib, "overlap", "warn", f"Satır {ia + 1} ile {a.end - b.start} ms çakışıyor"))
        elif 0 < b.start - a.end < o["min_gap"]:
            issues.append(Issue(ib, "gap", "info", f"Önceki satırla boşluk {b.start - a.end} ms"))
    return sorted(issues, key=lambda x: (x.index, x.kind))
