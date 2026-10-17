"""Komut satırı.

  python -m osisub [dosya.ass]                 arayüzü aç
  python -m osisub doctor                      ortam denetimi
  python -m osisub roundtrip dosya.ass         bayt korumayı sına (aç-kaydet = aynı mı?)
  python -m osisub macros betik.lua            betikteki makroları listele
  python -m osisub macro betik.lua "Ad" girdi.ass [-o çıktı.ass] [--sel 1,2] [--active 1]
                                               makroyu arayüzsüz çalıştır
                                               (--sel/--active: 1 tabanlı olay satır numaraları)
  python -m osisub aligncompare kaynak.srt aday.ass [--marked qc.ass] [--csv qc.csv] [--only-flags] ...
                                               kaynak ↔ aday karşılaştırması (aligncompare ile aynı seçenekler)
"""

from __future__ import annotations

import argparse
import os
import sys


def _doctor() -> int:
    ok = True

    def line(name, good, detail=""):
        nonlocal ok
        ok = ok and good
        print(f"  [{'TAMAM' if good else 'EKSİK'}] {name}" + (f" — {detail}" if detail else ""))

    print(f"Python {sys.version.split()[0]} ({sys.executable})")
    try:
        import PySide6
        line("PySide6 (arayüz)", True, PySide6.__version__)
    except ImportError:
        line("PySide6 (arayüz)", False, "pip install PySide6")
    try:
        from .automation.runtime import LUA_FLAVOR
        line("lupa (Lua motoru)", "JIT yok" not in LUA_FLAVOR, LUA_FLAVOR)
    except ImportError:
        line("lupa (Lua motoru)", False, "pip install lupa")
    try:
        import regex
        line("regex (aegisub.re)", True, regex.__version__)
    except ImportError:
        line("regex (aegisub.re)", True, "yok — standart 're' kullanılacak (pip install regex önerilir)")
    try:
        import numpy
        line("numpy (ses/dalga formu)", True, numpy.__version__)
    except ImportError:
        line("numpy (ses/dalga formu)", False, "pip install numpy")
    from .media.probe import tool
    for t in ("ffmpeg", "ffprobe"):
        p = tool(t)
        line(f"{t} (video/ses okuma)", bool(p), p or "indir: gyan.dev ffmpeg essentials; bin klasörüne koy")
    try:
        import av
        line("PyAV (hızlı kare dizini + kare çözme)", True, av.__version__)
    except ImportError:
        line("PyAV (hızlı kare dizini + kare çözme)", True, "yok — ffprobe ile daha yavaş yedek yol")
    try:
        import spylls  # noqa: F401
        from .core.spell import find_dictionary
        base = find_dictionary("tr_TR")
        line("Yazım denetimi (spylls + tr_TR sözlüğü)", True,
             base or "sözlük yok — uygulamada Araçlar > Türkçe sözlüğü indir")
    except ImportError:
        line("Yazım denetimi (spylls)", True, "yok — pip install spylls (isteğe bağlı)")
    try:
        import cv2
        line("OpenCV (hareket takibi)", True, cv2.__version__)
    except ImportError:
        line("OpenCV (hareket takibi)", True, "yok — pip install opencv-python (hareket takibi kapalı)")
    from .core.asr import available as asr_available
    ok_asr, why = asr_available()
    from .paths import frozen
    line("Whisper (konuşmayı yazıya dökme, isteğe bağlı)", True,
         "hazır" if ok_asr else ("paketli .exe'de yok (~1 GB); kaynak kurulumla kullanılabilir" if frozen()
                                 else f"{why} — KURULUM_WHISPER.bat"))
    from .ui.video import load_mpv, mpv_error
    m = load_mpv()
    line("libmpv + python-mpv (video oynatma, canlı altyazı)", m is not None,
         "hazır" if m else f"{mpv_error()} — libmpv-2.dll'i OsiSub ya da OsiSub/bin klasörüne koy; pip install mpv")
    from . import config
    print("Kullanıcı klasörü:", config.user_dir())
    a, i = config.aegisub_dirs()
    print("Bulunan Aegisub klasörleri:", (a + i) or "yok")
    return 0 if ok else 1


def _aligncompare(argv) -> int:
    """aligncompare.py ile aynı komut satırı: önce KAYNAK, sonra ADAY."""
    p = argparse.ArgumentParser(prog="osisub aligncompare",
                                description="Kaynak (EN) ↔ aday (TR) hizalama ve denetim raporu")
    p.add_argument("kaynak", help="referans altyazı (.ass / .srt / .vtt)")
    p.add_argument("aday", help="denetlenecek çeviri (.ass)")
    p.add_argument("-o", dest="html", help="HTML rapor (vars. <aday>_karsilastirma.html)")
    p.add_argument("--csv", help="CSV çıktısı (; ayraçlı, Excel için)")
    p.add_argument("--marked", help="işaretli aday kopyası (.ass)")
    p.add_argument("--only-flags", action="store_true", help="raporda yalnız bayraklı satırlar")
    p.add_argument("--shift", default="auto", help='küresel kayma: "auto" ya da saniye (ör. 2.35)')
    p.add_argument("--shift-range", type=float, default=15.0, help="otomatik arama aralığı ± sn (vars. 15)")
    p.add_argument("--ratio-min", type=float, default=0.45)
    p.add_argument("--ratio-max", type=float, default=2.2)
    p.add_argument("--sim", type=float, default=0.85, help='"İNGİLİZCE?" benzerlik eşiği')
    p.add_argument("--max-start-diff", type=float, default=0.5, help="ZAMAN bayrağı eşiği (sn)")
    p.add_argument("--min-overlap", type=float, default=0.25)
    p.add_argument("--min-len", type=int, default=10, help="UZUNLUK için en kısa kaynak (karakter)")
    p.add_argument("--skip-styles-src", default="", metavar="RX")
    p.add_argument("--skip-styles-cand", default="", metavar="RX")
    p.add_argument("--include-typeset", action="store_true")
    a = p.parse_args(argv)
    try:
        sys.stdout.reconfigure(errors="replace")
    except (AttributeError, ValueError):
        pass
    from .core.compare import CompareOptions, compare, marked_document, summary_lines, to_csv, to_html
    from .core.formats import load_any
    if a.shift != "auto":
        try:
            float(a.shift.replace(",", "."))
        except ValueError:
            p.error('--shift "auto" ya da sayı olmalı')
    opts = CompareOptions(shift="auto" if a.shift == "auto" else float(a.shift.replace(",", ".")),
                          shift_range=a.shift_range, ratio_min=a.ratio_min, ratio_max=a.ratio_max, sim=a.sim,
                          max_start_diff=a.max_start_diff, min_overlap=a.min_overlap, min_len=a.min_len,
                          skip_styles_src=a.skip_styles_src, skip_styles_cand=a.skip_styles_cand,
                          include_typeset=a.include_typeset)
    src_doc = load_any(a.kaynak)
    cand_doc = load_any(a.aday)
    res = compare(src_doc.events, cand_doc.events, opts)
    for ln in summary_lines(res, os.path.basename(a.kaynak), os.path.basename(a.aday)):
        print(ln)
    html_path = a.html or os.path.splitext(a.aday)[0] + "_karsilastirma.html"
    with open(html_path, "w", encoding="utf-8") as f:
        f.write(to_html(res, a.kaynak, a.aday, only_flags=a.only_flags))
    print(f"HTML rapor: {html_path}")
    if a.csv:
        with open(a.csv, "w", encoding="utf-8", newline="") as f:
            f.write(to_csv(res, only_flags=a.only_flags))
        print(f"CSV: {a.csv}")
    if a.marked:
        if os.path.abspath(a.marked) == os.path.abspath(a.aday):
            print("İşaretli kopya adayın üstüne yazılamaz; başka bir ad verin.", file=sys.stderr)
            return 2
        marked_document(cand_doc, res).save(a.marked)
        print(f"İşaretli kopya: {a.marked}  (bayraklı satırlarda {{QC: ...}}, EKSİK satırlar sonda Comment)")
    return 0


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv and argv[0] in ("--version", "-V", "version"):
        from . import __version__
        print(f"OsiSub {__version__}")
        return 0
    if argv and argv[0] in ("aligncompare", "compare"):
        return _aligncompare(argv[1:])
    cmds = {"doctor", "roundtrip", "macros", "macro"}
    if not argv or argv[0] not in cmds:
        from .ui.app import main as gui_main
        return gui_main([sys.argv[0]] + argv)

    p = argparse.ArgumentParser(prog="osisub")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("doctor")
    rt = sub.add_parser("roundtrip")
    rt.add_argument("files", nargs="+")
    ml = sub.add_parser("macros")
    ml.add_argument("script")
    mr = sub.add_parser("macro")
    mr.add_argument("script")
    mr.add_argument("name")
    mr.add_argument("input")
    mr.add_argument("-o", "--output")
    mr.add_argument("--sel", default="")
    mr.add_argument("--active", type=int, default=0)
    mr.add_argument("--include", action="append", default=[])
    a = p.parse_args(argv)

    if a.cmd == "doctor":
        return _doctor()

    if a.cmd == "roundtrip":
        from .core.assfile import AssDocument
        bad = 0
        for f in a.files:
            data = open(f, "rb").read()
            out = AssDocument.from_bytes(data).to_bytes()
            same = out == data
            bad += not same
            print(f"{'AYNI ' if same else 'FARKLI'}  {f}")
        return 1 if bad else 0

    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from . import config
    from .automation.host import HeadlessHost
    from .automation.runtime import LuaScript
    from .core.assfile import AssDocument
    cfg = config.Config()
    host = HeadlessHost(echo=True)
    sc = LuaScript(a.script, cfg.include_dirs() + a.include if a.cmd == "macro" else cfg.include_dirs(), host)
    if not sc.load():
        print("Betik yüklenemedi:\n" + (sc.error or ""), file=sys.stderr)
        return 2
    if a.cmd == "macros":
        print(f"{sc.name} {sc.version} — {sc.author}")
        for m in sc.macros:
            print(f"  {m.name}" + (f"  ({m.description})" if m.description else ""))
        return 0
    macro = sc.macros and next((m for m in sc.macros if m.name == a.name or m.name.split('/')[-1] == a.name), None)
    if not macro:
        print(f"Makro bulunamadı: {a.name}", file=sys.stderr)
        return 2
    doc = AssDocument.load(a.input)
    sel = [int(x) - 1 for x in a.sel.split(",") if x.strip()] or [max(0, a.active - 1)]
    r = sc.run_macro(macro, doc, sel, max(0, a.active - 1) if a.active else sel[0])
    print(f"durum: {r.status}  değişti: {r.changed}  süre: {r.seconds * 1000:.1f} ms", file=sys.stderr)
    if r.status == "error":
        print(r.error, file=sys.stderr)
        return 3
    if r.status == "ok":
        doc.save(a.output or a.input)
    return 0


if __name__ == "__main__":
    sys.exit(main())
