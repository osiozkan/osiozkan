"""Command line interface: batch conversion, QC, timing and Lua macro runner."""
from __future__ import annotations

import argparse
import json
import os
import sys

from . import __version__
from .core import tools, checks
from .core.ass import ASSFile
from .core.formats import load_file, export_text


def _load(path, enc=None) -> ASSFile:
    doc, _fmt, _enc = load_file(path, enc)
    return doc


def _save(doc: ASSFile, out: str, fmt: str | None = None):
    fmt = fmt or os.path.splitext(out)[1].lstrip(".").lower() or "ass"
    data = export_text(doc, fmt)
    with open(out, "w", encoding="utf-8-sig" if fmt in ("ass", "ssa") else "utf-8", newline="") as fh:
        fh.write(data)


def cmd_convert(a):
    doc = _load(a.input, a.encoding)
    _save(doc, a.output, a.format)
    print(f"{a.input} -> {a.output} ({len(doc.events)} events)")


def cmd_check(a):
    doc = _load(a.input, a.encoding)
    s = checks.QCSettings(max_cps=a.max_cps, max_chars_per_line=a.max_chars, min_duration=a.min_dur,
                          max_duration=a.max_dur, min_gap=a.min_gap)
    issues = checks.check(doc, s)
    ev = doc.events
    for i in issues:
        print(f"[{i.severity[:4]}] #{i.index + 1} {i.code}: {i.message} | {ev[i.index].text[:50]!r}")
    print(f"{len(issues)} issue(s) in {len(ev)} events")
    return 1 if any(i.severity == "error" for i in issues) else 0


def cmd_shift(a):
    doc = _load(a.input, a.encoding)
    n = tools.shift(doc.events, a.ms, a.which)
    _save(doc, a.output or a.input)
    print(f"shifted {n} events by {a.ms} ms")


def cmd_fix(a):
    doc = _load(a.input, a.encoding)
    ev = doc.events
    report = {}
    if a.bridge is not None:
        report["bridge"] = tools.bridge_gaps(ev, a.bridge, a.min_gap)
    report["overlaps"] = tools.fix_overlaps(ev, a.min_gap)
    report["common_errors"] = tools.fix_common_errors(ev)
    report["duplicates"] = tools.merge_duplicates(doc)
    _save(doc, a.output or a.input)
    print(json.dumps(report))


def cmd_lua(a):
    from .automation import AutomationScript, Host, HeadlessUI, ScriptError, CancelledError, ProjectInfo
    s = AutomationScript(a.script, a.include or [])
    if s.error:
        print("load error:", s.error, file=sys.stderr)
        return 2
    if a.list or not a.macro:
        print(f"{s.name} v{s.version} by {s.author}: {s.description}")
        for m in s.macros:
            print("  macro:", m.name, "-", m.description)
        for f in s.filters:
            print("  filter:", f.name, "-", f.description)
        return 0
    doc = _load(a.input, a.encoding)
    answers = {}
    for kv in a.set or []:
        k, _, v = kv.partition("=")
        answers[k] = {"true": True, "false": False}.get(v.lower(), int(v) if v.lstrip("-").isdigit() else v)
    ui = HeadlessUI(answers, a.button)
    ui.log = lambda lvl, msg: print(msg, end="", file=sys.stderr)
    proj = ProjectInfo(fps=a.fps, script_path=os.path.abspath(a.input))
    host = Host(doc, [], ui=ui, project=proj)
    ev_idx = host.event_indices()
    if a.lines:
        lo, _, hi = a.lines.partition("-")
        lo, hi = int(lo), int(hi or lo)
        host.selection = [ev_idx[i - 1] for i in range(lo, hi + 1) if 0 < i <= len(ev_idx)]
    else:
        host.selection = ev_idx
    host.active = host.selection[0] if host.selection else 0
    macro = next((m for m in s.macros if m.name == a.macro or m.name.endswith("/" + a.macro)), None)
    if not macro:
        print("macro not found:", a.macro, file=sys.stderr)
        return 2
    try:
        ok, msg = macro.validate(host)
        if not ok:
            print("validation failed:", msg, file=sys.stderr)
            return 3
        macro.run(host)
    except CancelledError:
        print("cancelled by script", file=sys.stderr)
        return 4
    except ScriptError as e:
        print("script error:", e, file=sys.stderr)
        return 5
    _save(host.doc, a.output or a.input)
    print(f"done; {len(host.doc.events)} events")
    return 0


def cmd_info(a):
    from . import media
    i = media.probe(a.input)
    print(json.dumps(i.__dict__, indent=2, ensure_ascii=False))


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="subforge", description="SubForge - anime subtitle studio")
    p.add_argument("--version", action="version", version=f"SubForge {__version__}")
    sp = p.add_subparsers(dest="cmd")
    g = sp.add_parser("gui", help="launch the editor (default)")
    g.add_argument("file", nargs="?")
    c = sp.add_parser("convert")
    c.add_argument("input"); c.add_argument("output"); c.add_argument("--format"); c.add_argument("--encoding")
    c.set_defaults(fn=cmd_convert)
    k = sp.add_parser("check", help="quality control report")
    k.add_argument("input"); k.add_argument("--encoding")
    k.add_argument("--max-cps", type=float, default=20); k.add_argument("--max-chars", type=int, default=42)
    k.add_argument("--min-dur", type=int, default=700); k.add_argument("--max-dur", type=int, default=8000)
    k.add_argument("--min-gap", type=int, default=0)
    k.set_defaults(fn=cmd_check)
    s = sp.add_parser("shift")
    s.add_argument("input"); s.add_argument("ms", type=int); s.add_argument("-o", "--output")
    s.add_argument("--which", choices=["both", "start", "end"], default="both"); s.add_argument("--encoding")
    s.set_defaults(fn=cmd_shift)
    f = sp.add_parser("fix", help="bridge gaps, fix overlaps/common errors, merge duplicates")
    f.add_argument("input"); f.add_argument("-o", "--output"); f.add_argument("--bridge", type=int)
    f.add_argument("--min-gap", type=int, default=0); f.add_argument("--encoding")
    f.set_defaults(fn=cmd_fix)
    l = sp.add_parser("lua", help="run an Aegisub Lua macro headlessly")
    l.add_argument("script"); l.add_argument("input", nargs="?"); l.add_argument("-m", "--macro")
    l.add_argument("-o", "--output"); l.add_argument("--list", action="store_true")
    l.add_argument("--lines", help="selection, e.g. 3-10 (event numbers)")
    l.add_argument("--set", action="append", help="dialog answer key=value (repeatable)")
    l.add_argument("--button"); l.add_argument("--fps", type=float); l.add_argument("--include", action="append")
    l.add_argument("--encoding")
    l.set_defaults(fn=cmd_lua)
    i = sp.add_parser("info"); i.add_argument("input"); i.set_defaults(fn=cmd_info)
    a = p.parse_args(argv)
    if a.cmd in (None, "gui"):
        from .gui.app import run_gui
        return run_gui(getattr(a, "file", None))
    return a.fn(a) or 0


if __name__ == "__main__":
    sys.exit(main())
