# -*- mode: python -*-
# PyInstaller tarifi — BUILD_EXE.bat çalıştırır:  py -3 -m PyInstaller --noconfirm --clean packaging/osisub.spec
#
# Çıktı: dist/OsiSub/  (tek klasör; içinde OsiSub.exe = pencereli, OsiSub-konsol.exe = doctor/komut satırı)
# Lua dosyaları (Aegisub include/autoload), yazım sözlükleri hariç tüm veriler pakete girer.
# ffmpeg/ffprobe/libmpv-2.dll pakete GÖMÜLMEZ: dist/OsiSub/bin/ klasörüne kopyalanır (BUILD_EXE.bat).
# Whisper (faster-whisper/ctranslate2) bilerek dışarıda: ~1 GB; kaynak kurulumda isteğe bağlı kalır.
import os

from PyInstaller.utils.hooks import collect_data_files, collect_submodules

ROOT = os.path.abspath(os.path.join(SPECPATH, ".."))
ICON = os.path.join(SPECPATH, "osisub.ico")

datas = collect_data_files("osisub", include_py_files=False)
datas += collect_data_files("spylls")
hidden = (collect_submodules("lupa") + collect_submodules("osisub") +
          ["PySide6.QtMultimedia", "mpv", "av", "cv2", "regex", "spylls", "numpy"])

a = Analysis(
    [os.path.join(SPECPATH, "launcher.py")],
    pathex=[ROOT],
    binaries=[],
    datas=datas,
    hiddenimports=hidden,
    hookspath=[],
    runtime_hooks=[],
    excludes=["tkinter", "matplotlib", "scipy", "faster_whisper", "ctranslate2", "torch", "IPython",
              "PySide6.QtWebEngineCore", "PySide6.QtWebEngineWidgets", "PySide6.Qt3DCore", "PySide6.QtQuick3D"],
    noarchive=False,
)
pyz = PYZ(a.pure)

exe_gui = EXE(pyz, a.scripts, [], exclude_binaries=True, name="OsiSub", console=False,
              icon=ICON if os.path.exists(ICON) else None, upx=False)
exe_cli = EXE(pyz, a.scripts, [], exclude_binaries=True, name="OsiSub-konsol", console=True,
              icon=ICON if os.path.exists(ICON) else None, upx=False)
coll = COLLECT(exe_gui, exe_cli, a.binaries, a.datas, strip=False, upx=False, name="OsiSub")
