"""packaging/osisub.ico üretir (PyInstaller simgesi). Kullanım: python tools/make_icon.py [çıktı.ico]"""
import os
import sys

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from PySide6.QtWidgets import QApplication  # noqa: E402

from osisub.ui.icon import write_ico  # noqa: E402

app = QApplication(sys.argv[:1])
out = sys.argv[1] if len(sys.argv) > 1 else os.path.join("packaging", "osisub.ico")
os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
print("simge:", out, "tamam" if write_ico(out) else "YAZILAMADI")
