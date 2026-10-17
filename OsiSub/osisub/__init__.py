"""OsiSub — Aegisub yerine geçmek üzere yazılan altyazı düzenleyici.

Katmanlar:
  osisub.core        bayt korumalı ASS modeli, zaman, CPL/CPS, geri al
  osisub.automation  Aegisub Automation 4 uyumlu Lua çalışma zamanı (lupa + LuaJIT)
  osisub.media       kare dizini, ses çözme, tepe zarfı, spektrogram, kare çözme (Qt'siz)
  osisub.ui          PySide6 arayüzü
"""

__version__ = "1.3.0"
APP_NAME = "OsiSub"
