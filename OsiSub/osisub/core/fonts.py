"""Yazı tipi toplayıcı (Aegisub Fonts Collector) + eksik karakter denetimi.

  used_fonts(doc)        Belgede kullanılan yazı tipleri: satırın stilinden başlayıp override bloklarındaki
                         \\fn, \\b, \\i, \\r değişimleri izlenir; her (ad, kalın, italik) için kullanılan
                         karakterler ve satırlar toplanır. Çizimler (\\p) ve yorum satırları sayılmaz.
  FontIndex              Sistem (Windows: Windows\\Fonts + kullanıcı klasörü; Linux/mac: olağan klasörler),
                         belgenin klasörü ve fonts/ alt klasörü taranır. TTF/OTF/TTC dosyalarının 'name'
                         tablosundan aile/tam/PostScript adları, OS/2 ve head'den kalın/italik, 'cmap'ten
                         desteklenen karakterler okunur (saf Python; ek paket gerekmez). Önbellekli.
  collect(doc, index)    Her yazı tipi için bulunan dosya, sentetik kalın/italik uyarısı ve EKSİK KARAKTERLER
                         (Türkçe ğ ş ı İ gibi — birçok anime yazı tipinde yoktur).

libass gibi: ad büyük/küçük harfe duyarsız aile, tam ad ya da PostScript adıyla eşleşir; baştaki '@'
(dikey yazı) yok sayılır. Kalın/italik yüz yoksa normal yüz kullanılır (libass sentezler) ve uyarılır.
"""

from __future__ import annotations

import json
import os
import re
import struct
import sys
from dataclasses import dataclass, field

FONT_EXT = (".ttf", ".otf", ".ttc", ".otc")


# ---------------------------------------------------------------------------
# Belgede kullanılan yazı tipleri
# ---------------------------------------------------------------------------

@dataclass
class FontUse:
    name: str
    bold: bool
    italic: bool
    chars: set = field(default_factory=set)
    rows: set = field(default_factory=set)
    styles: set = field(default_factory=set)


def _norm_name(n: str) -> str:
    return n.strip().lstrip("@").strip().casefold()


def used_fonts(doc) -> dict[tuple, FontUse]:
    styles = {s.name: s for s in doc.styles}
    default = styles.get("Default")
    out: dict[tuple, FontUse] = {}

    def use(name, bold, italic, row, style, chars):
        key = (_norm_name(name), bool(bold), bool(italic))
        fu = out.get(key)
        if fu is None:
            fu = out[key] = FontUse(name.strip().lstrip("@"), bool(bold), bool(italic))
        fu.rows.add(row)
        fu.styles.add(style)
        fu.chars.update(chars)

    for row, ev in enumerate(doc.events):
        if ev.comment:
            continue
        st = styles.get(ev.style) or default
        if st is None:
            continue

        def reset(sname=None):
            s = styles.get(sname) if sname else st
            s = s or st
            return {"fn": s.fontname, "b": s.bold, "i": s.italic}
        cur = reset()
        drawing = False
        pos = 0
        text = ev.text
        for m in list(re.finditer(r"\{([^}]*)\}", text)) + [None]:
            seg = text[pos:m.start()] if m else text[pos:]
            if seg and not drawing:
                vis = seg.replace("\\N", "").replace("\\n", "").replace("\\h", "\u00a0")
                chars = {c for c in vis if c not in "\r\n\t"}
                if chars:
                    use(cur["fn"], cur["b"], cur["i"], row, ev.style, chars)
            if m is None:
                break
            inner = m.group(1)
            for tm in re.finditer(r"\\(fn|b|i|r|p)([^\\]*)", inner):
                tag, arg = tm.group(1), tm.group(2).strip()
                if tag == "fn":
                    cur["fn"] = arg or reset()["fn"]
                elif tag == "b":
                    if arg.isdigit():
                        v = int(arg)
                        cur["b"] = v == 1 or v >= 700
                    elif not arg:
                        cur["b"] = reset()["b"]
                elif tag == "i":
                    if arg in ("0", "1"):
                        cur["i"] = arg == "1"
                    elif not arg:
                        cur["i"] = reset()["i"]
                elif tag == "r":
                    cur = reset(arg or None)
                elif tag == "p" and arg.isdigit():
                    drawing = int(arg) > 0
            pos = m.end()
        # stil satırda hiç görünür karakter olmasa da (ör. yalnız etiket) yazı tipi kaydedilmez
    return out


# ---------------------------------------------------------------------------
# Yazı tipi dosyası okuma (sfnt: TTF/OTF/TTC)
# ---------------------------------------------------------------------------

@dataclass
class FontFace:
    path: str
    index: int
    names: list            # aile, tam, PostScript (küçük harf)
    family: str
    bold: bool
    italic: bool
    cmap: object = None    # aralık listesi [(ilk, son), ...]
    weight: int = 400

    def has(self, cp: int) -> bool:
        import bisect
        rngs = self.cmap or []
        i = bisect.bisect_right(rngs, (cp, float("inf"))) - 1
        return i >= 0 and rngs[i][0] <= cp <= rngs[i][1]


def _tables(f, off: int) -> dict:
    f.seek(off)
    head = f.read(12)
    num = struct.unpack_from(">H", head, 4)[0]
    recs = f.read(16 * num)
    t = {}
    for k in range(num):
        tag, _ck, o, ln = struct.unpack_from(">4sIII", recs, 16 * k)
        t[tag.decode("latin-1")] = (o, ln)
    return t


def _read(f, tab) -> bytes:
    f.seek(tab[0])
    return f.read(tab[1])


def _names(data: bytes) -> dict[int, list[str]]:
    _fmt, count, str_off = struct.unpack_from(">HHH", data, 0)
    out: dict[int, list[str]] = {}
    for k in range(count):
        pid, eid, _lang, nid, length, offset = struct.unpack_from(">HHHHHH", data, 6 + 12 * k)
        if nid not in (1, 2, 4, 6, 16, 17):
            continue
        raw = data[str_off + offset: str_off + offset + length]
        try:
            if pid in (0, 3):
                s = raw.decode("utf-16-be")
            elif pid == 1 and eid == 0:
                s = raw.decode("mac_roman")
            else:
                continue
        except UnicodeDecodeError:
            continue
        s = s.strip("\x00").strip()
        if s:
            out.setdefault(nid, [])
            if s not in out[nid]:
                out[nid].append(s)
    return out


def _cmap(data: bytes) -> list[tuple[int, int]]:
    _ver, n = struct.unpack_from(">HH", data, 0)
    best = None
    for k in range(n):
        pid, eid, sub = struct.unpack_from(">HHI", data, 4 + 8 * k)
        fmt = struct.unpack_from(">H", data, sub)[0]
        score = {(3, 10): 4, (0, 4): 4, (0, 6): 4, (3, 1): 3, (0, 3): 3, (0, 2): 2, (0, 1): 2, (0, 0): 2}.get(
            (pid, eid), 0)
        if fmt in (4, 12) and score and (best is None or score > best[0]):
            best = (score, sub, fmt)
    if best is None:
        return []
    _s, so, fmt = best
    rngs = []
    if fmt == 4:
        segx2 = struct.unpack_from(">H", data, so + 6)[0]
        seg = segx2 // 2
        ends = struct.unpack_from(f">{seg}H", data, so + 14)
        starts = struct.unpack_from(f">{seg}H", data, so + 16 + segx2)
        deltas = struct.unpack_from(f">{seg}h", data, so + 16 + 2 * segx2)
        roffs_at = so + 16 + 3 * segx2
        roffs = struct.unpack_from(f">{seg}H", data, roffs_at)
        for i in range(seg):
            a, b = starts[i], ends[i]
            if a == 0xFFFF:
                continue
            if roffs[i] == 0:
                rngs.append((a, b))          # glif = (c + delta) & 0xFFFF — aralık dolu
                continue
            run = None                       # glyphIdArray üzerinden: glifi 0 olanları ayıkla
            for c in range(a, b + 1):
                gi_at = roffs_at + 2 * i + roffs[i] + 2 * (c - a)
                if gi_at + 2 > len(data):
                    break
                g = struct.unpack_from(">H", data, gi_at)[0]
                ok = g != 0 and ((g + deltas[i]) & 0xFFFF) != 0
                if ok:
                    run = (run[0], c) if run else (c, c)
                elif run:
                    rngs.append(run)
                    run = None
            if run:
                rngs.append(run)
    else:
        n_groups = struct.unpack_from(">I", data, so + 12)[0]
        for k in range(n_groups):
            a, b, _g = struct.unpack_from(">III", data, so + 16 + 12 * k)
            rngs.append((a, b))
    rngs.sort()
    merged = []
    for a, b in rngs:
        if merged and a <= merged[-1][1] + 1:
            merged[-1] = (merged[-1][0], max(merged[-1][1], b))
        else:
            merged.append((a, b))
    return merged


def read_faces(path: str, with_cmap: bool = True) -> list[FontFace]:
    """Yalnız gereken tablolar okunur (büyük CJK dosyalarında da hızlı)."""
    faces = []
    with open(path, "rb") as f:
        tag = f.read(12)
        offs = [0]
        if tag[:4] == b"ttcf":
            n = struct.unpack_from(">I", tag, 8)[0]
            offs = list(struct.unpack(f">{n}I", f.read(4 * n)))
        for idx, off in enumerate(offs):
            try:
                t = _tables(f, off)
                nm = _names(_read(f, t["name"])) if "name" in t else {}
                fam = (nm.get(16) or nm.get(1) or [os.path.splitext(os.path.basename(path))[0]])[0]
                names = []
                for nid in (1, 4, 6, 16):
                    for s in nm.get(nid, []):
                        if s.casefold() not in names:
                            names.append(s.casefold())
                bold = italic = False
                weight = 400
                if "OS/2" in t:
                    os2 = _read(f, t["OS/2"])
                    weight = struct.unpack_from(">H", os2, 4)[0]
                    fs_sel = struct.unpack_from(">H", os2, 62)[0]
                    bold = weight >= 600 or bool(fs_sel & 0x20)
                    italic = bool(fs_sel & 0x01)
                elif "head" in t:
                    mac = struct.unpack_from(">H", _read(f, t["head"]), 44)[0]
                    bold, italic = bool(mac & 1), bool(mac & 2)
                sub = " ".join((nm.get(17) or nm.get(2) or [""])).lower()
                bold = bold or "bold" in sub
                italic = italic or "italic" in sub or "oblique" in sub
                cm = _cmap(_read(f, t["cmap"])) if (with_cmap and "cmap" in t) else None
                if bold and weight < 600:
                    weight = 700
                faces.append(FontFace(path, idx, names, fam, bold, italic, cm, weight))
            except (struct.error, KeyError, IndexError):
                continue
    return faces


# ---------------------------------------------------------------------------
# Dizin (önbellekli)
# ---------------------------------------------------------------------------

def system_font_dirs() -> list[str]:
    dirs = []
    if sys.platform.startswith("win"):
        win = os.environ.get("WINDIR", r"C:\Windows")
        dirs.append(os.path.join(win, "Fonts"))
        la = os.environ.get("LOCALAPPDATA")
        if la:
            dirs.append(os.path.join(la, "Microsoft", "Windows", "Fonts"))
    elif sys.platform == "darwin":
        dirs += ["/System/Library/Fonts", "/Library/Fonts", os.path.expanduser("~/Library/Fonts")]
    else:
        dirs += ["/usr/share/fonts", "/usr/local/share/fonts", os.path.expanduser("~/.fonts"),
                 os.path.expanduser("~/.local/share/fonts")]
    return [d for d in dirs if os.path.isdir(d)]


class FontIndex:
    """dirs: alt klasörleriyle taranır; shallow: yalnız klasörün kendisi (ör. belgenin klasörü)."""

    def __init__(self, dirs: list[str], cache_path: str | None = None, progress=None, shallow=()):
        self.faces: list[FontFace] = []
        self.by_name: dict[str, list[FontFace]] = {}
        cache = {}
        if cache_path and os.path.exists(cache_path):
            try:
                with open(cache_path, encoding="utf-8") as f:
                    cache = json.load(f)
            except (OSError, ValueError):
                cache = {}
        files = []
        for d in dirs:
            for root, _dirs, fs in os.walk(d):
                for fn in fs:
                    if fn.lower().endswith(FONT_EXT):
                        files.append(os.path.join(root, fn))
        for d in shallow:
            try:
                files += [os.path.join(d, fn) for fn in os.listdir(d) if fn.lower().endswith(FONT_EXT)]
            except OSError:
                pass
        new_cache = {}
        for k, p in enumerate(files):
            try:
                stt = os.stat(p)
            except OSError:
                continue
            key = f"{p}|{stt.st_size}|{int(stt.st_mtime)}"
            hit = cache.get(key)
            if hit is None:
                try:
                    hit = [_face_to_json(fc) for fc in read_faces(p)]
                except (OSError, struct.error, ValueError):
                    hit = []
            new_cache[key] = hit
            for d in hit:
                fc = _face_from_json(d)
                self.faces.append(fc)
                for n in fc.names:
                    self.by_name.setdefault(n, []).append(fc)
            if progress and k % 50 == 0:
                progress(k / max(1, len(files)))
        if cache_path and new_cache != cache:
            try:
                os.makedirs(os.path.dirname(cache_path), exist_ok=True)
                with open(cache_path, "w", encoding="utf-8") as f:
                    json.dump(new_cache, f)
            except OSError:
                pass

    def find(self, name: str, bold: bool, italic: bool) -> tuple[FontFace | None, list[str]]:
        """En uygun yüz + uyarılar (sentetik kalın/italik)."""
        cands = self.by_name.get(_norm_name(name), [])
        if not cands:
            return None, []
        target = 700 if bold else 400            # aynı ailede Thin/Black gibi yüzler varken en yakın kalınlık
        pick = sorted(cands, key=lambda f: (f.italic != italic, f.bold != bold, abs(f.weight - target),
                                            f.path, f.index))[0]
        warns = []
        if bold and not pick.bold:
            warns.append("kalın yüz yok (sentetik kalın)")
        if italic and not pick.italic:
            warns.append("italik yüz yok (sentetik italik)")
        return pick, warns


def _face_to_json(f: FontFace) -> dict:
    return {"p": f.path, "i": f.index, "n": f.names, "f": f.family, "b": f.bold, "it": f.italic,
            "c": f.cmap or [], "w": f.weight}


def _face_from_json(d: dict) -> FontFace:
    return FontFace(d["p"], d["i"], d["n"], d["f"], d["b"], d["it"], [tuple(x) for x in d["c"]],
                    int(d.get("w", 700 if d["b"] else 400)))


# ---------------------------------------------------------------------------
# Toplama
# ---------------------------------------------------------------------------

@dataclass
class FontReport:
    use: FontUse
    face: FontFace | None
    warnings: list
    missing: list            # bu yazı tipinde olmayan karakterler

    @property
    def ok(self) -> bool:
        return self.face is not None and not self.missing


def collect(doc, index: FontIndex) -> list[FontReport]:
    out = []
    for key, fu in sorted(used_fonts(doc).items(), key=lambda kv: (kv[1].name.casefold(), kv[0][1], kv[0][2])):
        face, warns = index.find(fu.name, fu.bold, fu.italic)
        missing = []
        if face is not None and face.cmap:
            missing = sorted(c for c in fu.chars if not c.isspace() and not face.has(ord(c)))
        out.append(FontReport(fu, face, warns, missing))
    return out


def copy_fonts(reports: list[FontReport], dest: str) -> list[str]:
    import shutil
    os.makedirs(dest, exist_ok=True)
    done = []
    for p in sorted({r.face.path for r in reports if r.face is not None}):
        target = os.path.join(dest, os.path.basename(p))
        if os.path.abspath(target) != os.path.abspath(p):
            shutil.copy2(p, target)
        done.append(target)
    return done


def zip_fonts(reports: list[FontReport], zip_path: str) -> list[str]:
    import zipfile
    paths = sorted({r.face.path for r in reports if r.face is not None})
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for p in paths:
            z.write(p, os.path.basename(p))
    return paths


def doc_font_dirs(doc) -> tuple[list[str], list[str]]:
    """(alt klasörleriyle taranacaklar, yalnız kendisi taranacaklar): belgenin fonts/Fonts/attachments
    alt klasörleri ve belgenin klasörü (masaüstü gibi büyük bir klasör olabilir — alt klasörlerine inilmez)."""
    if not doc.path:
        return [], []
    base = os.path.dirname(os.path.abspath(doc.path))
    deep = [d for d in (os.path.join(base, "fonts"), os.path.join(base, "Fonts"), os.path.join(base, "attachments"))
            if os.path.isdir(d)]
    return deep, [base]


__all__ = ["used_fonts", "FontUse", "FontFace", "read_faces", "FontIndex", "system_font_dirs", "collect",
           "FontReport", "copy_fonts", "zip_fonts", "doc_font_dirs"]
