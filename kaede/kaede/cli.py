"""kaede-cli: headless operations (convert, QC, run Lua macros, shift)."""
from __future__ import annotations
import argparse
import sys

from .core import formats, tools
from .core.timeutil import FrameRate


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="kaede-cli", description="Kaede headless subtitle tools")
    sub = ap.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("convert", help="convert between ass/srt/vtt/sbv/txt")
    c.add_argument("input"); c.add_argument("output"); c.add_argument("--encoding")
    c.add_argument("--no-bom", action="store_true")

    q = sub.add_parser("qc", help="quality check")
    q.add_argument("input"); q.add_argument("--cps", type=float, default=20)
    q.add_argument("--cpl", type=int, default=42); q.add_argument("--lines", type=int, default=2)

    s = sub.add_parser("shift", help="shift times by ms")
    s.add_argument("input"); s.add_argument("output"); s.add_argument("ms", type=int)

    b = sub.add_parser("batch", help="convert/QC every subtitle in a folder")
    b.add_argument("folder"); b.add_argument("--to", choices=["ass", "srt", "vtt", "sbv", "txt"])
    b.add_argument("--qc", action="store_true"); b.add_argument("--out", help="output folder (default: next to input)")
    b.add_argument("--fix", action="store_true", help="apply 'fix common errors' + normalize before writing")

    r = sub.add_parser("run", help="run a Lua macro")
    r.add_argument("input"); r.add_argument("script"); r.add_argument("output")
    r.add_argument("--macro", help="macro name (default: first)")
    r.add_argument("--select", help="comma separated 0-based event indexes (default: all)")
    r.add_argument("--list", action="store_true", help="list macros in script and exit")

    a = ap.parse_args(argv)
    if a.cmd == "convert":
        d = formats.load_document(a.input, a.encoding)
        formats.save_document(d, a.output, bom=not a.no_bom)
        print(f"{len(d.events)} lines -> {a.output}")
    elif a.cmd == "qc":
        d = formats.load_document(a.input)
        iss = tools.run_qc(d, tools.QCConfig(max_cps=a.cps, max_cpl=a.cpl, max_lines=a.lines))
        for i in iss:
            print(f"#{i.index + 1:>4} [{i.severity}] {i.code}: {i.message}")
        print(f"{len(iss)} issue(s)")
        return 1 if any(i.severity == "error" for i in iss) else 0
    elif a.cmd == "shift":
        d = formats.load_document(a.input); tools.shift_times(d, a.ms)
        formats.save_document(d, a.output)
    elif a.cmd == "batch":
        import os
        total = bad = 0
        for fn in sorted(os.listdir(a.folder)):
            if not fn.lower().endswith((".ass", ".ssa", ".srt", ".vtt", ".sbv")): continue
            p = os.path.join(a.folder, fn); d = formats.load_document(p); total += 1
            if a.fix: tools.fix_common(d)
            if a.qc:
                iss = tools.run_qc(d); bad += sum(1 for i in iss if i.severity == "error")
                print(f"{fn}: {len(iss)} sorun")
            if a.to:
                outd = a.out or a.folder; os.makedirs(outd, exist_ok=True)
                formats.save_document(d, os.path.join(outd, os.path.splitext(fn)[0] + "." + a.to))
        print(f"{total} dosya işlendi")
        return 1 if bad else 0
    elif a.cmd == "run":
        from .lua.engine import Engine, AutomationError, Cancelled
        eng = Engine()
        sc = eng.load_script(a.script)
        if sc.error:
            print(sc.error, file=sys.stderr); return 2
        if a.list:
            for m in sc.macros: print("macro:", m.name)
            for f in sc.filters: print("filter:", f.name)
            return 0
        d = formats.load_document(a.input)
        allm = sc.macros + sc.filters
        m = next((x for x in allm if not a.macro or x.name == a.macro), None)
        if not m:
            print("macro not found", file=sys.stderr); return 2
        sel = [int(x) for x in a.select.split(",")] if a.select else list(range(len(d.events)))
        try:
            res = eng.run_filter(m, d) if m.kind == "filter" else eng.run_macro(m, d, sel, sel[0] if sel else 0)
        except Cancelled:
            print("cancelled"); return 1
        except AutomationError as e:
            print(f"error: {e}", file=sys.stderr); return 2
        for l in res.logs: print(l, end="" if l.endswith("\n") else "\n")
        formats.save_document(res.doc, a.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
