"""Command line interface: conversion, macro running, QC and quick tools."""
from __future__ import annotations
import argparse
import os
import sys

from . import __version__
from .formats import load_any, save_any
from .core import tools


def _parse_range(s: str | None, n: int):
    if not s:
        return None
    out = []
    for part in s.split(","):
        if "-" in part:
            a, b = part.split("-", 1)
            out.extend(range(int(a) - 1, int(b)))
        else:
            out.append(int(part) - 1)
    return [i for i in out if 0 <= i < n]


def _engine(args):
    from .automation import Engine
    inc = [d for d in (args.include or [])]
    for env in ("OSISUB_INCLUDE",):
        if os.environ.get(env):
            inc += os.environ[env].split(os.pathsep)
    return Engine(include_dirs=inc, log_handler=lambda lvl, m: sys.stderr.write(m))


def cmd_convert(a):
    save_any(load_any(a.input), a.output)


def cmd_macros(a):
    eng = _engine(a)
    s = eng.load_script(a.script)
    if s.error:
        print("ERROR:", s.error, file=sys.stderr)
        return 1
    print(f"{s.name} v{s.version} by {s.author}")
    for i, m in enumerate(s.macros):
        print(f"  macro {i}: {m.name} - {m.description}")
    for i, f in enumerate(s.filters):
        print(f"  filter {i}: {f.name} (prio {f.priority}) - {f.description}")
    return 0


def cmd_run(a):
    eng = _engine(a)
    s = eng.load_script(a.script)
    if s.error:
        print("ERROR:", s.error, file=sys.stderr)
        return 1
    doc = load_any(a.input)
    sel = _parse_range(a.select, len(doc.events)) or []
    if a.filter is not None:
        res = eng.run_filter(s.filters[a.filter], doc)
    else:
        m = s.macros[a.macro]
        res = eng.run_macro(m, doc, sel, sel[0] if sel else 0)
    if not res.ok:
        print("FAILED:", res.error, file=sys.stderr)
        return 2
    save_any(res.document, a.output or a.input)
    return 0


def cmd_check(a):
    doc = load_any(a.input)
    issues = tools.check(doc, max_cps=a.cps, max_cpl=a.cpl)
    for i in issues:
        print(f"{i.index + 1:5d} [{i.kind}] {i.message}")
    print(f"{len(issues)} issue(s) in {len(doc.events)} lines")
    return 1 if issues else 0


def cmd_shift(a):
    doc = load_any(a.input)
    tools.shift(doc, a.ms)
    save_any(doc, a.output or a.input)


def cmd_fix(a):
    from .core import assutil
    doc = load_any(a.input)
    if a.mojibake:
        tools.fix_mojibake(doc)
    if a.hi:
        tools.remove_hearing_impaired(doc)
    if a.common:
        tools.fix_common_errors(doc)
    if a.case:
        tools.change_case(doc, a.case)
    if a.overlaps:
        tools.fix_overlaps(doc)
    if a.merge:
        tools.merge_duplicates(doc)
    if a.shift:
        tools.shift(doc, a.shift)
    if a.resample:
        x, y = (int(v) for v in a.resample.lower().split("x"))
        assutil.resample(doc, x, y)
    tools.remove_empty(doc)
    save_any(doc, a.output or a.input)


def cmd_stats(a):
    for k, v in tools.statistics(load_any(a.input)).items():
        print(f"{k}: {v}")


def build_parser():
    p = argparse.ArgumentParser(prog="osisub", description="Osisub subtitle toolkit")
    p.add_argument("--version", action="version", version=__version__)
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("convert"); c.add_argument("input"); c.add_argument("output"); c.set_defaults(fn=cmd_convert)
    m = sub.add_parser("macros"); m.add_argument("script"); m.add_argument("-I", "--include", action="append"); m.set_defaults(fn=cmd_macros)
    r = sub.add_parser("run"); r.add_argument("script"); r.add_argument("input")
    r.add_argument("-o", "--output"); r.add_argument("-m", "--macro", type=int, default=0)
    r.add_argument("-f", "--filter", type=int); r.add_argument("-s", "--select", help="1-based line ranges, e.g. 1-5,9")
    r.add_argument("-I", "--include", action="append"); r.set_defaults(fn=cmd_run)
    k = sub.add_parser("check"); k.add_argument("input"); k.add_argument("--cps", type=float, default=20)
    k.add_argument("--cpl", type=int, default=42); k.set_defaults(fn=cmd_check)
    s = sub.add_parser("shift"); s.add_argument("input"); s.add_argument("ms", type=int)
    s.add_argument("-o", "--output"); s.set_defaults(fn=cmd_shift)
    f = sub.add_parser("fix", help="batch clean-up")
    f.add_argument("input"); f.add_argument("-o", "--output")
    f.add_argument("--mojibake", action="store_true"); f.add_argument("--hi", action="store_true")
    f.add_argument("--common", action="store_true"); f.add_argument("--overlaps", action="store_true")
    f.add_argument("--merge", action="store_true"); f.add_argument("--case", choices=["upper", "lower", "title", "sentence"])
    f.add_argument("--shift", type=int, default=0); f.add_argument("--resample", help="WxH, e.g. 1920x1080")
    f.set_defaults(fn=cmd_fix)
    st = sub.add_parser("stats"); st.add_argument("input"); st.set_defaults(fn=cmd_stats)
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    return args.fn(args) or 0


if __name__ == "__main__":
    sys.exit(main())
