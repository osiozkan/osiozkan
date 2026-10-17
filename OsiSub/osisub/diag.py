"""Günlük (log) ve çökme raporu.

  setup_logging()      <kullanıcı klasörü>/logs/osisub.log (1 MB × 3 döner dosya) + Qt iletileri
  install_crash_handler(on_crash)
                       yakalanmamış her istisna (ana iş parçacığı ve arka plan iş parçacıkları):
                       logs/cokme-YYYYAAGG-SSDDss.txt raporu yazılır (sürüm, platform, iz),
                       on_crash(rapor_yolu, özet) çağrılır — arayüz acil kayıt yapar ve kullanıcıya
                       gösterir. Program kapanmaz; kullanıcı kaydedip devam edebilir.
"""

from __future__ import annotations

import datetime
import logging
import logging.handlers
import os
import platform
import sys
import threading
import traceback

log = logging.getLogger("osisub")
_crash_cb = None
_installed = False


def logs_dir() -> str:
    from .config import user_dir
    d = os.path.join(user_dir(), "logs")
    os.makedirs(d, exist_ok=True)
    return d


def setup_logging(level: str = "INFO") -> str:
    path = os.path.join(logs_dir(), "osisub.log")
    root = logging.getLogger("osisub")
    if not any(isinstance(h, logging.handlers.RotatingFileHandler) for h in root.handlers):
        h = logging.handlers.RotatingFileHandler(path, maxBytes=1_000_000, backupCount=3, encoding="utf-8")
        h.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(name)s: %(message)s"))
        root.addHandler(h)
    root.setLevel(getattr(logging, str(level).upper(), logging.INFO))
    try:
        from PySide6.QtCore import QtMsgType, qInstallMessageHandler

        def qt_handler(mode, _ctx, msg):
            if "propagateSizeHints" in msg:
                return
            lvl = {QtMsgType.QtDebugMsg: logging.DEBUG, QtMsgType.QtInfoMsg: logging.INFO,
                   QtMsgType.QtWarningMsg: logging.WARNING}.get(mode, logging.ERROR)
            logging.getLogger("osisub.qt").log(lvl, msg)
        qInstallMessageHandler(qt_handler)
    except Exception:  # noqa: BLE001
        pass
    from . import __version__
    log.info("OsiSub %s başladı · Python %s · %s", __version__, platform.python_version(), platform.platform())
    return path


def write_report(exc_type, exc, tb, where: str = "") -> str:
    from . import __version__
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    path = os.path.join(logs_dir(), f"cokme-{stamp}.txt")
    n = 2
    while os.path.exists(path):                    # aynı saniyede ikinci hata öncekini ezmesin
        path = os.path.join(logs_dir(), f"cokme-{stamp}-{n}.txt")
        n += 1
    text = "".join(traceback.format_exception(exc_type, exc, tb))
    body = (f"OsiSub {__version__}\nPython {platform.python_version()} ({sys.executable})\n"
            f"{platform.platform()}\nYer: {where or 'ana iş parçacığı'}\n"
            f"Zaman: {datetime.datetime.now().isoformat(timespec='seconds')}\n\n{text}")
    try:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(body)
    except OSError:
        pass
    log.error("Yakalanmamış hata (%s): %s", where or "ana", text)
    return path


def _handle(exc_type, exc, tb, where=""):
    if issubclass(exc_type, (KeyboardInterrupt, SystemExit)):
        sys.__excepthook__(exc_type, exc, tb)
        return
    path = write_report(exc_type, exc, tb, where)
    summary = f"{exc_type.__name__}: {exc}"
    try:
        sys.__stderr__.write("".join(traceback.format_exception(exc_type, exc, tb)))
    except Exception:  # noqa: BLE001
        pass
    if _crash_cb is not None:
        try:
            _crash_cb(path, summary)
        except Exception:  # noqa: BLE001
            pass


def install_crash_handler(on_crash=None):
    """on_crash(rapor_yolu, özet) — HERHANGİ bir iş parçacığından çağrılabilir; arayüz tarafı
    bunu bir sinyalle ana iş parçacığına taşımalıdır."""
    global _crash_cb, _installed
    _crash_cb = on_crash
    if _installed:
        return
    _installed = True
    sys.excepthook = lambda t, e, tb: _handle(t, e, tb)

    def thread_hook(args):
        _handle(args.exc_type, args.exc_value, args.exc_traceback,
                f"iş parçacığı {args.thread.name if args.thread else '?'}")
    threading.excepthook = thread_hook
