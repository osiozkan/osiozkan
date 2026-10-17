"""Arayüzü başlatır."""

from __future__ import annotations

import os
import sys


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv if argv is None else argv)
    from PySide6.QtCore import Qt
    from PySide6.QtWidgets import QApplication

    from .. import APP_NAME
    from ..config import Config
    from . import theme
    from .context import Context
    from .mainwindow import MainWindow

    from PySide6.QtCore import QObject, QTimer, Signal

    from .. import diag

    QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    app = QApplication(argv)
    app.setApplicationName(APP_NAME)
    app.setOrganizationName(APP_NAME)
    cfg = Config()
    diag.setup_logging(cfg["app_log"] or "INFO")

    class _Bridge(QObject):                      # çökme bildirimini ana iş parçacığına taşır
        crashed = Signal(str, str)
    bridge = _Bridge()
    diag.install_crash_handler(lambda path, summary: bridge.crashed.emit(path, summary))
    from .icon import app_icon
    app.setWindowIcon(app_icon())
    theme.apply(app, cfg["theme"])
    ctx = Context(cfg)
    win = MainWindow(ctx)
    bridge.crashed.connect(win.on_crash)
    files = [a for a in argv[1:] if a.lower().endswith((".ass", ".ssa", ".srt", ".vtt")) and os.path.exists(a)]
    win.show()

    def startup():
        recovered = win.check_recovery()
        if files and not recovered:
            win.open_path(files[0], force=True)
    QTimer.singleShot(0, startup)
    return app.exec()
