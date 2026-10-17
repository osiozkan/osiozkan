"""Çoklu değiştir: Türkçe harf eşleşmesi, akıllı harf, etiket koruma, sıra, içe/dışa aktarma, pencere."""

import os
import tempfile
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from osisub.core import multireplace as M  # noqa: E402
from osisub.core.assfile import Event  # noqa: E402


def run(text, *rules):
    comp, err = M.compile_groups([M.Group("g", list(rules))])
    assert not err, err
    return M.apply_text(text, comp)[0]


class TestCore(unittest.TestCase):
    def test_turkish_case_insensitive(self):
        r = M.Rule("kır", "KIR", "word")
        self.assertEqual(run("kır kir KIR Kır", r), "KIR kir KIR KIR")         # kir (i) eşleşmez
        r = M.Rule("istanbul", "İzmir", "word")
        self.assertEqual(run("İSTANBUL istanbul Istanbul", r), "İZMİR İzmir Istanbul")   # I ≠ i

    def test_smart_case_and_word(self):
        r = M.Rule("herşey", "her şey", "word")
        self.assertEqual(run("Herşey yolunda, herşeyi gördüm, HERŞEY!", r), "Her şey yolunda, herşeyi gördüm, HER ŞEY!")
        r = M.Rule("Naruto", "Naruto Uzumaki", "word", case=True)
        self.assertEqual(run("Naruto'yu gördüm. naruto", r), "Naruto Uzumaki'yu gördüm. naruto")
        r = M.Rule("ar", "AR", "plain")
        self.assertEqual(run("Araba", r), "ARaba")

    def test_tags_and_drawings_kept(self):
        r = M.Rule("pos", "POZ", "plain")
        self.assertEqual(run("{\\pos(1,2)}pos", r), "{\\pos(1,2)}POZ")
        self.assertEqual(run("{\\p1}m 0 0 pos{\\p0}", r), "{\\p1}m 0 0 pos{\\p0}")

    def test_regex_and_order(self):
        r1 = M.Rule(r"(\d+)\.(\d+)", r"\1,\2", "regex")
        r2 = M.Rule(",", ";", "plain")
        self.assertEqual(run("3.5 kg", r1, r2), "3;5 kg")                       # sırayla uygulanır
        r = M.Rule(r"(\w+) (\w+)", r"\1\N\2", "regex")
        self.assertEqual(run("ali veli", r), "ali\\Nveli")                       # ASS \N kaçışı
        comp, err = M.compile_groups([M.Group("g", [M.Rule("(", "x", "regex")])])
        self.assertEqual(len(err), 1)

    def test_disabled_and_events(self):
        g1 = M.Group("a", [M.Rule("bir", "BİR", "word"), M.Rule("iki", "İKİ", "word", enabled=False)])
        g2 = M.Group("b", [M.Rule("üç", "ÜÇ", "word")], enabled=False)
        evs = [Event(text="bir iki üç"), Event(text="yok", comment=True), Event(text="bir")]
        ch, why, err = M.apply_events(evs, [0, 1, 2], [g1, g2])
        self.assertEqual(ch, {0: {"text": "BİR iki üç"}, 2: {"text": "BİR"}})
        self.assertEqual(why[0], ["bir → BİR"])

    def test_defaults_fix_common_typos(self):
        comp, _ = M.compile_groups(M.default_groups())
        out, hits = M.apply_text("Herşey yolunda ama malesef herkez yanlız.", comp)
        self.assertEqual(out, "Her şey yolunda ama maalesef herkes yalnız.")
        self.assertEqual(len(hits), 4)

    def test_json_and_se_xml(self):
        d = tempfile.mkdtemp()
        p = os.path.join(d, "k.json")
        M.save_json(p, M.default_groups())
        g = M.load_file(p)
        self.assertEqual(g[0].name, "Türkçe sık yazım hataları")
        self.assertEqual(len(g[0].rules), len(M.default_groups()[0].rules))
        xml = ("<?xml version='1.0' encoding='utf-8'?><MultipleSearchAndReplaceGroups><Group><Name>SE</Name>"
               "<Enabled>True</Enabled><MultipleSearchAndReplaceItem><Enabled>True</Enabled><FindWhat>teh</FindWhat>"
               "<ReplaceWith>the</ReplaceWith><SearchType>CaseSensitive</SearchType><Description>yazım</Description>"
               "</MultipleSearchAndReplaceItem><MultipleSearchAndReplaceItem><Enabled>False</Enabled>"
               "<FindWhat>a+</FindWhat><ReplaceWith>a</ReplaceWith><SearchType>RegularExpression</SearchType>"
               "</MultipleSearchAndReplaceItem></Group></MultipleSearchAndReplaceGroups>")
        px = os.path.join(d, "se.xml")
        with open(px, "w", encoding="utf-8") as f:
            f.write(xml)
        g = M.load_file(px)
        self.assertEqual(len(g), 1)
        self.assertEqual(g[0].name, "SE")
        r0, r1 = g[0].rules
        self.assertEqual((r0.find, r0.replace, r0.mode, r0.case, r0.note), ("teh", "the", "plain", True, "yazım"))
        self.assertEqual((r1.mode, r1.enabled), ("regex", False))


class TestDialog(unittest.TestCase):
    def test_flow(self):
        os.environ["APPDATA"] = tempfile.mkdtemp()
        from PySide6.QtWidgets import QApplication

        from osisub.config import Config
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        app = QApplication.instance() or QApplication([])
        cfg = Config(os.path.join(tempfile.mkdtemp(), "c.json"))
        cfg["load_aegisub_scripts"] = False
        ctx = Context(cfg)
        win = MainWindow(ctx, headless=True)
        try:
            win.open_path(os.path.join(os.path.dirname(__file__), "fixtures", "sample.ass"), force=True)
            ctx.update_events({1: {"text": "Herşey yolunda, malesef."}}, "t")
            dlg = win.show_multi_replace()
            self.assertEqual(dlg.glist.count(), 1)                          # başlangıç kuralları
            dlg.new_group("Benim")
            dlg.add_rule("yolunda", "yolundaydı", "word")
            self.assertEqual(len(dlg.groups[-1].rules), 1)
            ch, err = dlg.preview()
            self.assertEqual(err, [])
            self.assertEqual(ch[1]["text"], "Her şey yolundaydı, maalesef.")
            self.assertEqual(dlg.prev.topLevelItemCount(), 1)
            dlg.apply()
            self.assertEqual(ctx.doc.events[1].text, "Her şey yolundaydı, maalesef.")
            self.assertTrue(ctx.undo.undo_desc().startswith("Çoklu değiştir"))
            saved = ctx.config["multi_replace"]
            self.assertEqual([g["name"] for g in saved], ["Türkçe sık yazım hataları", "Benim"])
            dlg2 = win.show_multi_replace()
            self.assertEqual(dlg2.glist.count(), 2)                         # kurallar hatırlandı
            p = dlg2.export_file(os.path.join(tempfile.mkdtemp(), "k.json"))
            self.assertEqual(dlg2.import_file(p), 2)
            self.assertEqual(dlg2.glist.count(), 4)
            app.processEvents()
        finally:
            win.close()


if __name__ == "__main__":
    unittest.main()
