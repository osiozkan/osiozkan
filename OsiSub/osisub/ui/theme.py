"""Koyu / açık tema. Fusion stili + palet + küçük bir stil sayfası."""

from __future__ import annotations

from PySide6.QtGui import QColor, QPalette
from PySide6.QtWidgets import QApplication

DARK = {
    "window": "#1e1f22", "base": "#16171a", "alt": "#1b1c20", "text": "#e3e3e6",
    "button": "#2a2b30", "highlight": "#2f6feb", "hl_text": "#ffffff",
    "mid": "#3a3b41", "muted": "#9a9ba3", "border": "#34353b",
    "comment_bg": "#25262b", "comment_fg": "#8b8c94",
    "warn_bg": "#5a4410", "warn_fg": "#ffd27a",
    "err_bg": "#5c1d1d", "err_fg": "#ff9b9b",
    "ok_fg": "#8fd18f", "active_bg": "#24324d",
    "tag": "#c792ea", "tag_brace": "#7f8c98", "linebreak": "#ffcb6b", "drawing": "#80cbc4",
}

LIGHT = {
    "window": "#f3f3f5", "base": "#ffffff", "alt": "#f6f7f9", "text": "#1d1e22",
    "button": "#e8e9ec", "highlight": "#2f6feb", "hl_text": "#ffffff",
    "mid": "#c9cad0", "muted": "#6b6c73", "border": "#d4d5da",
    "comment_bg": "#eceef2", "comment_fg": "#7a7b82",
    "warn_bg": "#fff1cc", "warn_fg": "#7a5300",
    "err_bg": "#ffd9d9", "err_fg": "#8a1010",
    "ok_fg": "#1f7a1f", "active_bg": "#dbe6ff",
    "tag": "#8e24aa", "tag_brace": "#7a7f87", "linebreak": "#b26a00", "drawing": "#00796b",
}

_current = dict(DARK)


def colors() -> dict:
    return _current


def apply(app: QApplication, name: str = "dark"):
    global _current
    c = DARK if name != "light" else LIGHT
    _current = dict(c)
    app.setStyle("Fusion")
    pal = QPalette()
    pal.setColor(QPalette.Window, QColor(c["window"]))
    pal.setColor(QPalette.WindowText, QColor(c["text"]))
    pal.setColor(QPalette.Base, QColor(c["base"]))
    pal.setColor(QPalette.AlternateBase, QColor(c["alt"]))
    pal.setColor(QPalette.Text, QColor(c["text"]))
    pal.setColor(QPalette.Button, QColor(c["button"]))
    pal.setColor(QPalette.ButtonText, QColor(c["text"]))
    pal.setColor(QPalette.Highlight, QColor(c["highlight"]))
    pal.setColor(QPalette.HighlightedText, QColor(c["hl_text"]))
    pal.setColor(QPalette.ToolTipBase, QColor(c["button"]))
    pal.setColor(QPalette.ToolTipText, QColor(c["text"]))
    pal.setColor(QPalette.PlaceholderText, QColor(c["muted"]))
    pal.setColor(QPalette.Mid, QColor(c["mid"]))
    pal.setColor(QPalette.Disabled, QPalette.Text, QColor(c["muted"]))
    pal.setColor(QPalette.Disabled, QPalette.ButtonText, QColor(c["muted"]))
    pal.setColor(QPalette.Disabled, QPalette.WindowText, QColor(c["muted"]))
    app.setPalette(pal)
    app.setStyleSheet(f"""
        QToolTip {{ border: 1px solid {c['border']}; padding: 4px; }}
        QHeaderView::section {{ background: {c['button']}; border: 0; border-right: 1px solid {c['border']};
                                border-bottom: 1px solid {c['border']}; padding: 3px 6px; }}
        QTableView {{ gridline-color: {c['border']}; }}
        QLineEdit, QPlainTextEdit, QSpinBox, QDoubleSpinBox, QComboBox {{
            border: 1px solid {c['border']}; border-radius: 4px; padding: 2px 4px; }}
        QLineEdit:focus, QPlainTextEdit:focus, QSpinBox:focus, QComboBox:focus {{
            border: 1px solid {c['highlight']}; }}
        QLabel#cplBox {{ border: 1px solid {c['border']}; border-radius: 4px; padding: 2px 8px;
                         font-weight: 600; min-width: 34px; qproperty-alignment: AlignCenter; }}
        QDockWidget::title {{ padding: 4px; background: {c['button']}; }}
        QStatusBar {{ border-top: 1px solid {c['border']}; }}
    """)
