"""Uygulama simgesi (dosyasız: çalışma anında çizilir; paketleme için .ico'ya da yazılabilir)."""

from __future__ import annotations

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QFont, QIcon, QImage, QPainter, QPainterPath, QPixmap


def render(size: int) -> QImage:
    img = QImage(size, size, QImage.Format_ARGB32)
    img.fill(Qt.transparent)
    p = QPainter(img)
    p.setRenderHint(QPainter.Antialiasing)
    p.setRenderHint(QPainter.TextAntialiasing)
    r = QRectF(size * 0.04, size * 0.04, size * 0.92, size * 0.92)
    path = QPainterPath()
    path.addRoundedRect(r, size * 0.2, size * 0.2)
    p.fillPath(path, QColor("#2b5fd9"))
    # alt yazı çubuğu
    bar = QRectF(size * 0.16, size * 0.68, size * 0.68, size * 0.1)
    bp = QPainterPath()
    bp.addRoundedRect(bar, size * 0.04, size * 0.04)
    p.fillPath(bp, QColor("#ffd400"))
    f = QFont("Arial")
    f.setBold(True)
    f.setPixelSize(int(size * 0.46))
    p.setFont(f)
    p.setPen(QColor("#ffffff"))
    p.drawText(QRectF(0, size * 0.06, size, size * 0.62), Qt.AlignCenter, "Os")
    p.end()
    return img


def app_icon() -> QIcon:
    ic = QIcon()
    for s in (16, 24, 32, 48, 64, 128, 256):
        ic.addPixmap(QPixmap.fromImage(render(s)))
    return ic


def write_ico(path: str) -> bool:
    """Windows .ico (256 px; Qt'nin ico eklentisi). Paketleme betiği kullanır."""
    return render(256).save(path, "ICO") or render(256).save(path[:-4] + ".png", "PNG")
