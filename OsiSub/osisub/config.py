"""Ayarlar ve klasör yolları.

Kullanıcı klasörü:
  Windows : %APPDATA%\\OsiSub
  diğer   : ~/.config/osisub
İçinde: config.json, automation/autoload, automation/include, log/.
"""

from __future__ import annotations

import json
import os
import sys

_APP_ROOT = os.path.dirname(os.path.abspath(__file__))

DEFAULTS = {
    "theme": "dark",                    # dark | light
    "cpl_limit": 42,
    "grid_tag_marker": "\u2600",        # ☀
    "load_aegisub_scripts": True,       # Aegisub'ın kendi autoload klasörlerini de tara
    "extra_autoload_dirs": [],
    "extra_include_dirs": [],
    "recent_files": [],
    "editbox_font_size": 13,
    "grid_font_size": 10,
    "enter_moves_next": True,
    "log_level": 4,                     # aegisub.debug.out gösterim eşiği (Aegisub varsayılanı 3)
    "window_state": "",
    "auto_load_media": True,            # .ass açınca Project Garbage'daki video/sesi aç
    "video_seek_on_line": True,         # satır değişince video satır başına gitsin
    "audio_rate": 22050,                # ses bölmesi örnekleme hızı (dinleme kalitesi / bellek)
    "timing_pp": {},                    # zamanlama son işlemcisi son ayarları
    "motion": {},                       # hareket takibi bölmesinin son ayarları
    "sync": {},                         # konuşmaya göre senkron (bsync) son ayarları
    "compare": {},                      # kaynakla karşılaştır (aligncompare) son ayarları
    "tools": {},                        # Subtitle Edit araçlarının son ayarları (araç başına)
    "multi_replace": [],                # çoklu değiştir kural grupları (boş = başlangıç kuralları)
    "batch_convert": {},                # toplu dönüştürme son ayarları
    "ttml_time": "clock",               # TTML/DFXP dışa aktarma zaman biçimi: clock | ticks
    "export_lang": "tr",                # TTML xml:lang
    "autosave_seconds": 60,             # otomatik yedek aralığı (0 = kapalı)
    "app_log": "INFO",                  # logs/osisub.log ayrıntısı (DEBUG/INFO/WARNING)
    "qc_rules": [],                     # boş = varsayılan kurallar
    "qc_min_duration": 700,
    "qc_max_duration": 7000,
    "qc_cps_max": 25,
    "qc_ellipsis": "...",
    "spell_enabled": True,
    "spell_lang": "tr_TR",
    "asr_model": "small",               # Whisper modeli
    "asr_lang": "ja",
    "scene_threshold": 10.0,            # sahne değişimi eşiği (ffmpeg scdet; düşük = daha çok kesme)
    "window_geometry": "",
    # Harici komut satırı araçları (assmotion, hairtrack, bsync ...). Örnek:
    # {"name": "assmotion: seçili satırı izle", "program": "python",
    #  "args": "C:/araclar/assmotion.py track --ass {ass} --video {video} --line {active}",
    #  "reload": true}
    "external_tools": [],
}


def app_dir() -> str:
    """osisub paketinin klasörü."""
    return _APP_ROOT


def app_data_dir() -> str:
    """Aegisub'daki ?data karşılığı: uygulama ile gelen veriler."""
    return _APP_ROOT


def user_dir() -> str:
    if sys.platform.startswith("win"):
        base = os.environ.get("APPDATA") or os.path.expanduser("~")
        d = os.path.join(base, "OsiSub")
    else:
        d = os.path.join(os.path.expanduser("~"), ".config", "osisub")
    os.makedirs(d, exist_ok=True)
    return d


def builtin_lua_dir() -> str:
    return os.path.join(_APP_ROOT, "automation", "lua")


def builtin_include_dir() -> str:
    return os.path.join(builtin_lua_dir(), "include")


def builtin_autoload_dirs() -> list[str]:
    return [os.path.join(builtin_lua_dir(), "autoload")]


def aegisub_dirs() -> tuple[list[str], list[str]]:
    """Kurulu Aegisub'ın (autoload, include) klasörleri — varsa."""
    autoload, include = [], []
    cands = []
    if sys.platform.startswith("win"):
        appdata = os.environ.get("APPDATA", "")
        if appdata:
            cands.append(os.path.join(appdata, "Aegisub"))
        for pf in (os.environ.get("ProgramFiles", r"C:\Program Files"),
                   os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")):
            cands.append(os.path.join(pf, "Aegisub"))
    else:
        cands += [os.path.expanduser("~/.aegisub"), "/usr/share/aegisub", "/usr/local/share/aegisub"]
    for c in cands:
        a = os.path.join(c, "automation", "autoload")
        i = os.path.join(c, "automation", "include")
        if os.path.isdir(a):
            autoload.append(a)
        if os.path.isdir(i):
            include.append(i)
    return autoload, include


class Config:
    def __init__(self, path: str | None = None):
        self.path = path or os.path.join(user_dir(), "config.json")
        self.data = dict(DEFAULTS)
        self.load()

    def load(self):
        try:
            with open(self.path, encoding="utf-8") as f:
                self.data.update(json.load(f))
        except (OSError, ValueError):
            pass

    def save(self):
        tmp = self.path + ".tmp"
        try:
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(self.data, f, ensure_ascii=False, indent=2)
            os.replace(tmp, self.path)
        except OSError:
            pass

    def __getitem__(self, k):
        return self.data.get(k, DEFAULTS.get(k))

    def __setitem__(self, k, v):
        self.data[k] = v

    def add_recent(self, path: str, limit: int = 15):
        rec = [p for p in self.data.get("recent_files", []) if p != path]
        rec.insert(0, path)
        self.data["recent_files"] = rec[:limit]

    def autoload_dirs(self) -> list[str]:
        dirs = builtin_autoload_dirs()
        dirs.append(os.path.join(user_dir(), "automation", "autoload"))
        if self["load_aegisub_scripts"]:
            dirs += aegisub_dirs()[0]
        dirs += list(self["extra_autoload_dirs"] or [])
        out = []
        for d in dirs:
            if d not in out:
                out.append(d)
        return out

    def include_dirs(self) -> list[str]:
        # Sıra önemli: OsiSub'ın kendi include klasörü HER ZAMAN ilk sırada,
        # çünkü Aegisub'ın aegisub/*.lua modülleri C++ FFI'ye bağlıdır.
        dirs = [builtin_include_dir(), os.path.join(user_dir(), "automation", "include")]
        if self["load_aegisub_scripts"]:
            dirs += aegisub_dirs()[1]
        dirs += list(self["extra_include_dirs"] or [])
        out = []
        for d in dirs:
            if d not in out:
                out.append(d)
        return out
