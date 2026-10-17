"""Medya önbelleği. Anahtar: tam yol + boyut + değiştirilme zamanı; kaynak
değişince kendiliğinden geçersiz olur. Videonun yanına dosya yazılmaz
(salt okunur klasörler / ağ sürücüleri sorun çıkarmasın diye)."""

from __future__ import annotations

import hashlib
import os
import shutil
import sys


def cache_root() -> str:
    if sys.platform.startswith("win"):
        base = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA") or os.path.expanduser("~")
        d = os.path.join(base, "OsiSub", "cache")
    else:
        d = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache"), "osisub")
    os.makedirs(d, exist_ok=True)
    return d


def key_for(path: str, *extra) -> str:
    st = os.stat(path)
    raw = f"{os.path.abspath(path)}|{st.st_size}|{int(st.st_mtime_ns)}|" + "|".join(map(str, extra))
    return hashlib.sha1(raw.encode("utf-8", "surrogateescape")).hexdigest()[:20]


def entry_dir(path: str) -> str:
    d = os.path.join(cache_root(), key_for(path))
    os.makedirs(d, exist_ok=True)
    return d


def clear() -> int:
    root = cache_root()
    n = 0
    for name in os.listdir(root):
        p = os.path.join(root, name)
        if os.path.isdir(p):
            shutil.rmtree(p, ignore_errors=True)
            n += 1
    return n
