"""Fonts used by a script, system font lookup, and ASS embedded font (uuencode) attachments."""
from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass

from .ass import ASSFile, Event, Other, Style, S_FONTS, S_GRAPHICS


@dataclass(frozen=True)
class FontUse:
    family: str
    bold: bool = False
    italic: bool = False

    def label(self) -> str:
        return self.family + (" (kalın)" if self.bold else "") + (" (italik)" if self.italic else "")


def used_fonts(doc: ASSFile) -> dict[FontUse, int]:
    """FontUse -> number of lines/styles using it (override \\fn \\b \\i tracked per line, simplified)."""
    out: dict[FontUse, int] = {}
    smap = doc.style_map()

    def add(u):
        out[u] = out.get(u, 0) + 1

    for st in doc.styles:
        add(FontUse(st.fontname.lstrip("@"), st.bold, st.italic))
    for e in doc.events:
        if e.comment:
            continue
        st = smap.get(e.style) or (doc.styles[0] if doc.styles else Style())
        fam, b, i = st.fontname.lstrip("@"), st.bold, st.italic
        seen = {FontUse(fam, b, i)}
        for blk in re.findall(r"\{[^}]*\}", e.text):
            for m in re.finditer(r"\\(fn|b|i)([^\\}]*)", blk):
                t, v = m.groups()
                if t == "fn":
                    fam = v.strip().lstrip("@") or st.fontname.lstrip("@")
                elif t == "b":
                    b = (v.strip() not in ("0", "")) if v.strip() else st.bold
                elif t == "i":
                    i = (v.strip() not in ("0", "")) if v.strip() else st.italic
                seen.add(FontUse(fam, b, i))
        for u in seen:
            if u != FontUse(st.fontname.lstrip("@"), st.bold, st.italic):
                add(u)
    return out


# ---------------------------------------------------------------- system fonts

_index_cache: dict | None = None


def font_index(refresh=False) -> dict[str, list[tuple[str, str]]]:
    """family(lower) -> [(file, style)]. Uses fontconfig where available, registry on Windows."""
    global _index_cache
    if _index_cache is not None and not refresh:
        return _index_cache
    idx: dict[str, list[tuple[str, str]]] = {}
    if shutil.which("fc-list"):
        out = subprocess.run(["fc-list", "--format", "%{file}\t%{family}\t%{style}\t%{fullname}\n"],
                             capture_output=True, text=True).stdout
        for ln in out.splitlines():
            parts = ln.split("\t")
            if len(parts) < 3:
                continue
            f, fam, sty = parts[0], parts[1], parts[2]
            names = set(n.strip().lower() for n in fam.split(","))
            if len(parts) > 3:
                names |= {n.strip().lower() for n in parts[3].split(",")}
            for n in names:
                idx.setdefault(n, []).append((f, sty))
    elif sys.platform == "win32":
        try:
            import winreg
            for root in (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER):
                try:
                    k = winreg.OpenKey(root, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts")
                except OSError:
                    continue
                i = 0
                while True:
                    try:
                        name, path, _ = winreg.EnumValue(k, i)
                    except OSError:
                        break
                    i += 1
                    if not os.path.isabs(path):
                        path = os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts", path)
                    base = re.sub(r"\s*\((TrueType|OpenType)\)$", "", name)
                    fam = re.sub(r"\s+(Bold|Italic|Regular|Oblique|Light|Medium)+.*$", "", base).strip()
                    idx.setdefault(fam.lower(), []).append((path, base))
                    idx.setdefault(base.lower(), []).append((path, base))
        except Exception:
            pass
    _index_cache = idx
    return idx


def find_font_files(use: FontUse, index=None) -> list[str]:
    index = index if index is not None else font_index()
    cands = index.get(use.family.lower(), [])
    if not cands:
        return []
    want = []
    for f, sty in cands:
        s = sty.lower()
        isb, isi = "bold" in s or "heavy" in s or "black" in s, "italic" in s or "oblique" in s
        if isb == use.bold and isi == use.italic:
            want.append(f)
    return want or [f for f, _ in cands[:1]]


def missing_fonts(doc: ASSFile, index=None) -> list[FontUse]:
    index = index if index is not None else font_index()
    return [u for u in used_fonts(doc) if not index.get(u.family.lower())]


def collect(doc: ASSFile, dest: str, index=None) -> tuple[list[str], list[FontUse]]:
    os.makedirs(dest, exist_ok=True)
    copied, missing = [], []
    for u in used_fonts(doc):
        files = find_font_files(u, index)
        if not files:
            missing.append(u)
            continue
        for f in files:
            tgt = os.path.join(dest, os.path.basename(f))
            if not os.path.exists(tgt):
                shutil.copy2(f, tgt)
            copied.append(tgt)
    return sorted(set(copied)), missing


# ---------------------------------------------------------------- attachments (uuencode, Aegisub flavour)

def uuencode(data: bytes, linebreaks: bool = True) -> str:
    out, written = [], 0
    n = len(data)
    for pos in range(0, n, 3):
        chunk = data[pos:pos + 3]
        src = list(chunk) + [0] * (3 - len(chunk))
        dst = [src[0] >> 2, ((src[0] & 3) << 4) | ((src[1] & 0xF0) >> 4),
               ((src[1] & 0xF) << 2) | ((src[2] & 0xC0) >> 6), src[2] & 0x3F]
        cnt = min(n - pos + 1, 4)
        out.append("".join(chr(d + 33) for d in dst[:cnt]))
        written += cnt
        if linebreaks and written == 80 and pos + 3 < n:
            out.append("\r\n")
            written = 0
    return "".join(out)


def uudecode(text: str) -> bytes:
    s = "".join(text.split())
    out = bytearray()
    for i in range(0, len(s), 4):
        grp = [ord(c) - 33 for c in s[i:i + 4]]
        n = len(grp)
        grp += [0] * (4 - n)
        b = [(grp[0] << 2) | (grp[1] >> 4), ((grp[1] & 0xF) << 4) | (grp[2] >> 2), ((grp[2] & 3) << 6) | grp[3]]
        out += bytes(b[:max(0, n - 1)])
    return bytes(out)


@dataclass
class Attachment:
    name: str
    data: bytes
    graphic: bool = False


def list_attachments(doc: ASSFile) -> list[Attachment]:
    res: list[Attachment] = []
    cur_name, buf, graphic = None, [], False

    def flush():
        nonlocal cur_name, buf
        if cur_name is not None:
            res.append(Attachment(cur_name, uudecode("".join(buf)), graphic))
        cur_name, buf = None, []

    for l in doc.lines:
        if not isinstance(l, Other) or l.section not in (S_FONTS, S_GRAPHICS):
            continue
        m = re.match(r"^(fontname|filename):\s*(.*)$", l.raw.strip(), re.I)
        if m:
            flush()
            cur_name, graphic = m.group(2).strip(), l.section == S_GRAPHICS
        elif cur_name is not None:
            buf.append(l.raw.strip())
    flush()
    return res


def set_attachments(doc: ASSFile, atts: list[Attachment]) -> None:
    doc.lines = [l for l in doc.lines if not (isinstance(l, Other) and l.section in (S_FONTS, S_GRAPHICS))]
    for graphic in (False, True):
        sec = S_GRAPHICS if graphic else S_FONTS
        for a in atts:
            if a.graphic != graphic:
                continue
            doc.lines.append(Other(f"{'filename' if graphic else 'fontname'}: {a.name}", sec))
            for ln in uuencode(a.data).split("\r\n"):
                doc.lines.append(Other(ln, sec))
