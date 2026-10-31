"""Komut satırı: yorusub convert|qc|shift|fix|lua|gui"""
import argparse
import sys

from .core import timing
from .core.formats import load_any, save_any
from .core.qc import check


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="yorusub", description="YoruSub – anime altyazı stüdyosu")
    sub = p.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("convert", help="ASS/SRT/VTT dönüştür"); c.add_argument("src"); c.add_argument("dst")
    q = sub.add_parser("qc", help="kalite kontrol"); q.add_argument("src")
    q.add_argument("--cps", type=float, default=20); q.add_argument("--cpl", type=int, default=42)
    s = sub.add_parser("shift", help="zaman kaydır (ms)"); s.add_argument("src"); s.add_argument("dst"); s.add_argument("ms", type=int)
    f = sub.add_parser("fix", help="çakışma ve kısa süre düzelt"); f.add_argument("src"); f.add_argument("dst")
    f.add_argument("--min-ms", type=int, default=500); f.add_argument("--gap", type=int, default=0)
    l = sub.add_parser("lua", help="Aegisub Lua betiği çalıştır")
    l.add_argument("script"); l.add_argument("src"); l.add_argument("dst")
    l.add_argument("--macro", help="makro adı (yoksa listele)")
    l.add_argument("--select", help="virgülle ayrılmış satır numaraları (1 tabanlı); boşsa hepsi")
    sub.add_parser("gui", help="arayüzü aç").add_argument("file", nargs="?")

    a = p.parse_args(argv)
    if a.cmd == "convert":
        save_any(load_any(a.src), a.dst)
    elif a.cmd == "qc":
        ass = load_any(a.src)
        issues = check(ass, max_cps=a.cps, max_cpl=a.cpl)
        for i in issues:
            print(f"#{i.index + 1:<5} [{i.severity}] {i.kind}: {i.message}")
        print(f"{len(issues)} sorun, {len(ass.events)} satır")
        return 1 if any(i.severity == "error" for i in issues) else 0
    elif a.cmd == "shift":
        ass = load_any(a.src); timing.shift(ass.events, a.ms); save_any(ass, a.dst)
    elif a.cmd == "fix":
        ass = load_any(a.src)
        n1 = timing.fix_overlaps(ass.events, a.gap); n2 = timing.enforce_min_duration(ass.events, a.min_ms)
        save_any(ass, a.dst); print(f"{n1} çakışma, {n2} kısa süre düzeltildi")
    elif a.cmd == "lua":
        from .lua import LuaRuntime
        rt = LuaRuntime()
        macros = rt.load_script(a.script)
        if not a.macro:
            for m in macros:
                print(f"{m.kind}: {m.name} — {m.description}")
            return 0
        m = next((m for m in macros if m.name == a.macro), None)
        if not m:
            print("makro bulunamadı", file=sys.stderr); return 2
        ass = load_any(a.src)
        sel = [int(x) - 1 for x in a.select.split(",")] if a.select else list(range(len(ass.events)))
        out, _, _ = rt.run(m, ass, sel)
        save_any(out, a.dst)
    elif a.cmd == "gui":
        from .gui.app import run
        return run(a.file)
    return 0


if __name__ == "__main__":
    sys.exit(main())
