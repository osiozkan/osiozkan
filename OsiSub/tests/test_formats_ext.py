"""Ek biçimler: TTML/DFXP (Netflix tikleri, kare saati, stiller, bölgeler), SBV, MicroDVD, WebVTT/metin yazma."""

import os
import tempfile
import unittest

from osisub.core.assfile import AssDocument, Event as E, Style
from osisub.core.formats import (export_bytes, load_any, parse_microdvd, parse_sbv, to_microdvd, to_sbv, to_text,
                                 to_vtt)
from osisub.core.ttml import TimeBase, parse_ttml, to_ttml

NETFLIX = b"""\xef\xbb\xbf<?xml version="1.0" encoding="utf-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tts="http://www.w3.org/ns/ttml#styling"
    xmlns:ttp="http://www.w3.org/ns/ttml#parameter" ttp:tickRate="10000000" xml:lang="tr">
  <head>
    <styling>
      <style xml:id="s1" tts:fontStyle="italic"/>
      <style xml:id="s2" tts:color="#FFFF00"/>
    </styling>
    <layout>
      <region xml:id="bottom" tts:displayAlign="after"/>
      <region xml:id="top" tts:displayAlign="before"/>
    </layout>
  </head>
  <body>
    <div>
      <p begin="12345670000t" end="12665670000t" region="bottom">
        Merhaba,
        <br/>
        d\xc3\xbcnya!
      </p>
      <p begin="20000000t" end="35000000t" region="top"><span style="s1">Tabela</span> yaz\xc4\xb1s\xc4\xb1</p>
      <p begin="40000000t" end="50000000t" style="s1">T\xc3\xbcm\xc3\xbc <span tts:fontStyle="normal" style="s2">sar\xc4\xb1</span> italik</p>
      <p begin="60000000t" end="60000000t"></p>
    </div>
  </body>
</tt>"""


class TestTtml(unittest.TestCase):
    def test_netflix_ticks_styles_regions(self):
        ev = parse_ttml(NETFLIX)
        self.assertEqual(len(ev), 3)                        # boş p atlandı; zamana göre sıralı
        self.assertEqual((ev[0].start, ev[0].end), (2000, 3500))
        self.assertEqual(ev[0].text, "{\\an8\\i1}Tabela{\\i0} yazısı")
        self.assertEqual(ev[1].text, "{\\i1}Tümü {\\i0\\c&H00FFFF&}sarı{\\i1\\c} italik")
        self.assertEqual((ev[2].start, ev[2].end), (1234570, 1266570))
        self.assertEqual(ev[2].text, "Merhaba,\\Ndünya!")

    def test_time_expressions(self):
        import xml.etree.ElementTree as ET
        root = ET.fromstring('<tt xmlns="http://www.w3.org/ns/ttml" xmlns:ttp="http://www.w3.org/ns/ttml#parameter" '
                             'ttp:frameRate="24" ttp:frameRateMultiplier="1000 1001"/>')
        tb = TimeBase(root)
        self.assertAlmostEqual(tb.ms("00:00:01:12"), 1500.5, delta=1)    # 1 sn + 12 kare (23.976)
        self.assertEqual(tb.ms("00:01:02.5"), 62500)
        self.assertEqual(tb.ms("12.5s"), 12500)
        self.assertEqual(tb.ms("1500ms"), 1500)
        self.assertEqual(tb.ms("1h"), 3600000)
        self.assertEqual(tb.ms("48f"), 2002)
        with self.assertRaises(ValueError):
            tb.ms("yarın")

    def test_export_roundtrip(self):
        doc = AssDocument.new()
        doc.ensure_section("styles").items.append(Style(name="İtalik", italic=True))
        doc.ensure_section("events").items = [
            E(start=1000, end=2500, text="{\\an8}Üstte\\Nikinci satır"),
            E(start=3000, end=4000, text="Normal {\\i1}italik{\\i0} ve {\\c&H0000FF&}kırmızı{\\c} & <tamam>"),
            E(start=5000, end=6000, style="İtalik", text="Stil italik"),
            E(start=7000, end=8000, text="{\\p1}m 0 0 l 10 10{\\p0}"),
            E(start=9000, end=9500, comment=True, text="yorum"),
        ]
        for fmt in ("clock", "ticks"):
            x = to_ttml(doc, time_format=fmt, title="Bölüm 1")
            self.assertIn('xml:lang="tr"', x)
            back = parse_ttml(x)
            self.assertEqual(len(back), 3, x)                 # çizim ve yorum yok
            self.assertEqual((back[0].start, back[0].end), (1000, 2500))
            self.assertEqual(back[0].text, "{\\an8}Üstte\\Nikinci satır")
            self.assertEqual(back[1].text, "Normal {\\i1}italik{\\i0} ve {\\c&H0000FF&}kırmızı{\\c} & <tamam>")
            self.assertEqual(back[2].text, "{\\i1}Stil italik")
        self.assertIn('ttp:tickRate="10000000"', to_ttml(doc, time_format="ticks"))


class TestOthers(unittest.TestCase):
    def test_sbv(self):
        txt = "0:00:01.000,0:00:03.500\nMerhaba\nikinci\n\n0:01:02.250,0:01:04.000\nTek\n"
        ev = parse_sbv(txt)
        self.assertEqual([(e.start, e.end, e.text) for e in ev], [(1000, 3500, "Merhaba\\Nikinci"),
                                                                   (62250, 64000, "Tek")])
        self.assertEqual(parse_sbv(to_sbv(ev))[0].text, "Merhaba\\Nikinci")
        self.assertTrue(to_sbv(ev).startswith("0:00:01.000,0:00:03.500\nMerhaba\nikinci\n"))

    def test_microdvd(self):
        txt = "{1}{1}25\n{25}{100}{y:i}Merhaba|dünya\n{125}{150}Tek\n"
        ev = parse_microdvd(txt)
        self.assertEqual([(e.start, e.end) for e in ev], [(1000, 4000), (5000, 6000)])
        self.assertEqual(ev[0].text, "{\\i1}Merhaba\\Ndünya")
        out = to_microdvd(ev, 25)
        self.assertTrue(out.startswith("{1}{1}25\r\n{25}{100}{y:i}Merhaba|dünya"))
        ev2 = parse_microdvd("{24}{48}x\n", fps=24)
        self.assertEqual((ev2[0].start, ev2[0].end), (1000, 2000))

    def test_vtt_and_text(self):
        ev = [E(start=1000, end=2000, text="{\\an8}{\\i1}Üst{\\i0}\\Nalt"), E(start=3000, end=4000, text="{\\p1}m 0 0")]
        v = to_vtt(ev)
        self.assertTrue(v.startswith("WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000 line:10%\n<i>Üst</i>\nalt\n"))
        self.assertNotIn("m 0 0", v)
        self.assertEqual(to_text(ev), "Üst\nalt\n")
        self.assertIn("[0:00:01.00 → 0:00:02.00]", to_text(ev, with_times=True))

    def test_load_any_dispatch(self):
        d = tempfile.mkdtemp()

        def w(name, data):
            p = os.path.join(d, name)
            with open(p, "wb") as f:
                f.write(data)
            return p
        self.assertEqual(len(load_any(w("a.dfxp", NETFLIX)).events), 3)
        self.assertEqual(len(load_any(w("a.xml", NETFLIX)).events), 3)
        self.assertEqual(load_any(w("a.sbv", b"0:00:01.000,0:00:02.000\nx\n")).events[0].text, "x")
        self.assertEqual(load_any(w("a.sub", b"{24}{48}x\n"), fps=24).events[0].end, 2000)
        with self.assertRaises(ValueError):
            load_any(w("b.sub", b"\x00\x00\x01\xba binary vobsub"))
        doc = load_any(w("a.dfxp", NETFLIX))
        self.assertIsNone(doc.path)
        for key in ("ass", "srt", "vtt", "ttml", "sbv", "sub", "txt", "txt_times"):
            self.assertTrue(export_bytes(doc, key))



class TestUi(unittest.TestCase):
    def test_export_menu_and_batch(self):
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        os.environ["APPDATA"] = tempfile.mkdtemp()
        from PySide6.QtWidgets import QApplication

        from osisub.config import Config
        from osisub.ui.batch_convert import convert_files, output_path
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        QApplication.instance() or QApplication([])
        cfg = Config(os.path.join(tempfile.mkdtemp(), "c.json"))
        cfg["load_aegisub_scripts"] = False
        ctx = Context(cfg)
        win = MainWindow(ctx, headless=True)
        d = tempfile.mkdtemp()
        try:
            src = os.path.join(d, "n.dfxp")
            with open(src, "wb") as f:
                f.write(NETFLIX)
            win.open_path(src, force=True)
            self.assertEqual(len(ctx.doc.events), 3)
            self.assertIn("(içe aktarıldı)", ctx.title)
            out = win.export_as("vtt", os.path.join(d, "x.vtt"))
            with open(out, encoding="utf-8") as f:
                self.assertTrue(f.read().startswith("WEBVTT"))
            ctx.config["ttml_time"] = "ticks"
            out = win.export_as("ttml", os.path.join(d, "x.dfxp"))
            with open(out, encoding="utf-8") as f:
                self.assertIn("ttp:tickRate", f.read())
            self.assertFalse(ctx.undo.modified)
            names = [a.text() for a in win.menuBar().actions()[0].menu().actions()]
            self.assertIn("Toplu dönüştür…", names)
            # toplu: iki dosya -> srt; kaynak üstüne yazılmaz
            srt = os.path.join(d, "a.srt")
            with open(srt, "w", encoding="utf-8") as f:
                f.write("1\n00:00:01,000 --> 00:00:02,000\nx\n")
            self.assertTrue(output_path(srt, "srt", None).endswith("a_donusum.srt"))
            log = []
            ok, bad = convert_files([src, srt, os.path.join(d, "yok.srt")], "srt", os.path.join(d, "cikti"),
                                    log=log.append)
            self.assertEqual((ok, bad), (2, 1))
            self.assertTrue(os.path.exists(os.path.join(d, "cikti", "n.srt")))
            dlg = win.show_batch_convert()
            dlg.add_paths([src, srt, src, os.path.join(d, "resim.png")])
            self.assertEqual(dlg.files.count(), 2)
            dlg.fmt.setCurrentIndex(dlg.fmt.findData("sbv"))
            self.assertEqual(dlg.run(), (2, 0))
            self.assertEqual(ctx.config["batch_convert"]["format"], "sbv")
        finally:
            win.close()


if __name__ == "__main__":
    unittest.main()
