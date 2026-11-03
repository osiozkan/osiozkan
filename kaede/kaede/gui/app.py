from __future__ import annotations
import sys

from PySide6.QtGui import QPalette, QColor
from PySide6.QtWidgets import QApplication


def apply_theme(app: QApplication, dark: bool):
    app.setStyle("Fusion")
    p = QPalette()
    if dark:
        bg, base, text, hl = QColor("#1e2127"), QColor("#16181d"), QColor("#d7dae0"), QColor("#3d7eff")
        p.setColor(QPalette.Window, bg); p.setColor(QPalette.WindowText, text); p.setColor(QPalette.Base, base)
        p.setColor(QPalette.AlternateBase, QColor("#1b1e24")); p.setColor(QPalette.Text, text)
        p.setColor(QPalette.Button, QColor("#2a2e36")); p.setColor(QPalette.ButtonText, text)
        p.setColor(QPalette.Highlight, hl); p.setColor(QPalette.HighlightedText, QColor("#fff"))
        p.setColor(QPalette.ToolTipBase, QColor("#2a2e36")); p.setColor(QPalette.ToolTipText, text)
        p.setColor(QPalette.PlaceholderText, QColor("#7a808c"))
        p.setColor(QPalette.Disabled, QPalette.Text, QColor("#6b7080")); p.setColor(QPalette.Disabled, QPalette.ButtonText, QColor("#6b7080"))
        p.setColor(QPalette.Disabled, QPalette.WindowText, QColor("#6b7080"))
        app.setPalette(p)
    else:
        app.setPalette(app.style().standardPalette())


def main(argv=None):
    argv = list(sys.argv if argv is None else argv)
    app = QApplication(argv)
    app.setApplicationName("Kaede")
    from .main_window import MainWindow
    path = argv[1] if len(argv) > 1 else None
    w = MainWindow(path)
    w.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
