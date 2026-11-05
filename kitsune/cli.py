"""Headless CLI: run Aegisub Lua macros, convert formats, lint."""
import argparse, sys
from .core.ass import Script
from .core import srt
from .core.checks import lint
from .lua.runtime import AutomationHost

def load_any(path):
    p = str(path).lower()
    if p.endswith((".srt", ".vtt")): return srt.load_srt(path)
    return Script.load(path)

def main(argv=None):
    ap = argparse.ArgumentParser("kitsune")
    sub = ap.add_subparsers(dest="cmd")
    r = sub.add_parser("run", help="run a Lua macro headlessly")
    r.add_argument("subtitle"); r.add_argument("lua"); r.add_argument("--macro", default=None)
    r.add_argument("--select", default="", help="comma-separated 1-based indices"); r.add_argument("-o", "--out")
    c = sub.add_parser("convert"); c.add_argument("src"); c.add_argument("dst")
    l = sub.add_parser("lint"); l.add_argument("subtitle")
    sub.add_parser("gui")
    a = ap.parse_args(argv)
    if a.cmd == "run":
        h = AutomationHost(load_any(a.subtitle)); s = h.load_script(a.lua)
        if s.error: print(s.error, file=sys.stderr); return 2
        ms = [m for m in h.macros() if a.macro in (None, m.name)]
        if not ms: print("macro not found; available:", [m.name for m in h.macros()], file=sys.stderr); return 2
        sel = [int(x) for x in a.select.split(",") if x]
        h.run(ms[0], sel)
        print("\n".join(h.logs))
        h.script.save(a.out or a.subtitle); return 0
    if a.cmd == "convert":
        sc = load_any(a.src)
        if a.dst.lower().endswith((".srt", ".vtt")):
            open(a.dst, "w", encoding="utf-8").write(srt.dump_srt(sc, vtt=a.dst.lower().endswith(".vtt")))
        else: sc.save(a.dst)
        return 0
    if a.cmd == "lint":
        for i in lint(load_any(a.subtitle)): print(i)
        return 0
    from .gui.app import run_gui
    return run_gui()
