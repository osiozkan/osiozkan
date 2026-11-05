"""QC checks: the recurring subtitle problems (CPS, overlaps, long lines, styles, tags, spacing)."""
from dataclasses import dataclass
import re
from .ass import Script
from .tags import visible_length, strip_tags, cps
from .timecode import ms_to_ass

@dataclass
class Issue:
    index: int; code: str; message: str; severity: str = "warn"
    def __str__(self): return f"[{self.severity}] #{self.index} {self.code}: {self.message}"

def lint(sc: Script, max_cps=20.0, max_line=42, min_dur=500, max_dur=7000, max_lines=2):
    out = []
    evs = [(i, e) for i, e in enumerate(sc.events, 1) if not e.comment]
    names = {s.name for s in sc.styles}
    for i, e in evs:
        d = e.end - e.start
        plain = strip_tags(e.text)
        if d <= 0: out.append(Issue(i, "duration", "süre sıfır/negatif", "error"))
        elif d < min_dur: out.append(Issue(i, "short", f"çok kısa ({d} ms)"))
        elif d > max_dur: out.append(Issue(i, "long", f"çok uzun ({d} ms)"))
        if d > 0 and e.effect == "" and cps(e.text, d) > max_cps:
            out.append(Issue(i, "cps", f"{cps(e.text, d):.1f} karakter/sn"))
        lines = plain.split("\n")
        if len(lines) > max_lines: out.append(Issue(i, "lines", f"{len(lines)} satır"))
        for ln in lines:
            if len(ln) > max_line: out.append(Issue(i, "linelen", f"satır {len(ln)} karakter")); break
        if e.style not in names: out.append(Issue(i, "style", f"tanımsız stil '{e.style}'", "error"))
        if e.text.count("{") != e.text.count("}"): out.append(Issue(i, "braces", "kapanmamış { }", "error"))
        if re.search(r"  +", plain): out.append(Issue(i, "spaces", "çift boşluk"))
        if plain != plain.strip(): out.append(Issue(i, "trim", "baş/son boşluk"))
        if not plain.strip() and "\\p1" not in e.text and "\\p" not in e.text: out.append(Issue(i, "empty", "boş satır"))
    srt = sorted(evs, key=lambda x: (x[1].layer, x[1].start))
    for (i1, a), (i2, b) in zip(srt, srt[1:]):
        if a.layer == b.layer and a.style == b.style and b.start < a.end and a.effect == b.effect == "" :
            out.append(Issue(i2, "overlap", f"#{i1} ile {a.end - b.start} ms çakışıyor"))
    return out

def fix_basic(sc: Script):
    """Safe auto-fixes: trim, collapse double spaces, drop empty braces."""
    n = 0
    for e in sc.events:
        t = re.sub(r"\{\}", "", e.text); t = re.sub(r"(?<=[^\\N]) {2,}", " ", t).strip()
        if t != e.text: e.text = t; n += 1
    return n
