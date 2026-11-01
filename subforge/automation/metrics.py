"""Text extents provider used by aegisub.text_extents / karaskel.

Uses Qt font metrics when available (accurate for the user's installed fonts),
else a crude proportional fallback so scripts still run headless.
"""
from __future__ import annotations

_cache: dict = {}


def _qt_metrics(fontname, size, bold, italic):
    key = (fontname, round(size, 2), bold, italic)
    fm = _cache.get(key)
    if fm is None:
        from PySide6.QtGui import QFont, QFontMetricsF, QGuiApplication
        if QGuiApplication.instance() is None:
            raise RuntimeError("no Qt app")
        f = QFont(fontname)
        f.setPixelSize(max(1, int(round(size))))
        f.setBold(bool(bold))
        f.setItalic(bool(italic))
        f.setStyleStrategy(QFont.StyleStrategy.PreferAntialias)
        f.setHintingPreference(QFont.HintingPreference.PreferNoHinting)
        fm = QFontMetricsF(f)
        _cache[key] = fm
    return fm


def text_extents(style: dict, text: str):
    """Returns (width, height, descent, external_leading) in script pixels."""
    size = float(style.get("fontsize", 48))
    sx = float(style.get("scale_x", 100)) / 100.0
    sy = float(style.get("scale_y", 100)) / 100.0
    spacing = float(style.get("spacing", 0))
    bold, italic = bool(style.get("bold")), bool(style.get("italic"))
    try:
        fm = _qt_metrics(style.get("fontname", "Arial"), size, bold, italic)
        w = fm.horizontalAdvance(text) if text else 0.0
        h = fm.height()
        d = fm.descent()
        el = fm.leading()
    except Exception:
        w = sum(size * (0.6 if ch.isupper() else 0.5 if ch != " " else 0.28) for ch in text)
        h, d, el = size * 1.2, size * 0.2, 0.0
    w = (w + spacing * len(text)) * sx
    return w, h * sy, d * sy, el * sy
