"""ASS-specific utilities: resolution resampling, font usage, karaoke helpers, style import."""
from __future__ import annotations
import re
import shutil
import subprocess
from .ass import Document, Style, Event
from .tags import BLOCK


# ------------------------------------------------------------------ number formatting
def _fmt(v: float, places: int = 2) -> str:
    s = f"{v:.{places}f}".rstrip("0").rstrip(".")
    return s if s not in ("", "-0") else "0"


# ------------------------------------------------------------------ resample
_SCALE1 = re.compile(r"\\(fs|fsp|bord|xbord|ybord|shad|xshad|yshad)(-?\d+(?:\.\d+)?)")
_POS = re.compile(r"\\(pos|org)\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\)")
_MOVE = re.compile(r"\\move\(([^)]*)\)")
_CLIP = re.compile(r"\\(i?clip)\(([^)]*)\)")
_DRAW_SCALE = re.compile(r"\\p(\d+)")


def _scale_block(block: str, sx: float, sy: float, sf: float, in_draw: list) -> str:
    def one(m):
        k, v = m.group(1), float(m.group(2))
        f = sx if k in ("xbord", "xshad") else sy if k in ("ybord", "yshad") else sf
        return f"\\{k}{_fmt(v * f)}"
    b = _SCALE1.sub(one, block)
    b = _POS.sub(lambda m: f"\\{m.group(1)}({_fmt(float(m.group(2)) * sx)},{_fmt(float(m.group(3)) * sy)})", b)

    def mv(m):
        nums = [x.strip() for x in m.group(1).split(",")]
        out = []
        for i, n in enumerate(nums):
            try:
                v = float(n)
            except ValueError:
                out.append(n); continue
            if i < 4:
                out.append(_fmt(v * (sx if i % 2 == 0 else sy)))
            else:
                out.append(n)
        return "\\move(" + ",".join(out) + ")"
    b = _MOVE.sub(mv, b)

    def clip(m):
        args = [a.strip() for a in m.group(2).split(",")]
        if len(args) == 4:
            try:
                v = [float(a) for a in args]
                return f"\\{m.group(1)}({_fmt(v[0] * sx)},{_fmt(v[1] * sy)},{_fmt(v[2] * sx)},{_fmt(v[3] * sy)})"
            except ValueError:
                return m.group(0)
        inner = m.group(2)
        # vector clip: optional scale then drawing commands
        parts = inner.split(",", 1) if re.match(r"^\s*\d+\s*,", inner) else [None, inner]
        scale = parts[0]
        return f"\\{m.group(1)}(" + (scale + "," if scale is not None else "") + _scale_draw(parts[1], sx, sy) + ")"
    b = _CLIP.sub(clip, b)
    for m in _DRAW_SCALE.finditer(b):
        in_draw[0] = int(m.group(1)) > 0
    return b


def _scale_draw(s: str, sx: float, sy: float) -> str:
    toks = s.split()
    out, i = [], 0
    for t in toks:
        if re.fullmatch(r"[a-zA-Z]+", t):
            out.append(t); i = 0
        else:
            try:
                v = float(t)
                out.append(_fmt(v * (sx if i % 2 == 0 else sy)))
                i += 1
            except ValueError:
                out.append(t)
    return " ".join(out)


def scale_text(text: str, sx: float, sy: float) -> str:
    sf = sy
    in_draw = [False]
    out, pos = [], 0
    for m in BLOCK.finditer(text):
        seg = text[pos:m.start()]
        out.append(_scale_draw(seg, sx, sy) if in_draw[0] and seg else seg)
        out.append(_scale_block(m.group(0), sx, sy, sf, in_draw))
        pos = m.end()
    seg = text[pos:]
    out.append(_scale_draw(seg, sx, sy) if in_draw[0] and seg else seg)
    return "".join(out)


def resample(doc: Document, new_x: int, new_y: int, change_ar_margins: bool = True) -> None:
    ox, oy = doc.res
    if (ox, oy) == (new_x, new_y) or not ox or not oy:
        doc.set_info("PlayResX", str(new_x)); doc.set_info("PlayResY", str(new_y)); return
    sx, sy = new_x / ox, new_y / oy
    for s in doc.styles:
        s.fontsize = round(s.fontsize * sy, 2)
        s.outline = round(s.outline * sy, 2)
        s.shadow = round(s.shadow * sy, 2)
        s.spacing = round(s.spacing * sx, 2)
        s.margin_l, s.margin_r = int(round(s.margin_l * sx)), int(round(s.margin_r * sx))
        s.margin_v = int(round(s.margin_v * sy))
    for e in doc.events:
        e.margin_l, e.margin_r = int(round(e.margin_l * sx)), int(round(e.margin_r * sx))
        e.margin_v = int(round(e.margin_v * sy))
        e.text = scale_text(e.text, sx, sy)
    doc.set_info("PlayResX", str(new_x))
    doc.set_info("PlayResY", str(new_y))


# ------------------------------------------------------------------ fonts
def fonts_used(doc: Document) -> dict[str, set[str]]:
    """font name -> set of places (style names or 'line N')"""
    used: dict[str, set[str]] = {}
    styles = {s.name: s for s in doc.styles}
    for s in doc.styles:
        used.setdefault(s.fontname, set()).add(f"stil {s.name}")
    for i, e in enumerate(doc.events):
        if e.comment:
            continue
        for m in re.finditer(r"\\fn([^\\}]+)", e.text):
            used.setdefault(m.group(1).strip(), set()).add(f"satır {i + 1}")
    return used


def installed_fonts() -> set[str]:
    fc = shutil.which("fc-list")
    names: set[str] = set()
    if not fc:
        return names
    out = subprocess.run([fc, ":", "family"], capture_output=True, text=True).stdout
    for line in out.splitlines():
        for n in line.split(","):
            names.add(n.strip().lower())
    return names


def missing_fonts(doc: Document, installed: set[str] | None = None) -> dict[str, set[str]]:
    inst = installed if installed is not None else installed_fonts()
    if not inst:
        return {}
    return {f: w for f, w in fonts_used(doc).items() if f.lstrip("@").lower() not in inst}


# ------------------------------------------------------------------ styles
def import_styles(dst: Document, src: Document, names: list[str] | None = None, overwrite: bool = True) -> int:
    n = 0
    by = {s.name: i for i, s in enumerate(dst.styles)}
    for s in src.styles:
        if names is not None and s.name not in names:
            continue
        c = Style(**s.__dict__)
        if s.name in by:
            if overwrite:
                dst.styles[by[s.name]] = c; n += 1
        else:
            dst.styles.append(c); n += 1
    return n


def unused_styles(doc: Document) -> list[str]:
    used = {e.style for e in doc.events}
    return [s.name for s in doc.styles if s.name not in used and s.name != "Default"]


# ------------------------------------------------------------------ karaoke
_K = re.compile(r"\{[^}]*\\[kK][fo]?\d+[^}]*\}")


def has_karaoke(text: str) -> bool:
    return bool(_K.search(text))


def parse_syllables(text: str) -> list[dict]:
    """Split a karaoke line into syllables: [{'dur': cs, 'tag': 'k', 'text': ...}] (other tags kept in text)."""
    syls, cur = [], {"dur": 0, "tag": "k", "text": ""}
    pos = 0
    for m in BLOCK.finditer(text):
        cur["text"] += text[pos:m.start()]
        pos = m.end()
        block = m.group(0)[1:-1]
        parts = re.split(r"(\\[kK][fo]?-?\d+)", block)
        kept = ""
        for p in parts:
            km = re.fullmatch(r"\\([kK][fo]?)(-?\d+)", p)
            if km:
                if kept:
                    cur["text"] += "{" + kept + "}"; kept = ""
                syls.append(cur)
                cur = {"dur": int(km.group(2)), "tag": km.group(1), "text": ""}
            else:
                kept += p
        if kept:
            cur["text"] += "{" + kept + "}"
    cur["text"] += text[pos:]
    syls.append(cur)
    return syls


def build_karaoke(syls: list[dict]) -> str:
    out = []
    for i, s in enumerate(syls):
        if i == 0 and not s["text"] and s["dur"] == 0:
            continue
        out.append(("{\\%s%d}" % (s["tag"], s["dur"])) + s["text"])
    return "".join(out)


def auto_karaoke(text: str, duration_ms: int, mode: str = "syllable", tag: str = "kf") -> str:
    """Spread the line duration evenly over syllables (vowel-group split) / characters / words."""
    plain = BLOCK.sub("", text)
    if mode == "char":
        units = [c for c in plain]
    elif mode == "word":
        units = re.findall(r"\s*\S+", plain)
    else:
        units = re.findall(r"[^aeıioöuüAEIİOÖUÜ\s]*[aeıioöuüAEIİOÖUÜ]+(?:[^aeıioöuüAEIİOÖUÜ\s](?![aeıioöuüAEIİOÖUÜ]))?|\s+|[^aeıioöuüAEIİOÖUÜ\s]+", plain)
    units = [u for u in units if u != ""]
    if not units:
        return text
    weights = [max(1, len(u.strip())) if u.strip() else 0 for u in units]
    total = sum(weights) or 1
    cs_total = duration_ms // 10
    out, used = [], 0
    for i, (u, w) in enumerate(zip(units, weights)):
        dur = cs_total - used if i == len(units) - 1 else int(round(cs_total * w / total))
        used += dur
        out.append(("{\\%s%d}" % (tag, max(0, dur))) + u)
    return "".join(out)


def apply_auto_karaoke(doc: Document, indices, mode="syllable", tag="kf") -> int:
    n = 0
    for i in indices:
        e = doc.events[i]
        if e.comment:
            continue
        e.text = auto_karaoke(e.text, e.duration, mode, tag)
        n += 1
    return n


def lead_in_out(doc: Document, indices, lead_in=300, lead_out=300) -> None:
    """Extend line start/end and compensate karaoke (first syllable gets the extra lead-in as a silent \\k)."""
    for i in indices:
        e = doc.events[i]
        e.start = max(0, e.start - lead_in)
        e.end += lead_out
        if lead_in and has_karaoke(e.text):
            syls = parse_syllables(e.text)
            if syls and not syls[0]["text"] and syls[0]["dur"] == 0:
                syls[0] = {"dur": lead_in // 10, "tag": "k", "text": ""}
                e.text = "{\\k%d}" % (lead_in // 10) + build_karaoke(syls)
            else:
                e.text = "{\\k%d}" % (lead_in // 10) + e.text
