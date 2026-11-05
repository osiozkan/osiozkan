"""Aegisub-compatible Lua automation runtime (LuaJIT via lupa)."""
from __future__ import annotations
import os, re, traceback
from dataclasses import dataclass
from pathlib import Path
from lupa import luajit21 as _lj
from ..core.ass import Script, Style, Event, Info, Raw
from ..core.timecode import ms_to_ass

HERE = Path(__file__).parent
_STYLE_MAP = {"color1": "color1", "color2": "color2", "color3": "color3", "color4": "color4"}

class Cancelled(Exception): pass

@dataclass
class Macro:
    name: str; description: str; fn: object; validate: object; is_active: object
    script: "LuaScript"; kind: str = "macro"

class LuaScript:
    def __init__(self, host: "AutomationHost", path: str):
        self.host, self.path = host, str(path)
        self.name = Path(path).stem; self.description = ""; self.author = ""; self.version = ""
        self.macros: list[Macro] = []; self.error = None
        self.lua = _lj.LuaRuntime(unpack_returned_tuples=True, register_eval=False, encoding="utf-8")
        self._setup()

    # ---- conversion helpers
    def _line_to_lua(self, e):
        L = self.lua
        t = L.table()
        if e.kind == "event":
            t.class_ = None
            d = dict(comment=e.comment, layer=e.layer, start_time=e.start, end_time=e.end, style=e.style,
                     actor=e.actor, margin_l=e.margin_l, margin_r=e.margin_r, margin_t=e.margin_t,
                     margin_b=e.margin_t, effect=e.effect, text=e.text, section="[Events]",
                     extra=L.table_from(e.extra))
            d["class"] = "dialogue"
            d["raw"] = ("Comment: " if e.comment else "Dialogue: ") + f"{e.layer},{ms_to_ass(e.start)},{ms_to_ass(e.end)},{e.style},{e.actor},{e.margin_l},{e.margin_r},{e.margin_t},{e.effect},{e.text}"
        elif e.kind == "style":
            d = dict(name=e.name, fontname=e.fontname, fontsize=e.fontsize, color1=e.color1, color2=e.color2,
                     color3=e.color3, color4=e.color4, bold=e.bold, italic=e.italic, underline=e.underline,
                     strikeout=e.strikeout, scale_x=e.scale_x, scale_y=e.scale_y, spacing=e.spacing, angle=e.angle,
                     borderstyle=e.borderstyle, outline=e.outline, shadow=e.shadow, align=e.align,
                     margin_l=e.margin_l, margin_r=e.margin_r, margin_t=e.margin_t, margin_b=e.margin_t,
                     margin_v=e.margin_t, encoding=e.encoding, section="[V4+ Styles]")
            d["class"] = "style"
        elif e.kind == "info":
            d = dict(key=e.key, value=e.value, section="[Script Info]"); d["class"] = "info"
        else:
            d = dict(raw=e.line, section=f"[{e.section}]"); d["class"] = "unknown"
        for k, v in d.items():
            t[k] = v
        return t

    def _lua_to_entry(self, t):
        g = lambda k, dflt=None: (t[k] if t[k] is not None else dflt)
        cls = t["class"]
        if cls == "dialogue":
            return Event(comment=bool(g("comment", False)), layer=int(g("layer", 0)), start=int(g("start_time", 0)),
                         end=int(g("end_time", 0)), style=str(g("style", "Default")), actor=str(g("actor", "")),
                         margin_l=int(g("margin_l", 0)), margin_r=int(g("margin_r", 0)),
                         margin_t=int(g("margin_t", g("margin_b", 0))), effect=str(g("effect", "")),
                         text=str(g("text", "")))
        if cls == "style":
            return Style(name=str(g("name", "Default")), fontname=str(g("fontname", "Arial")),
                         fontsize=g("fontsize", 48), color1=g("color1", "&H00FFFFFF"), color2=g("color2", "&H000000FF"),
                         color3=g("color3", "&H00000000"), color4=g("color4", "&H00000000"),
                         bold=bool(g("bold", False)), italic=bool(g("italic", False)),
                         underline=bool(g("underline", False)), strikeout=bool(g("strikeout", False)),
                         scale_x=g("scale_x", 100), scale_y=g("scale_y", 100), spacing=g("spacing", 0),
                         angle=g("angle", 0), borderstyle=int(g("borderstyle", 1)), outline=g("outline", 2),
                         shadow=g("shadow", 2), align=int(g("align", 2)), margin_l=int(g("margin_l", 10)),
                         margin_r=int(g("margin_r", 10)), margin_t=int(g("margin_t", g("margin_b", 10))),
                         encoding=int(g("encoding", 1)))
        if cls == "info":
            return Info(str(g("key", "")), str(g("value", "")))
        return Raw("unknown", str(g("raw", "")))

    # ---- setup
    def _setup(self):
        L, host = self.lua, self.host
        g = L.globals()
        # package path for include/require
        paths = ";".join(f"{p}/?.lua" for p in [str(HERE), *host.include_dirs, str(Path(self.path).parent)])
        g.package.path = paths + ";" + g.package.path
        k = L.table()
        k.get = lambda i: self._line_to_lua(host.script.entries[int(i) - 1])
        def _set(i, t): host.script.entries[int(i) - 1] = self._lua_to_entry(t)
        k.set = _set
        k.count = lambda: len(host.script.entries)
        def _ins(i, t): host.script.entries.insert(int(i) - 1, self._lua_to_entry(t))
        k.insert = _ins
        k.delete = lambda i: host.script.entries.pop(int(i) - 1)
        g["__k"] = k
        L.execute((HERE / "prelude.lua").read_text(encoding="utf-8"))
        a = L.table()
        a.register_macro = self._register_macro
        a.register_filter = self._register_filter
        a.log = lambda *args: host.log(" ".join(str(x) for x in args) if len(args) != 2 or not isinstance(args[0], (int, float)) else str(args[1]))
        dbg = L.table()
        dbg.out = lambda *args: host.log(self._fmt_dbg(args))
        a.debug = dbg
        prog = L.table()
        prog.set = lambda v: host.progress(v=v)
        prog.task = lambda s: host.progress(task=s)
        prog.title = lambda s: host.progress(title=s)
        prog.is_cancelled = lambda: host.cancelled
        a.progress = prog
        def _cancel(*_): raise Cancelled()
        a.cancel = _cancel
        dlg = L.table()
        dlg.display = lambda dialog, buttons=None, ids=None: host.show_dialog(self, dialog, buttons, ids)
        dlg.open = lambda *x: None
        dlg.save = lambda *x: None
        a.dialog = dlg
        def _extents(style, text):
            return host.text_extents(style, text)
        a.text_extents = _extents
        a.frame_from_ms = lambda ms: None if host.fps is None else int(ms * host.fps / 1000)
        a.ms_from_frame = lambda f: None if host.fps is None else int(round(f * 1000 / host.fps))
        a.video_size = lambda: (host.script.res + (1, ""))  if host.video_loaded else None
        a.keyframes = lambda: L.table_from(host.keyframes)
        a.decode_path = lambda p: str(p).replace("?script", str(Path(self.path).parent)).replace("?data", str(HERE)).replace("?user", str(Path.home()))
        a.cancel_ = None
        a.gettext = lambda s: s
        a.lua_automation_version = 4
        a.project_properties = lambda: L.table_from({"video_file": "", "audio_file": "", "automation_scripts": "", "timecodes_file": "", "keyframes_file": "", "video_position": 0, "ar_mode": 0, "ar_value": 0, "scroll_position": 0, "active_row": 0, "video_zoom": 1})
        g.aegisub = a
        g.include = lambda name: g.require(Path(name).stem if name.endswith(".lua") else name)
        g.unicode = self._unicode_module()
        g.re = self._re_module()
        L.execute("""
          local old_require = require
          function require(n) return old_require((n:gsub('%.lua$',''))) end
          function aegisub.cancel() __cancel() end
          function tr(s) return s end
          function _G.unpack_args(...) return ... end
          table.unpack = table.unpack or unpack
          -- Aegisub-style clipboard stub
          clipboard = { get = function() return __clip_get() end, set = function(s) __clip_set(s) end }
          aegisub.clipboard = clipboard
        """)
        g["__cancel"] = _cancel
        g["__clip_get"] = lambda: host.clipboard
        def _cs(s): host.clipboard = s; return True
        g["__clip_set"] = _cs

    @staticmethod
    def _fmt_dbg(args):
        if len(args) >= 2 and isinstance(args[0], (int, float)): return str(args[1])
        return " ".join(str(a) for a in args)

    def _unicode_module(self):
        L = self.lua; u = L.table()
        u.len = lambda s: len(s)
        u.codepoint = lambda s: ord(s[0]) if s else 0
        u.chars = lambda s: L.eval("function(t) local i=0 return function() i=i+1 return t[i] end end")(L.table_from(list(s)))
        u.sub = lambda s, i, j=None: s[max(i,1)-1: (j if j is not None else len(s))]
        u.upper = lambda s: s.upper(); u.lower = lambda s: s.lower(); u.title = lambda s: s.title()
        return u

    def _re_module(self):
        L = self.lua; r = L.table()
        def conv(pat, flags=0): return re.compile(pat, flags)
        def find(s, pat, *a):
            m = conv(pat).search(s)
            return None if not m else (m.start() + 1, m.end(), *[x or "" for x in m.groups()])
        def match(s, pat):
            m = conv(pat).search(s)
            return None if not m else L.table_from([L.table_from({"str": m.group(0), "first": m.start() + 1, "last": m.end()})] + [L.table_from({"str": x or "", "first": 0, "last": 0}) for x in m.groups()])
        r.find = find; r.match = match
        r.sub = lambda s, pat, rep, count=0: re.sub(pat, re.sub(r"\\(\d)", r"\\g<\1>", rep), s, count=0 if not count else int(count))
        r.gsub = lambda s, pat, rep: re.sub(pat, re.sub(r"\\(\d)", r"\\g<\1>", rep), s)
        r.split = lambda s, pat: L.table_from(re.split(pat, s))
        return r

    def _register_macro(self, name, desc, fn, validate=None, is_active=None):
        self.macros.append(Macro(str(name), str(desc), fn, validate, is_active, self))

    def _register_filter(self, name, desc, prio, fn, cfg=None):
        self.macros.append(Macro(str(name), str(desc), fn, None, None, self, kind="filter"))

    def load(self):
        g = self.lua.globals()
        try:
            src = Path(self.path).read_text(encoding="utf-8-sig", errors="replace")
            self.lua.execute(src)
            self.name = g.script_name or self.name
            self.description = g.script_description or ""
            self.author = g.script_author or ""; self.version = g.script_version or ""
        except Exception as e:
            self.error = f"{type(e).__name__}: {e}"
        return self

class AutomationHost:
    """Owns the Script being edited and executes macros against it."""
    def __init__(self, script: Script | None = None):
        self.script = script or Script()
        self.scripts: list[LuaScript] = []
        self.include_dirs: list[str] = []
        self.fps = None; self.video_loaded = False; self.keyframes: list[int] = []
        self.clipboard = ""; self.cancelled = False
        self.logs: list[str] = []
        self.dialog_handler = None   # fn(dialog:list[dict], buttons, ids) -> (button, result)
        self.text_measure = None     # fn(style_dict, text) -> (w,h,desc,ext)
        self.on_progress = None

    def log(self, s): self.logs.append(s)
    def progress(self, **kw):
        if self.on_progress: self.on_progress(**kw)

    def text_extents(self, style, text):
        fs = float(style["fontsize"] or 40)
        if self.text_measure:
            return self.text_measure({k: style[k] for k in ("fontname", "fontsize", "bold", "italic", "scale_x", "scale_y", "spacing")}, text)
        w = sum(fs * (1.0 if ord(c) > 0x2E80 else 0.52) for c in text) * float(style["scale_x"] or 100) / 100 + float(style["spacing"] or 0) * len(text)
        return w, fs * 1.2 * float(style["scale_y"] or 100) / 100, fs * 0.22, 0

    def show_dialog(self, ls: LuaScript, dialog, buttons, ids):
        items = [dict(dialog[i]) for i in range(1, len(list(dialog.keys())) + 1)] if dialog is not None else []
        btns = [buttons[i] for i in range(1, len(list(buttons.keys())) + 1)] if buttons is not None else ["OK", "Cancel"]
        idt = dict(ids) if ids is not None else {}
        if self.dialog_handler:
            btn, res = self.dialog_handler(items, btns, idt)
        else:
            btn = btns[0]
            res = {it["name"]: it.get("value", it.get("text", "")) for it in items if it.get("name")}
        if btn is False or btn is None: return False, ls.lua.table()
        return btn, ls.lua.table_from(res)

    def load_script(self, path) -> LuaScript:
        s = LuaScript(self, path).load(); self.scripts.append(s); return s

    def load_dir(self, d):
        for p in sorted(Path(d).glob("*.lua")):
            self.load_script(p)

    def macros(self): return [m for s in self.scripts for m in s.macros]

    def run(self, macro: Macro, selected: list[int] | None = None, active: int | None = None):
        """Run a macro. Indices are 1-based (Aegisub-style). Returns (selection, active)."""
        self.cancelled = False
        ls = macro.script; L = ls.lua
        sel = L.table_from(selected if selected else [])
        act = active if active else (selected[0] if selected else 1)
        snapshot = [e for e in self.script.entries]
        import copy
        backup = copy.deepcopy(self.script.entries)
        try:
            if macro.kind == "filter":
                macro.fn(L.globals()["__subs"], None); return list(selected or []), act
            r = macro.fn(L.globals()["__subs"], sel, act)
        except Cancelled:
            self.script.entries[:] = backup; return list(selected or []), act
        except Exception:
            self.script.entries[:] = backup
            self.log("Lua error in %s: %s" % (macro.name, traceback.format_exc(limit=2)))
            raise
        out_sel, out_act = list(selected or []), act
        if r is not None:
            if isinstance(r, tuple):
                s, out_act = r[0], r[1] if len(r) > 1 else act
            else: s = r
            if hasattr(s, "values"): out_sel = [int(x) for x in s.values()]
            elif isinstance(s, (int, float)): out_sel = [int(s)]
        return out_sel, out_act
