"""Kalite kontrol: anime altyazıda yaygın sorunları yakalar."""
from __future__ import annotations
import re
from dataclasses import dataclass
from .ass import AssFile
from .tags import unbalanced_braces
from .timing import cps


@dataclass
class Issue:
    index: int
    kind: str
    message: str
    severity: str = "warn"


def check(a: AssFile, *, max_cps=20.0, max_cpl=42, max_lines=2, min_ms=500, max_ms=7000, min_gap_ms=0) -> list[Issue]:
    out: list[Issue] = []
    names = {s.name for s in a.styles}
    for i, e in enumerate(a.events):
        if e.comment:
            continue
        if e.style not in names:
            out.append(Issue(i, "style", f"Tanımsız stil: {e.style}", "error"))
        if e.end <= e.start:
            out.append(Issue(i, "timing", "Bitiş ≤ başlangıç", "error"))
        else:
            if e.duration < min_ms:
                out.append(Issue(i, "short", f"Çok kısa ({e.duration} ms)"))
            if e.duration > max_ms:
                out.append(Issue(i, "long", f"Çok uzun ({e.duration} ms)"))
            if (c := cps(e)) > max_cps:
                out.append(Issue(i, "cps", f"Okuma hızı {c:.1f} cps"))
        if unbalanced_braces(e.text):
            out.append(Issue(i, "tags", "Dengesiz { } etiket parantezi", "error"))
        if not e.plain.strip() and not re.search(r"\\p[1-9]", e.text):
            out.append(Issue(i, "empty", "Boş metin"))
        rows = e.plain.split("\n")
        if len(rows) > max_lines:
            out.append(Issue(i, "lines", f"{len(rows)} satır"))
        if any(len(r) > max_cpl for r in rows) and not re.search(r"\\p[1-9]", e.text):
            out.append(Issue(i, "cpl", f"Satır {max(len(r) for r in rows)} karakter"))
        if re.search(r"  +", e.plain):
            out.append(Issue(i, "spaces", "Çift boşluk"))
        if re.search(r"^\s|\s$", e.plain.replace("\n", "")):
            out.append(Issue(i, "spaces", "Baş/son boşluk"))
        if re.search(r"\.\.\.\.+|\s[,.!?;:]", e.plain):
            out.append(Issue(i, "punct", "Şüpheli noktalama"))
    # çakışma (aynı stil + aynı layer)
    evs = sorted(((i, e) for i, e in enumerate(a.events) if not e.comment), key=lambda t: (t[1].style, t[1].layer, t[1].start))
    for (i, x), (j, y) in zip(evs, evs[1:]):
        if x.style == y.style and x.layer == y.layer and y.start < x.end:
            out.append(Issue(j, "overlap", f"Satır {i + 1} ile çakışıyor"))
        elif x.style == y.style and 0 < y.start - x.end < min_gap_ms:
            out.append(Issue(j, "gap", f"Önceki satırla boşluk {y.start - x.end} ms"))
    return sorted(out, key=lambda x: x.index)
