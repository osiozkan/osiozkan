"""1.0 sağlamlaştırma: çökme raporu, otomatik yedek/kurtarma, paketli (frozen) yol çözümü, simge."""

import os
import shutil
import sys
import tempfile
import threading
import time
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"

from PySide6.QtWidgets import QApplication  # noqa: E402

app = QApplication.instance() or QApplication([])

HERE = os.path.dirname(os.path.abspath(__file__))


class _Home(unittest.TestCase):
    """Kullanıcı klasörünü geçici bir yere al (gerçek yedeklere/raporlara dokunma)."""

    def setUp(self):
        self._env = {k: os.environ.get(k) for k in ("HOME", "APPDATA")}
        self.home = tempfile.mkdtemp()
        os.environ["HOME"] = self.home
        os.environ["APPDATA"] = self.home

    def tearDown(self):
        for k, v in self._env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.home, ignore_errors=True)


class TestCrash(_Home):
    def test_report_and_thread_hook(self):
        from osisub import diag
        got = []
        old_hook, old_thook = sys.excepthook, threading.excepthook
        diag._installed = False
        try:
            diag.install_crash_handler(lambda p, s: got.append((p, s)))
            stderr = sys.stderr
            sys.__stderr__ = open(os.devnull, "w")   # raporun konsol kopyasını sustur

            def boom():
                raise ValueError("deneme hatası")
            t = threading.Thread(target=boom, name="deneme")
            t.start()
            t.join()
            sys.excepthook(*_exc_info(KeyError("ana")))
        finally:
            sys.excepthook, threading.excepthook = old_hook, old_thook
            diag._installed = False
            diag._crash_cb = None
            sys.__stderr__ = stderr
        self.assertEqual(len(got), 2)
        path, summary = got[0]
        self.assertTrue(os.path.exists(path))
        self.assertIn("ValueError: deneme hatası", summary)
        with open(path, encoding="utf-8") as fh:
            body = fh.read()
        self.assertIn("iş parçacığı deneme", body)
        self.assertIn("Traceback", body)
        self.assertTrue(path.startswith(diag.logs_dir()))
        self.assertIn("KeyError", got[1][1])


def _exc_info(exc):
    try:
        raise exc
    except Exception:  # noqa: BLE001
        return sys.exc_info()


class TestAutosave(_Home):
    def test_backup_cycle_and_recovery(self):
        from osisub.config import Config
        from osisub.ui import recovery
        from osisub.ui.context import Context
        d = tempfile.mkdtemp()
        src = os.path.join(d, "bölüm 01.ass")
        shutil.copy(os.path.join(HERE, "fixtures", "sample.ass"), src)
        old = time.time() - 100
        os.utime(src, (old, old))
        ctx = Context(Config(os.path.join(d, "c.json")))
        ctx.open_document(src)
        auto = recovery.Autosave(ctx, 0)
        self.assertIsNone(auto.tick())                       # değişiklik yok
        ctx.update_events({0: {"text": "yedeklenecek değişiklik"}}, "test")
        p = auto.tick()
        self.assertTrue(p and os.path.exists(p))
        self.assertIsNone(auto.tick())                       # aynı durum: yeniden yazma
        ctx.update_events({0: {"text": "ikinci"}}, "test")
        self.assertEqual(auto.tick(), p)
        # önceki oturumdan kalmış gibi: kurtarılabilir listede
        items = recovery.find_recoverable()
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]["original"], os.path.abspath(src))
        # kaydedince bu oturumun yedeği silinir
        ctx.save_document()
        self.assertFalse(os.path.exists(p))
        self.assertEqual(recovery.find_recoverable(), [])
        # çökme yedeği: düzgün kapanışta (clear) silinmez
        ctx.update_events({0: {"text": "çökmeden önce"}}, "test")
        e = auto.emergency()
        auto.clear()
        self.assertTrue(os.path.exists(e))
        # kurtarma: özgün yol, 'değişmiş' durum; özgün dosyaya yazılmadı
        ctx2 = Context(Config(os.path.join(d, "c2.json")))
        items = recovery.find_recoverable()
        self.assertEqual(len(items), 1)
        with open(src, "rb") as fh:
            before = fh.read()
        recovery.restore_into(ctx2, items[0])
        self.assertEqual(ctx2.doc.path, os.path.abspath(src))
        self.assertTrue(ctx2.undo.modified)
        self.assertEqual(ctx2.doc.events[0].text, "çökmeden önce")
        with open(src, "rb") as fh:
            self.assertEqual(fh.read(), before)
        self.assertEqual(recovery.find_recoverable(), [])    # yedek kullanıldı
        # özgün dosya yedekten yeniyse yedek önerilmez ve temizlenir
        ctx.update_events({0: {"text": "eski yedek"}}, "test")
        stale = auto.emergency()
        future = time.time() + 50
        os.utime(src, (future, future))
        self.assertEqual(recovery.find_recoverable(), [])
        self.assertFalse(os.path.exists(stale))

    def test_untitled_and_dialog(self):
        from osisub.config import Config
        from osisub.ui import recovery
        from osisub.ui.context import Context
        ctx = Context(Config(os.path.join(tempfile.mkdtemp(), "c.json")))
        auto = recovery.Autosave(ctx, 0)
        ctx.update_events({0: {"text": "adsız iş"}}, "test")
        p = auto.emergency()
        self.assertIn("adsiz-", os.path.basename(p))
        items = recovery.find_recoverable()
        self.assertEqual(items[0]["original"], None)
        dlg = recovery.RecoveryDialog(items)
        self.assertEqual(dlg.list.count(), 1)
        dlg._delete()
        self.assertEqual(recovery.find_recoverable(), [])


class TestFrozenPaths(unittest.TestCase):
    def test_bin_dirs_follow_exe_when_frozen(self):
        from osisub import paths
        d = tempfile.mkdtemp()
        old_exe, old_frozen = sys.executable, getattr(sys, "frozen", None)
        try:
            sys.frozen = True
            sys.executable = os.path.join(d, "OsiSub.exe")
            self.assertTrue(paths.frozen())
            self.assertEqual(paths.exe_dir(), d)
            self.assertEqual(paths.bin_dirs()[0], os.path.join(d, "bin"))
            # ffmpeg bin/ içine konursa önce o bulunur
            os.makedirs(os.path.join(d, "bin"))
            exe = os.path.join(d, "bin", "ffmpeg" + (".exe" if sys.platform.startswith("win") else ""))
            with open(exe, "w") as fh:
                fh.write("")
            from osisub.media.probe import tool
            self.assertEqual(tool("ffmpeg"), exe)
        finally:
            sys.executable = old_exe
            if old_frozen is None:
                del sys.frozen
            else:
                sys.frozen = old_frozen
        self.assertFalse(paths.frozen())

    def test_icon_and_version_cli(self):
        from osisub import __version__
        from osisub.__main__ import main
        from osisub.ui.icon import app_icon, write_ico
        self.assertFalse(app_icon().isNull())
        p = os.path.join(tempfile.mkdtemp(), "o.ico")
        self.assertTrue(write_ico(p))
        self.assertGreater(os.path.getsize(p), 1000)
        import contextlib
        import io
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(main(["--version"]), 0)
        self.assertIn(__version__, buf.getvalue())

    def test_spec_lists_lua_data(self):
        # paketleme tarifi Lua verisini (include/autoload) kapsamalı
        with open(os.path.join(os.path.dirname(HERE), "packaging", "osisub.spec"), encoding="utf-8") as fh:
            spec = fh.read()
        self.assertIn('collect_data_files("osisub"', spec)
        self.assertIn('collect_submodules("lupa")', spec)


if __name__ == "__main__":
    unittest.main()
