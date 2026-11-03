"""Font tools: collect fonts used by a script, check installation, attach to [Fonts]."""
from __future__ import annotations
import os
import re
import shutil
import subprocess
import zipfile
from dataclasses import dataclass
from typing import Optional

from .ass import Document


@dataclass
class FontUse:
    family: str
    bold: bool = False
    italic: bool = False
    lines: int = 0
    path: Optional[str] = None      # resolved file
    installed: bool = False


def used_fonts(doc: Document) -> list[FontUse]:
    """All (family, bold, italic) combos referenced by styles actually used and by \\fn/\\b/\\i overrides."""
    uses: dict[tuple, FontUse] = {}
    styles = {s.name: s for s in doc.styles}

    def add(fam, b, i, n=1):
        fam = fam.lstrip("@").strip()
        if not fam: return
        k = (fam.lower(), bool(b), bool(i))
        u = uses.setdefault(k, FontUse(fam, bool(b), bool(i)))
        u.lines += n

    used_styles = {e.style for e in doc.events if not e.comment}
    for name in used_styles:
        s = styles.get(name)
        if s: add(s.fontname, s.bold, s.italic, sum(1 for e in doc.events if e.style == name and not e.comment))
    for e in doc.events:
        if e.comment: continue
        s = styles.get(e.style)
        cur = [s.fontname if s else "Arial", s.bold if s else False, s.italic if s else False]
        for blk in re.findall(r"\{[^}]*\}", e.text):
            for m in re.finditer(r"\\(fn([^\\}]*)|b(\d+)|i([01]))", blk):
                if m.group(2) is not None:
                    cur[0] = m.group(2) or (s.fontname if s else "Arial")
                elif m.group(3) is not None:
                    v = int(m.group(3)); cur[1] = (v == 1 or v >= 600) if v else (s.bold if s and False else False)
                elif m.group(4) is not None:
                    cur[2] = m.group(4) == "1"
                if m.group(2) is not None or m.group(3) is not None or m.group(4) is not None:
                    add(cur[0], cur[1], cur[2])
    return sorted(uses.values(), key=lambda u: (-u.lines, u.family.lower()))


def resolve(u: FontUse) -> FontUse:
    """Find font file with fontconfig (fc-match); marks installed if family matches."""
    if not shutil.which("fc-match"):
        return u
    pat = f"{u.family}:{'bold' if u.bold else 'regular'}{':italic' if u.italic else ''}"
    try:
        out = subprocess.run(["fc-match", "-f", "%{file}|%{family}", pat], capture_output=True, text=True, timeout=10).stdout
        path, _, fam = out.partition("|")
        u.path = path or None
        fams = [f.strip().lower() for f in fam.split(",")]
        u.installed = u.family.lower() in fams
    except Exception:
        pass
    return u


def check(doc: Document) -> list[FontUse]:
    return [resolve(u) for u in used_fonts(doc)]


def collect(doc: Document, dest: str, as_zip: bool = False) -> tuple[list[str], list[FontUse]]:
    """Copy font files to dest (folder or .zip). Returns (copied, missing)."""
    uses = check(doc)
    copied, missing = [], []
    files = {}
    for u in uses:
        if u.installed and u.path and os.path.exists(u.path):
            files[u.path] = u
        else:
            missing.append(u)
    if as_zip:
        with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as z:
            for p in files:
                z.write(p, os.path.basename(p)); copied.append(p)
    else:
        os.makedirs(dest, exist_ok=True)
        for p in files:
            shutil.copy2(p, dest); copied.append(p)
    return copied, missing


# ASS font attachment encoding (UUencode variant used by Aegisub)
def _uuencode(data: bytes) -> list[str]:
    out, line = [], ""
    for i in range(0, len(data), 3):
        chunk = data[i:i + 3]
        pad = 3 - len(chunk)
        c = chunk + b"\0" * pad
        v = (c[0] << 16) | (c[1] << 8) | c[2]
        enc = [((v >> 18) & 63) + 33, ((v >> 12) & 63) + 33, ((v >> 6) & 63) + 33, (v & 63) + 33]
        s = "".join(chr(x) for x in enc[: 4 - pad])
        line += s
        if len(line) >= 80:
            out.append(line); line = ""
    if line: out.append(line)
    return out


def attach_font(doc: Document, path: str) -> str:
    name = os.path.basename(path)
    data = open(path, "rb").read()
    lines = [f"fontname: {name}"] + _uuencode(data)
    sec = doc.sections.setdefault("Fonts", [])
    sec.extend(lines)
    return name
