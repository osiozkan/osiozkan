"""aligncompare (kaynak ↔ aday karşılaştırma) testleri — çekirdek, komut satırı ve bölme."""

import contextlib
import io
import os
import random
import tempfile
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from osisub.core.assfile import AssDocument, Event as E
from osisub.core.asstime import round_ms
from osisub.core.compare import (CompareOptions, compare, english_words, find_shift, marked_document,
                                 to_csv, to_html)
from osisub.core.formats import to_srt

EN = ["Where are you going?", "I told you not to come back here.", "We need to find the key before dawn.",
      "That's the only way out.", "Do you really think so?", "Look at the sky tonight.",
      "He said he would wait for us.", "Let's go, everyone!", "I can't believe this is happening.",
      "Thank you for everything."]
TR = ["Nereye gidiyorsun?", "Buraya geri gelme demiştim.", "Şafaktan önce anahtarı bulmalıyız.",
      "Tek çıkış yolu bu.", "Gerçekten öyle mi düşünüyorsun?", "Bu gece gökyüzüne bak.",
      "Bizi bekleyeceğini söyledi.", "Hadi gidelim, millet!", "Bunun olduğuna inanamıyorum.",
      "Her şey için teşekkürler."]


def _read(p):
    with open(p, "rb") as f:
        return f.read()


def make_pair(shift=2350, n=160, seed=7):
    """Kaynak satırları + aynı bölümün Türkçe adayı (kaymalı). Bilinen bozulmalar:
    satır 10 → aday yok (EKSİK), satır 20 → adayda iki parça (1:2), 30+31 → adayda tek satır (2:1),
    40 → İngilizce bırakılmış, 50 → boş, 60 → sayı eksik, 70 → 0,9 sn geç (ZAMAN), sonda kredi satırı (FAZLA)."""
    rnd = random.Random(seed)
    src, cand = [], []
    t = 5000
    for i in range(n):
        dur = rnd.randint(1200, 3600)
        k = i % len(EN)
        src.append(E(start=t, end=t + dur, text=EN[k] + (f" It's {i + 1000} yen." if i == 60 else "")))
        t += dur + rnd.choice([150, 300, 600, 1200, 2500, 4500])
    for i, s in enumerate(src):
        k = i % len(TR)
        st, en = s.start + shift, s.end + shift
        if i == 10:
            continue
        if i == 20:
            mid = (st + en) // 2
            cand.append(E(start=st, end=mid, text=TR[k][:len(TR[k]) // 2]))
            cand.append(E(start=mid, end=en, text=TR[k][len(TR[k]) // 2:]))
            continue
        if i == 31:
            continue
        if i == 30:
            cand.append(E(start=st, end=src[31].end + shift, text=TR[k] + " " + TR[(k + 1) % len(TR)]))
            continue
        text = TR[k]
        if i == 40:
            text = "We need to find the key, and you know that."
        if i == 50:
            text = "{\\i1}{\\i0}"
        if i == 60:
            text = TR[k] + " Bin yen tutuyor."
        if i == 70:
            st += 900
        cand.append(E(start=st, end=en, text=text))
    cand.append(E(start=src[-1].end + shift + 3000, end=src[-1].end + shift + 7000,
                  text="Çeviri: Osi — iyi seyirler"))
    return src, cand


class TestShift(unittest.TestCase):
    def test_finds_shift_despite_splits_and_merges(self):
        for shift in (2350, -1780, 0, 11420):
            src, cand = make_pair(shift)
            res = compare(src, cand)
            self.assertTrue(res.shift_auto)
            self.assertLessEqual(abs(res.shift_ms - shift), 20, (shift, res.shift_ms))
            self.assertGreater(res.shift_score, 0.6)
            self.assertFalse(res.warnings, res.warnings)

    def test_wrong_pair_warns(self):
        src, _ = make_pair(0, seed=1)
        _, cand = make_pair(0, seed=99)            # başka zamanlamalı "bölüm"
        res = compare(src, cand)
        self.assertTrue(any("Zayıf ilinti" in w for w in res.warnings), res.warnings)

    def test_manual_shift(self):
        src, cand = make_pair(2350)
        res = compare(src, cand, CompareOptions(shift=2.35))
        self.assertFalse(res.shift_auto)
        self.assertEqual(res.shift_ms, 2350)
        res0 = compare(src, cand, CompareOptions(shift=0))
        self.assertGreater(res0.counts()["EKSİK"], 20)          # kaymasız: eşleşme çöker

    def test_find_shift_empty(self):
        self.assertEqual(find_shift([], [], 15000), (0, 0.0))


class TestFlags(unittest.TestCase):
    def setUp(self):
        self.src, self.cand = make_pair(2350)
        self.res = compare(self.src, self.cand)

    def group_of_src(self, i):
        for g in self.res.groups:
            if any(x.row == i for x in g.src):
                return g
        self.fail(f"kaynak {i} grupta yok")

    def test_kinds(self):
        self.assertEqual(self.group_of_src(20).kind, "1:2")
        self.assertEqual(self.group_of_src(30).kind, "2:1")
        self.assertIs(self.group_of_src(30), self.group_of_src(31))
        kinds = self.res.kinds()
        self.assertEqual(kinds.get("1:2"), 1)
        self.assertEqual(kinds.get("2:1"), 1)

    def test_each_flag(self):
        self.assertEqual(self.group_of_src(10).codes, ["EKSİK"])
        self.assertIn("İNGİLİZCE?", self.group_of_src(40).codes)
        self.assertIn("BOŞ", self.group_of_src(50).codes)
        g60 = self.group_of_src(60)
        self.assertIn("SAYI", g60.codes)
        self.assertIn("1060", next(f.detail for f in g60.flags if f.code == "SAYI"))
        g70 = self.group_of_src(70)
        self.assertEqual(g70.codes, ["ZAMAN"])
        self.assertIn("+0.90", g70.flags[0].detail)
        extra = [g for g in self.res.groups if not g.src]
        self.assertEqual(len(extra), 1)
        self.assertEqual(extra[0].codes, ["FAZLA"])
        c = self.res.counts()
        self.assertEqual((c["EKSİK"], c["FAZLA"], c["BOŞ"], c["ZAMAN"]), (1, 1, 1, 1))
        # temiz satırlarda yanlış alarm yok (2:1 birleşmesi uzunluk/ZAMAN bayrağı almamalı)
        self.assertEqual(self.group_of_src(30).codes, [])
        self.assertEqual(self.group_of_src(20).codes, [])
        clean = [g for g in self.res.groups if g.src and g.cand and not g.flags]
        self.assertGreater(len(clean), 140)

    def test_copy_of_source_and_length(self):
        src = [E(start=0, end=2000, text="Giorno Giovanna"), E(start=3000, end=5000, text="This is a long sentence to translate.")]
        cand = [E(start=0, end=2000, text="Giorno Giovanna"), E(start=3000, end=5000, text="Evet.")]
        res = compare(src, cand, CompareOptions(shift=0))
        self.assertIn("İNGİLİZCE?", res.groups[0].codes)          # özel ad: kılavuzdaki gibi tetiklenir
        self.assertIn("UZUNLUK", res.groups[1].codes)
        loose = compare(src, cand, CompareOptions(shift=0, sim=1.01, ratio_min=0.1))
        self.assertEqual(loose.groups[0].codes, [])
        self.assertEqual(loose.groups[1].codes, [])

    def test_english_heuristic(self):
        self.assertTrue(english_words("I told you not to come back here."))
        self.assertFalse(english_words("Buraya geri gelme demiştim."))
        self.assertFalse(english_words("Hey, OK, her şey on dakikada biter!"))     # Türkçede de geçenler
        self.assertFalse(english_words("Oh, hey."))


class TestTypesetAndStyles(unittest.TestCase):
    def test_typeset_skipped_by_default(self):
        src = [E(start=0, end=2000, text="Hello there."), E(start=0, end=2000, text="{\\pos(10,10)}SHOP"),
               E(start=3000, end=4000, text="Sign\\N \\N \\N")]
        cand = [E(start=0, end=2000, text="Merhaba."), E(start=0, end=2000, text="{\\pos(10,10)}DÜKKÂN")]
        res = compare(src, cand, CompareOptions(shift=0))
        self.assertEqual((res.src_count, res.cand_count), (1, 1))
        self.assertEqual(res.src_skipped["typeset"], 2)
        res2 = compare(src, cand, CompareOptions(shift=0, include_typeset=True))
        self.assertEqual((res2.src_count, res2.cand_count), (3, 2))

    def test_skip_style_regex(self):
        src = [E(start=0, end=2000, text="La la la", style="OP"), E(start=3000, end=5000, text="Hi.")]
        cand = [E(start=3000, end=5000, text="Selam.", style="Default"),
                E(start=0, end=2000, text="La la la", style="Karaoke")]
        res = compare(src, cand, CompareOptions(shift=0, skip_styles_src="^OP$", skip_styles_cand="Karaoke|OP|ED"))
        self.assertEqual((res.src_count, res.cand_count), (1, 1))
        self.assertEqual(res.counts()["EKSİK"] + res.counts()["FAZLA"], 0)


class TestOutputs(unittest.TestCase):
    def setUp(self):
        self.src, self.cand = make_pair(2350)
        self.res = compare(self.src, self.cand)

    def test_html_and_csv(self):
        h = to_html(self.res, "kaynak_en.srt", "aday_tr.ass")
        self.assertIn("<!doctype html>", h)
        self.assertIn("EKSİK (1)", h)
        self.assertIn("+2.35 sn (aday kaynaktan GEÇ)", h)
        self.assertIn("Çeviri: Osi", h)
        hf = to_html(self.res, "kaynak_en.srt", "aday_tr.ass", only_flags=True)
        self.assertLess(len(hf), len(h) / 3)
        c = to_csv(self.res)
        self.assertTrue(c.startswith("﻿"))
        self.assertGreater(c.count("\r\n"), 150)
        self.assertEqual(to_csv(self.res, only_flags=True).count("\r\n"), 1 + len(self.res.flagged()))

    def test_marked_copy(self):
        doc = AssDocument.new()
        doc.ensure_section("events").items = list(self.cand)
        raw = AssDocument.from_text(doc.to_text())          # ham bağlı (okunmuş) satırlar
        before = raw.to_text()
        m = marked_document(raw, self.res)
        self.assertEqual(raw.to_text(), before)              # özgün belge değişmedi
        evs = m.events
        tagged = [e for e in evs if e.text.startswith("{QC: ")]
        self.assertEqual(len(tagged), 6)                     # FAZLA, İNGİLİZCE?, BOŞ, SAYI, ZAMAN, ... (EKSİK hariç)
        miss = [e for e in evs if e.comment and e.text.startswith("{QC: EKSİK}")]
        self.assertEqual(len(miss), 1)
        self.assertEqual(miss[0].start, round_ms(self.src[10].start + 2350))
        # dokunulmayan satırlar bayt bayt aynı
        out_lines = m.to_text().splitlines()
        in_lines = before.splitlines()
        same = sum(1 for x in in_lines if x in out_lines)
        self.assertGreaterEqual(same, len(in_lines) - 6)


class TestCli(unittest.TestCase):
    def test_cli_outputs(self):
        from osisub.__main__ import main
        src, cand = make_pair(2350)
        d = tempfile.mkdtemp()
        sp = os.path.join(d, "kaynak_en.srt")
        with open(sp, "w", encoding="utf-8") as f:
            f.write(to_srt(src))
        doc = AssDocument.new()
        doc.ensure_section("events").items = list(cand)
        cp = os.path.join(d, "aday_tr.ass")
        doc.save(cp)
        data = _read(cp)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            rc = main(["aligncompare", sp, cp, "--marked", os.path.join(d, "qc.ass"), "--csv",
                       os.path.join(d, "qc.csv"), "--only-flags"])
        self.assertEqual(rc, 0)
        self.assertIn("Bulunan kayma: +2.35 sn", out.getvalue())
        self.assertTrue(os.path.exists(os.path.join(d, "aday_tr_karsilastirma.html")))
        self.assertTrue(os.path.exists(os.path.join(d, "qc.csv")))
        q = AssDocument.load(os.path.join(d, "qc.ass"))
        self.assertTrue(any(e.text.startswith("{QC: EKSİK}") for e in q.events))
        self.assertEqual(_read(cp), data)                    # aday dosyaya dokunulmadı
        with contextlib.redirect_stdout(io.StringIO()):
            rc = main(["aligncompare", sp, cp, "--shift", "2.35", "-o", os.path.join(d, "r.html")])
        self.assertEqual(rc, 0)
        self.assertIn("elle verildi", _read(os.path.join(d, "r.html")).decode("utf-8"))



def write_pair(d, shift=2350):
    src, cand = make_pair(shift)
    sp = os.path.join(d, "kaynak_en.srt")
    with open(sp, "w", encoding="utf-8") as f:
        f.write(to_srt(src))
    doc = AssDocument.new()
    doc.ensure_section("events").items = list(cand)
    cp = os.path.join(d, "aday_tr.ass")
    doc.save(cp)
    return src, cand, sp, cp


class TestComparePanel(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        os.environ["APPDATA"] = tempfile.mkdtemp()
        global Qt
        from PySide6.QtCore import Qt
        from PySide6.QtWidgets import QApplication
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        from osisub.config import Config
        from osisub.ui import theme
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        cfg = Config(os.path.join(tempfile.mkdtemp(), "config.json"))
        cfg["load_aegisub_scripts"] = False
        theme.apply(self.app, "dark")
        self.ctx = Context(cfg)
        self.win = MainWindow(self.ctx, headless=True)
        self.win.show()
        self.d = tempfile.mkdtemp()
        self.src, self.cand, self.sp, self.cp = write_pair(self.d)
        self.win.open_path(self.cp, force=True)
        self.app.processEvents()

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        self.app.processEvents()

    def test_flow(self):
        win, ctx = self.win, self.ctx
        self.assertIsNone(win.show_compare())                    # kaynak yok: çalışmaz, çökmez
        self.assertFalse(win.compare_panel.b_run.isEnabled())
        win.open_reference(self.sp)
        ref = win.reference
        # kaymasız eşleme yanlış: ilk TR satırının yanında 2. EN satırı görünüyordu
        self.assertNotEqual(ref.text_for(ctx.doc.events[0]), "Where are you going?")
        res = win.show_compare()
        self.assertIsNotNone(res)
        self.assertEqual(res.shift_ms, 2350)
        self.assertEqual(ref.offset, 2350)                       # bulunan kayma referansa uygulandı
        self.assertEqual(ref.text_for(ctx.doc.events[0]), "Where are you going?")
        from osisub.ui.grid import C_REF
        self.assertEqual(win.grid.model().headerData(C_REF, Qt.Horizontal), "Referans (+2.35 sn)")
        self.assertEqual(win.ref_panel.offset.value(), 2.35)
        ref.set_offset(0)
        self.assertEqual(win.ref_panel.find_offset(), 2350)       # Referans bölmesindeki "Kaymayı bul"
        self.assertEqual(ref.offset, 2350)
        # aktarma da kaymayla eşler: yalnız gerçekten karşılıksız olan (kaynak 10) ev sahipsiz kalır
        from osisub.ui.reference import TransferDialog
        plan = TransferDialog(ctx, ref, win).preview()
        self.assertEqual(plan.stats["orphans"], 1)
        ref.set_offset(0)
        self.assertGreater(TransferDialog(ctx, ref, win).preview().stats["ambiguous"] +
                           TransferDialog(ctx, ref, win).preview().stats["weak"], 20)
        ref.set_offset(2350)
        p = win.compare_panel
        self.assertTrue(win.dock_compare.isVisible())
        self.assertEqual(p.tree.topLevelItemCount(), len(res.flagged()))    # varsayılan: yalnız bayraklı
        p.filter.setCurrentIndex(0)
        self.assertEqual(p.tree.topLevelItemCount(), len(res.groups))
        p.filter.setCurrentIndex(p.filter.findData("ZAMAN"))
        self.assertEqual(p.tree.topLevelItemCount(), 1)
        it = p.tree.topLevelItem(0)
        p._go(it)
        self.assertEqual(ctx.active, 69)                         # kaynak 70 → aday satırı 70 (10 eksik)
        rows = p.select_flagged()
        self.assertEqual(len(rows), 5)
        self.assertEqual(ctx.selection, set(rows))
        # eksikleri ekle: tek geri al
        n = len(ctx.doc.events)
        self.assertEqual(p.add_missing(), 1)
        self.assertEqual(len(ctx.doc.events), n + 1)
        added = [i for i, e in enumerate(ctx.doc.events) if e.comment and e.text.startswith("{QC: EKSİK}")]
        self.assertEqual(added, [10])                            # zamanca doğru yerde
        self.assertEqual(p.result.counts()["EKSİK"], 1)           # yorum satırı çeviri sayılmaz: hâlâ eksik
        self.assertEqual(p.add_missing(), 0)                      # ikinci kez eklemez
        self.assertEqual(len(ctx.doc.events), n + 1)
        ctx.do_undo()
        self.assertEqual(len(ctx.doc.events), n)
        self.assertTrue(p.stale)
        # dışa aktarma
        h = p.export("html", os.path.join(self.d, "r.html"))
        c = p.export("csv", os.path.join(self.d, "r.csv"))
        m = p.export("marked", os.path.join(self.d, "r_qc.ass"))
        for x in (h, c, m):
            self.assertTrue(os.path.exists(x))
        self.assertIsNone(p.export("marked", self.cp))            # açık belgenin üstüne yazmaz
        self.assertFalse(ctx.undo.modified)


if __name__ == "__main__":
    unittest.main()
