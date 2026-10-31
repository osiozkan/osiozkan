from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QFont, QFontMetricsF, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import QWidget

from ..core.tags import strip_tags


def ass_color(s: str) -> QColor:
    h = s.strip().lstrip("&Hh").rstrip("&")
    h = h.rjust(8, "0")
    a, b, g, r = (int(h[i:i + 2], 16) for i in (0, 2, 4, 6))
    return QColor(r, g, b, 255 - a)


class Preview(QWidget):
    """Basit altyazı önizlemesi (stil, hizalama, kenarlık). Video/render entegrasyonu yol haritasında."""

    def __init__(self):
        super().__init__()
        self.setMinimumHeight(160)
        self.doc = None
        self.line = None

    def show_line(self, doc, line):
        self.doc, self.line = doc, line
        self.update()

    def paintEvent(self, _):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.fillRect(self.rect(), QColor("#101418"))
        if not self.doc or not self.line:
            return
        rx, ry = self.doc.res
        sc = min(self.width() / rx, self.height() / ry)
        ox, oy = (self.width() - rx * sc) / 2, (self.height() - ry * sc) / 2
        p.fillRect(QRectF(ox, oy, rx * sc, ry * sc), QColor("#1b2229"))
        st = self.doc.style(self.line.style) or self.doc.styles[0]
        f = QFont(st.fontname); f.setPixelSize(max(4, int(st.fontsize * sc))); f.setBold(st.bold); f.setItalic(st.italic)
        p.setFont(f)
        fm = QFontMetricsF(f)
        rows = strip_tags(self.line.text).replace("\\N", "\n").replace("\\n", "\n").split("\n")
        mv = (self.line.margin_t or st.margin_t) * sc
        ml = (self.line.margin_l or st.margin_l) * sc
        mr = (self.line.margin_r or st.margin_r) * sc
        lh = fm.height()
        al = st.align
        total = lh * len(rows)
        row_pos = (al - 1) // 3
        y0 = {0: oy + ry * sc - mv - total, 1: oy + (ry * sc - total) / 2, 2: oy + mv}[row_pos]
        for i, r in enumerate(rows):
            w = fm.horizontalAdvance(r)
            col = (al - 1) % 3
            x = {0: ox + ml, 1: ox + (rx * sc - w) / 2, 2: ox + rx * sc - mr - w}[col]
            path = QPainterPath(); path.addText(x, y0 + i * lh + fm.ascent(), f, r)
            if st.outline > 0:
                p.setPen(QPen(ass_color(st.outline_c), max(1.0, st.outline * sc * 2), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                p.drawPath(path)
            p.setPen(Qt.NoPen); p.setBrush(ass_color(st.primary)); p.drawPath(path)
