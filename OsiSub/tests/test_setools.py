"""Subtitle Edit araçları (core/setools): satır kırma, harf, HI, süreler, boşluk, birleştir/böl, senkron, sıralama."""

import unittest

from osisub.core.assfile import Event as E
from osisub.core.setools import (DurationOptions, HiOptions, MergeOptions, adjust_durations, auto_break,
                                 bridge_gaps, change_case, change_fps, change_speed, mask_tags, merge_short,
                                 point_sync, remove_hi, sort_rows, split_long, statistics, unbreak, unmask)
from osisub.core.textutil import cpl_lengths


class TestBreak(unittest.TestCase):
    def check_ok(self, out, limit=42):
        lens = cpl_lengths(out)
        self.assertEqual(len(lens), 2, out)
        self.assertTrue(all(n <= limit for n in lens), (out, lens))
        return lens

    def test_long_line_balanced(self):
        t = "Bu gerçekten çok uzun bir cümle ve iki satıra bölünmesi gerekiyor arkadaşlar."
        out = auto_break(t)
        lens = self.check_ok(out)
        self.assertLessEqual(abs(lens[0] - lens[1]), 12)
        self.assertEqual(out.replace("\\N", " "), t)

    def test_short_untouched_and_force(self):
        self.assertEqual(auto_break("Kısa bir satır."), "Kısa bir satır.")
        self.assertEqual(auto_break("Kısa\\Nsatır"), "Kısa\\Nsatır")              # kurala uyuyor
        self.assertEqual(auto_break("Kısa\\Nsatır", force=True), "Kısa satır")

    def test_tags_preserved(self):
        t = "{\\an8}{\\i1}Bu gerçekten çok uzun bir cümle{\\i0} ve iki satıra bölünmesi gerekiyor."
        out = auto_break(t)
        self.check_ok(out)
        self.assertTrue(out.startswith("{\\an8}{\\i1}"))
        self.assertIn("{\\i0}", out)
        self.assertEqual(out.replace("\\N", " "), t)

    def test_prefers_punctuation(self):
        out = auto_break("Bunu yapmamalıydın kardeşim, şimdi hepimiz büyük bir belanın içindeyiz.")
        self.assertIn("kardeşim,\\Nşimdi", out)

    def test_turkish_particles(self):
        out = auto_break("Gerçekten bunu benim için yapacak mısın sevgili arkadaşım?")
        self.check_ok(out)
        self.assertNotIn("\\Nmısın", out)
        out = auto_break("Ben de seninle gelmek istiyorum ama annem buna da izin vermez.")
        self.assertNotIn("\\Nda ", out)
        self.assertNotIn("\\Nde ", out)

    def test_dialog_dash(self):
        out = auto_break("- Nereye gidiyorsun böyle? - Eve gidiyorum, çok yoruldum bugün.")
        self.assertEqual(out, "- Nereye gidiyorsun böyle?\\N- Eve gidiyorum, çok yoruldum bugün.")
        t = "- Bu satır çok uzun ve kırılması gerekse bile yapı korunur.\\N- Evet."
        self.assertEqual(auto_break(t), t)

    def test_unbreak(self):
        self.assertEqual(unbreak("Bir\\Niki"), "Bir iki")
        self.assertEqual(unbreak("Bir \\N iki"), "Bir iki")
        self.assertEqual(unbreak("- A\\N- B"), "- A\\N- B")
        self.assertEqual(unbreak("{\\i1}Bir\\N{\\i0}iki"), "{\\i1}Bir {\\i0}iki")

    def test_drawing_untouched(self):
        t = "{\\p1}m 0 0 l 100 0 100 100 0 100 bu çizim çok uzun olsa da dokunulmaz{\\p0}"
        self.assertEqual(auto_break(t), t)
        self.assertEqual(change_case(t, "upper"), t)
        self.assertEqual(remove_hi(t), t)

    def test_mask_roundtrip(self):
        t = "{\\pos(1,2)}(a) {\\t(0,100,\\fs20)}b"
        m, blocks = mask_tags(t)
        self.assertNotIn("(1,2)", m)
        self.assertEqual(unmask(m, blocks), t)


class TestCase(unittest.TestCase):
    def test_modes(self):
        self.assertEqual(change_case("{\\i1}istanbul ılık{\\i0}\\Nyağmur", "upper"),
                         "{\\i1}İSTANBUL ILIK{\\i0}\\NYAĞMUR")
        self.assertEqual(change_case("İSTANBUL IŞIK\\N", "lower"), "istanbul ışık\\N")
        self.assertEqual(change_case("MERHABA. NASILSIN? iyiyim.", "sentence"), "Merhaba. Nasılsın? İyiyim.")
        self.assertEqual(change_case("naruto'yu gördün mü", "title"), "Naruto'yu Gördün Mü")
        self.assertEqual(change_case("{\\fnArial}ALİ\\hVELİ", "lower"), "{\\fnArial}ali\\hveli")


class TestHi(unittest.TestCase):
    def test_basic(self):
        self.assertEqual(remove_hi("[kapı çarpar] Kim var orada?"), "Kim var orada?")
        self.assertEqual(remove_hi("(güler) Çok komik."), "Çok komik.")
        self.assertEqual(remove_hi("ALİ: Merhaba!"), "Merhaba!")
        self.assertEqual(remove_hi("- ALİ: Selam.\\N- AYŞE: Selam."), "- Selam.\\N- Selam.")
        self.assertEqual(remove_hi("♪ La la la ♪"), "")
        self.assertEqual(remove_hi("Saat 10:30'da gel."), "Saat 10:30'da gel.")
        self.assertEqual(remove_hi("Dikkat: kapı açık."), "Dikkat: kapı açık.")    # karışık harf varsayılan kapalı
        self.assertEqual(remove_hi("Dikkat: kapı açık.", HiOptions(speaker_mixed_case=True)), "kapı açık.")

    def test_tags_and_cleanup(self):
        self.assertEqual(remove_hi("{\\pos(10,20)}(sighs) Fine."), "{\\pos(10,20)}Fine.")
        self.assertEqual(remove_hi("{\\an8}[MUSIC]\\NHello there."), "{\\an8}Hello there.")
        self.assertEqual(remove_hi("- [sighs]\\N- Neyse."), "Neyse.")
        self.assertEqual(remove_hi("[a]", HiOptions(remove_empty=False)), "")
        self.assertEqual(remove_hi("Gel (buraya) hemen."), "Gel hemen.")


def evs(*spans, text="Merhaba dünya nasılsın bugün"):
    return [E(start=a, end=b, text=text) for a, b in spans]


class TestTiming(unittest.TestCase):
    def test_durations_cps(self):
        e = evs((0, 1000), (5000, 6000), (6300, 6500))
        e[0] = e[0].replace(text="x" * 30)                        # 30 karakter, 15 kr/sn -> 2000 ms
        ch = adjust_durations(e, [0, 1, 2], DurationOptions(cps=15, min_gap_ms=84))
        self.assertEqual(ch[0], {"end": 2000})
        self.assertEqual(ch[1]["end"], 6220)                     # sonraki 6300'de başlıyor, 84 ms pay -> 6216 -> 6220
        self.assertEqual(ch[2]["end"], 7970)                     # 25 karakter / 15 = 1667 ms
        # yalnız uzat: uzun satır kısalmaz
        long_ = evs((0, 9000), text="kısa")
        self.assertEqual(adjust_durations(long_, [0], DurationOptions(cps=15, max_ms=7000)), {})
        ch = adjust_durations(long_, [0], DurationOptions(cps=15, max_ms=7000, only_extend=False))
        self.assertEqual(ch[0]["end"], 700)                      # en az süre

    def test_durations_percent_fixed(self):
        e = evs((0, 1000), (10000, 12000))
        self.assertEqual(adjust_durations(e, [0, 1], DurationOptions(mode="percent", percent=150))[1]["end"], 13000)
        self.assertEqual(adjust_durations(e, [0], DurationOptions(mode="fixed", fixed_ms=250))[0]["end"], 1250)

    def test_bridge(self):
        e = evs((0, 1000), (1100, 2000), (2600, 3000))
        ch = bridge_gaps(e, [0, 1, 2], max_gap_ms=500, min_gap_ms=40)
        self.assertEqual(ch, {0: {"end": 1060}})
        ch = bridge_gaps(e, [0, 1, 2], max_gap_ms=500, min_gap_ms=0, bias=0.5)
        self.assertEqual(ch, {0: {"end": 1050}, 1: {"start": 1050}})

    def test_fps_speed_point(self):
        e = evs((25000, 50000))
        self.assertEqual(change_fps(e, [0], 23.976, 25)[0], {"start": 23980, "end": 47950})
        self.assertEqual(change_speed(e, [0], 104.271)[0], {"start": 23980, "end": 47950})
        e = evs((1000, 2000), (6000, 7000), (11000, 12000))
        ch = point_sync(e, [0, 1, 2], [(1000, 2000), (11000, 21000)])      # eğim 1,9
        self.assertEqual(ch[0], {"start": 2000, "end": 3900})
        self.assertEqual(ch[1], {"start": 11500, "end": 13400})
        self.assertEqual(ch[2]["start"], 21000)
        ch = point_sync(e, [0, 1, 2], [(1000, 1000), (6000, 7000), (11000, 12000)])   # parçalı
        self.assertEqual(ch[1]["start"], 7000)
        self.assertEqual(ch[2]["start"], 12000)
        self.assertEqual(ch[0], {"start": 1000, "end": 2200})      # bitiş ilk parçanın eğimiyle (1,2) uzar
        self.assertEqual(point_sync(e, [0], [(1000, 1500)]), {0: {"start": 1500, "end": 2500}})


class TestStructure(unittest.TestCase):
    def test_merge_short(self):
        e = [E(start=0, end=800, text="Evet."), E(start=900, end=1700, text="Tamam."),
             E(start=1750, end=2500, text="Gidelim o zaman."), E(start=6000, end=7000, text="Ayrı.")]
        out, n = merge_short(e, range(4), MergeOptions(max_gap_ms=250))
        self.assertEqual(n, 2)
        self.assertEqual(len(out), 2)
        self.assertEqual(out[0].text, "Evet. Tamam. Gidelim o zaman.")
        self.assertEqual((out[0].start, out[0].end), (0, 2500))
        e[1] = e[1].replace(actor="Ali")
        out, n = merge_short(e, range(4))
        self.assertEqual(n, 0)                                    # aktör farklı: birleşmez (ve zincir kopar)

    def test_split_long(self):
        t = ("Bunu sana daha önce de söylemiştim ama beni hiç dinlemedin. "
             "Şimdi hepimiz bu karmaşanın ortasında kaldık ve çıkış yolu bulmamız lazım.")
        e = [E(start=0, end=8000, text="{\\an8}" + t), E(start=9000, end=10000, text="Kısa.")]
        out, n = split_long(e, [0, 1])
        self.assertEqual(n, 1)
        self.assertEqual(len(out), 3)
        self.assertTrue(out[0].text.startswith("{\\an8}Bunu sana"))
        self.assertTrue(out[0].text.replace("\\N", " ").endswith("dinlemedin."))
        self.assertTrue(out[1].text.startswith("{\\an8}Şimdi"))
        self.assertEqual(out[0].end, out[1].start)
        self.assertTrue(0 < out[0].end < 8000)
        for ev in out[:2]:
            self.assertTrue(all(n <= 42 for n in cpl_lengths(ev.text)), ev.text)

    def test_sort(self):
        e = [E(start=3000, style="B"), E(start=1000, style="A"), E(start=2000, style="B"), E(start=0, style="Z")]
        out = sort_rows(e, [0, 1, 2], ["style", "start"])
        self.assertEqual([(x.style, x.start) for x in out], [("A", 1000), ("B", 2000), ("B", 3000), ("Z", 0)])
        out = sort_rows(e, [0, 1, 2, 3], ["start"], reverse=True)
        self.assertEqual([x.start for x in out], [3000, 2000, 1000, 0])

    def test_statistics(self):
        e = [E(start=0, end=2000, text="Naruto gel buraya!", actor="Sakura"),
             E(start=2000, end=3000, text="Naruto nerede?"), E(comment=True, text="not")]
        st = statistics(e)
        self.assertEqual((st["lines"], st["dialogue"], st["comments"]), (3, 2, 1))
        self.assertEqual(st["words"], 5)
        self.assertEqual(st["top_words"][0], ("naruto", 2))
        self.assertEqual(st["actors"], [("Sakura", 1)])
        self.assertEqual(st["total_ms"], 3000)


if __name__ == "__main__":
    unittest.main()
