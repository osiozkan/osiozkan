"""Uygulama klasörleri — kaynak koddan çalışırken ve PyInstaller ile paketlenmiş .exe'de aynı sonuç.

  app_root()   osisub paketinin klasörü (paketli: <_internal>/osisub; Lua dosyaları buradadır)
  exe_dir()    kullanıcının gördüğü uygulama klasörü (paketli: OsiSub.exe'nin yanı;
               kaynak: depo kökü — BASLAT.bat'ın olduğu yer)
  bin_dirs()   ffmpeg / ffprobe / libmpv-2.dll aranacak klasörler (öncelik sırasıyla)
"""

from __future__ import annotations

import os
import sys

_PKG = os.path.dirname(os.path.abspath(__file__))


def frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def app_root() -> str:
    return _PKG


def exe_dir() -> str:
    if frozen():
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.path.dirname(_PKG)


def bin_dirs() -> list[str]:
    out = []
    for d in (os.path.join(exe_dir(), "bin"), exe_dir(), os.path.join(os.path.dirname(_PKG), "bin"),
              os.path.dirname(_PKG)):
        d = os.path.normpath(d)
        if d not in out:
            out.append(d)
    return out
