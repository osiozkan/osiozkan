"""aegisub.text_extents karşılığı.

Aegisub Windows'ta GDI kullanır: LOGFONT.lfHeight = fontsize*64 (pozitif =
HÜCRE yüksekliği = ascent+descent), karakter aralığı varsa her harf ayrı
ölçülüp toplanır, en sonda her şey 64'e bölünüp ScaleX/ScaleY uygulanır.
Burada Qt ile aynı tanım kurulur:

  * Yazı tipi önce büyük bir piksel boyunda ölçülür, ascent+descent oranından
    "hücre = fontsize" olacak em boyu bulunur (GDI pozitif lfHeight davranışı).
  * Kerning kapalı (GDI GetTextExtentPoint32 kerning uygulamaz).
  * Ölçüm 64 kat büyük yapılır; ipucu (hinting) kaynaklı yuvarlama etkisi azalır.

Sınır: Qt Windows'ta DirectWrite metrikleri döndürebilir; usWinAscent ile
hhea farkı olan yazı tiplerinde birkaç piksel sapma olabilir. Bu fark
tests/test_automation.py içinde bilinen yazı tiplerinde ölçülüp izlenir.
"""

from __future__ import annotations

_font_cache: dict = {}


def _ensure_gui():
    from PySide6.QtGui import QGuiApplication
    if QGuiApplication.instance() is None:
        import os
        import sys
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        # QApplication (QGuiApplication değil): sonradan aynı süreçte pencere açılabilsin
        from PySide6.QtWidgets import QApplication
        _ensure_gui.app = QApplication(sys.argv[:1])  # yaşasın diye referans


def _font_for(fontname: str, bold: bool, italic: bool, underline: bool,
              strikeout: bool, fontsize: float):
    from PySide6.QtGui import QFont, QFontMetricsF
    key = (fontname, bold, italic, underline, strikeout, fontsize)
    hit = _font_cache.get(key)
    if hit is not None:
        return hit
    target_cell = fontsize * 64.0
    probe = QFont(fontname)
    probe.setBold(bold)
    probe.setItalic(italic)
    probe.setKerning(False)
    probe.setPixelSize(1000)
    pm = QFontMetricsF(probe)
    cell = pm.ascent() + pm.descent()
    ratio = cell / 1000.0 if cell > 0 else 1.0
    em = max(1, int(round(target_cell / ratio)))
    f = QFont(fontname)
    f.setBold(bold)
    f.setItalic(italic)
    f.setUnderline(underline)
    f.setStrikeOut(strikeout)
    f.setKerning(False)
    f.setPixelSize(em)
    fm = QFontMetricsF(f)
    res = (f, fm)
    _font_cache[key] = res
    return res


def text_extents(style: dict, text: str) -> tuple[float, float, float, float]:
    _ensure_gui()
    fontname = str(style.get("fontname", "Arial"))
    fontsize = float(style.get("fontsize", 20) or 0)
    if fontsize <= 0:
        return 0.0, 0.0, 0.0, 0.0
    spacing = float(style.get("spacing", 0) or 0) * 64.0
    scale_x = float(style.get("scale_x", 100) or 0)
    scale_y = float(style.get("scale_y", 100) or 0)
    _, fm = _font_for(fontname, bool(style.get("bold")), bool(style.get("italic")),
                      bool(style.get("underline")), bool(style.get("strikeout")), fontsize)
    height = fm.ascent() + fm.descent()
    if spacing != 0:
        width = 0.0
        for ch in text:
            width += fm.horizontalAdvance(ch) + spacing
    else:
        width = fm.horizontalAdvance(text)
    descent = fm.descent()
    extlead = max(0.0, fm.leading())
    return (scale_x / 100.0 * width / 64.0,
            scale_y / 100.0 * height / 64.0,
            scale_y / 100.0 * descent / 64.0,
            scale_y / 100.0 * extlead / 64.0)
