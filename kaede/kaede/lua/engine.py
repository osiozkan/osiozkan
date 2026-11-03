"""Aegisub-compatible Lua automation engine on LuaJIT (via lupa).

Each script runs in its own Lua state, like Aegisub. Supports macros, export
filters, dialogs (through a Host), karaskel/util/utf8/re/unicode modules, and
selection/active-line semantics (indexes in Lua are over info+styles+events).
"""
from __future__ import annotations
import os
import re as _re
import traceback
import unicodedata
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

try:
    from lupa.luajit21 import LuaRuntime, LuaError
except Exception:  # pragma: no cover
    from lupa import LuaRuntime, LuaError  # type: ignore

from ..core.ass import Document, Event, Style, parse_color
from ..core.timeutil import FrameRate

LIBDIR = os.path.join(os.path.dirname(__file__), "lib")
CANCEL = "__AEGISUB_CANCEL__"


class AutomationError(Exception):
    pass


class Cancelled(Exception):
    pass


@dataclass
class Macro:
    name: str
    description: str
    fn: Any
    validate: Any
    is_active: Any
    script: "Script"
    kind: str = "macro"
    priority: int = 0
    dialog_fn: Any = None


@dataclass
class Script:
    path: str
    name: str = ""
    description: str = ""
    author: str = ""
    version: str = ""
    namespace: str = ""
    macros: list = field(default_factory=list)
    filters: list = field(default_factory=list)
    error: str = ""
    rt: Any = None
    runner: Any = None


class Host:
    """Callbacks from scripts into the application. Override in the GUI."""

    def __init__(self):
        self.fps = FrameRate(24000 / 1001)
        self.video_size: Optional[tuple[int, int]] = None
        self.keyframes: list[int] = []
        self.logs: list[str] = []
        self.clipboard = ""
        self.audio_selection: Optional[tuple[int, int]] = None
        self.script_path = ""
        self.video_path = ""
        self.audio_path = ""
        self.user_dir = os.path.expanduser("~/.kaede")
        self.canceled = False

    def log(self, level: int, msg: str): self.logs.append(msg)
    def progress(self, kind: str, value=None): return None
    def undo_point(self, desc): pass

    def text_extents(self, style: dict, text: str):
        size = float(style.get("fontsize", 48) or 48)
        sx = float(style.get("scale_x", 100)) / 100
        sy = float(style.get("scale_y", 100)) / 100
        sp = float(style.get("spacing", 0))
        n = len(text)
        w = sum(size * (0.62 if ch.isupper() or ch in "MWmw" else 0.28 if ch in " ilIj.,'" else 0.5)
                for ch in text) * sx + sp * n * sx
        return w, size * sy * 1.2, size * sy * 0.22, 0.0

    def dialog(self, dialog: list, buttons: list, ids: dict):
        """Default (headless): accept defaults, press first/OK button."""
        res = {}
        for c in dialog:
            name = c.get("name")
            if not name:
                continue
            cls = c.get("class")
            if cls in ("edit", "textbox", "color", "coloralpha", "alpha"):
                res[name] = c.get("text", c.get("value", ""))
            elif cls in ("intedit", "floatedit", "checkbox", "slider"):
                res[name] = c.get("value", False if cls == "checkbox" else 0)
            elif cls == "dropdown":
                res[name] = c.get("value", (c.get("items") or [""])[0] if c.get("items") else "")
        if not buttons:
            return True, res
        return buttons[0], res

    def file_dialog(self, mode, *args): return None


# ------------------------------------------------------------------- helpers
def _to_py(v):
    """Lua table -> list / dict (recursive)."""
    if v is None or isinstance(v, (bool, int, float, str, bytes)):
        return v
    try:
        keys = list(v.keys())
    except Exception:
        return v
    if keys and all(isinstance(k, (int, float)) for k in keys) and \
            sorted(keys) == list(range(1, len(keys) + 1)):
        return [_to_py(v[i]) for i in range(1, len(keys) + 1)]
    return {k: _to_py(v[k]) for k in keys}


def _color_str(c: str) -> str:
    return c if c.endswith("&") or not c.startswith("&H") else c


class _Items:
    """Flat Aegisub-style entry list: [info..., styles..., events...]."""

    def __init__(self, doc: Document):
        self.items: list[tuple[str, Any]] = []
        for k, v in doc.info:
            self.items.append(("info", [k, v]))
        for s in doc.styles:
            self.items.append(("style", s.copy()))
        for e in doc.events:
            self.items.append(("dialogue", e.copy()))
        self.sections = {k: list(v) for k, v in doc.sections.items()}
        self.garbage = list(doc.garbage)
        self.v4plus = doc.v4plus

    def to_doc(self) -> Document:
        d = Document()
        d.info, d.styles, d.events = [], [], []
        for kind, obj in self.items:
            if kind == "info":
                d.info.append((obj[0], obj[1]))
            elif kind == "style":
                d.styles.append(obj)
            else:
                d.events.append(obj)
        d.sections, d.garbage, d.v4plus = self.sections, self.garbage, self.v4plus
        return d

    def n_before_events(self) -> int:
        return sum(1 for k, _ in self.items if k != "dialogue")

    def event_to_flat(self, idx: int) -> int:
        """0-based event index -> 1-based flat index."""
        return self.n_before_events() + idx + 1

    def flat_to_event(self) -> dict[int, int]:
        m, n = {}, 0
        for i, (k, _) in enumerate(self.items):
            if k == "dialogue":
                m[i + 1] = n
                n += 1
        return m


class ScriptRunner:
    def __init__(self, script: Script, engine: "Engine"):
        self.script = script
        self.engine = engine
        self.host = engine.host
        self.items: Optional[_Items] = None
        self.rt = LuaRuntime(unpack_returned_tuples=True, encoding="utf-8")
        self._setup()

    # ---- Lua <-> Python conversion --------------------------------------
    def _entry_to_lua(self, kind, obj, flat_index=0):
        rt = self.rt
        if kind == "info":
            k, v = obj
            return rt.table_from({"class": "info", "section": "[Script Info]", "key": k, "value": v,
                                  "raw": f"{k}: {v}"})
        if kind == "style":
            s: Style = obj
            return rt.table_from({
                "class": "style", "section": "[V4+ Styles]", "raw": s.to_line(), "name": s.name,
                "fontname": s.fontname, "fontsize": s.fontsize, "color1": s.primary, "color2": s.secondary,
                "color3": s.outline, "color4": s.back, "bold": s.bold, "italic": s.italic,
                "underline": s.underline, "strikeout": s.strikeout, "scale_x": s.scale_x,
                "scale_y": s.scale_y, "spacing": s.spacing, "angle": s.angle,
                "borderstyle": s.border_style, "outline": s.outline_w, "shadow": s.shadow,
                "align": s.align, "margin_l": s.margin_l, "margin_r": s.margin_r,
                "margin_t": s.margin_t, "margin_b": s.margin_t, "encoding": s.encoding,
                "relative_to": 2})
        e: Event = obj
        t = rt.table_from({
            "class": "dialogue", "section": "[Events]", "raw": e.to_line(), "comment": e.comment,
            "layer": e.layer, "start_time": e.start, "end_time": e.end, "style": e.style,
            "actor": e.actor, "margin_l": e.margin_l, "margin_r": e.margin_r,
            "margin_t": e.margin_t, "margin_b": e.margin_t, "effect": e.effect, "text": e.text})
        t["extra"] = rt.table_from(dict(e.extra))
        return t

    def _lua_to_entry(self, t):
        cls = t["class"]
        g = lambda k, d=None: (t[k] if t[k] is not None else d)
        if cls == "info":
            return "info", [str(g("key", "")), str(g("value", ""))]
        if cls == "style":
            return "style", Style(
                name=str(g("name", "Default")), fontname=str(g("fontname", "Arial")),
                fontsize=float(g("fontsize", 48)), primary=str(g("color1", "&H00FFFFFF")),
                secondary=str(g("color2", "&H000000FF")), outline=str(g("color3", "&H00000000")),
                back=str(g("color4", "&H00000000")), bold=bool(g("bold", False)),
                italic=bool(g("italic", False)), underline=bool(g("underline", False)),
                strikeout=bool(g("strikeout", False)), scale_x=float(g("scale_x", 100)),
                scale_y=float(g("scale_y", 100)), spacing=float(g("spacing", 0)),
                angle=float(g("angle", 0)), border_style=int(g("borderstyle", 1)),
                outline_w=float(g("outline", 2)), shadow=float(g("shadow", 2)),
                align=int(g("align", 2)), margin_l=int(g("margin_l", 10)),
                margin_r=int(g("margin_r", 10)), margin_t=int(g("margin_t", g("margin_b", 10))),
                encoding=int(g("encoding", 1)))
        if cls == "dialogue":
            extra = {}
            ex = t["extra"]
            if ex is not None:
                try:
                    extra = {k: ex[k] for k in ex.keys()}
                except Exception:
                    extra = {}
            return "dialogue", Event(
                comment=bool(g("comment", False)), layer=int(g("layer", 0)),
                start=int(round(g("start_time", 0))), end=int(round(g("end_time", 0))),
                style=str(g("style", "Default")), actor=str(g("actor", "")),
                margin_l=int(g("margin_l", 0)), margin_r=int(g("margin_r", 0)),
                margin_t=int(g("margin_t", g("margin_b", 0))), effect=str(g("effect", "")),
                text=str(g("text", "")), extra=extra)
        raise AutomationError(f"unsupported line class: {cls!r}")

    # ---- host callbacks exposed as K ------------------------------------
    def _setup(self):
        rt, host = self.rt, self.host
        script = self.script
        py = rt.eval("function(t) return t end")  # identity (keeps lupa happy)

        def sub_count(): return len(self.items.items) if self.items else 0

        def sub_get(i):
            kind, obj = self.items.items[int(i) - 1]
            return self._entry_to_lua(kind, obj)

        def sub_set(i, line):
            self.items.items[int(i) - 1] = self._lua_to_entry(line)

        def sub_insert(i, lines):
            i = int(i) - 1
            for off, ln in enumerate(_to_lines(lines)):
                self.items.items.insert(i + off, self._lua_to_entry(ln))

        def _to_lines(lines):
            return [lines[k] for k in range(1, len(list(lines.keys())) + 1)]

        def sub_delete(idx):
            ids = sorted({int(idx[k]) for k in idx.keys()}, reverse=True)
            for i in ids:
                del self.items.items[i - 1]

        def log(level, msg): host.log(int(level), msg)

        def progress(kind, v=None):
            return host.progress(kind, v)

        def register_macro(name, desc, fn, validate, is_active):
            script.macros.append(Macro(str(name), str(desc or ""), fn, validate, is_active, script))

        def register_filter(name, desc, prio, fn, dlg):
            script.filters.append(Macro(str(name), str(desc or ""), fn, None, None, script,
                                        kind="filter", priority=int(prio or 0), dialog_fn=dlg))

        def text_extents(style, text):
            st = {k: style[k] for k in style.keys()}
            return host.text_extents(st, text)

        def dialog(dlg, buttons, ids):
            d = [_to_py(dlg[k]) for k in range(1, len(list(dlg.keys())) + 1)] if dlg else []
            b = _to_py(buttons) if buttons else []
            if isinstance(b, dict):
                b = list(b.values())
            ids_py = _to_py(ids) if ids else {}
            btn, res = host.dialog(d, b, ids_py or {})
            return btn, rt.table_from(res)

        def decode_path(p):
            p = str(p)
            sp = host.script_path
            m = {"?script": os.path.dirname(sp) if sp else os.getcwd(),
                 "?data": LIBDIR, "?user": host.user_dir, "?temp": "/tmp", "?local": host.user_dir,
                 "?video": os.path.dirname(host.video_path) if host.video_path else "",
                 "?audio": os.path.dirname(host.audio_path) if host.audio_path else ""}
            for k, v in m.items():
                if p.startswith(k):
                    return v + p[len(k):]
            return p

        def project_properties():
            return rt.table_from({"automation_scripts": "", "export_filters": "", "export_encoding": "UTF-8",
                                  "style_storage": "Default", "video_zoom": 1.0,
                                  "ar_mode": 0, "ar_value": 0.0, "video_position": 0,
                                  "audio_file": host.audio_path, "video_file": host.video_path,
                                  "timecodes_file": "", "keyframes_file": ""})

        def video_size():
            if not host.video_size:
                return None
            w, h = host.video_size
            return w, h, 0, 0.0

        K = rt.table_from({
            "sub_count": sub_count, "sub_get": sub_get, "sub_set": sub_set,
            "sub_insert": sub_insert, "sub_delete": sub_delete, "log": log, "progress": progress,
            "register_macro": register_macro, "register_filter": register_filter,
            "frame_from_ms": lambda ms: host.fps.frame_at(int(ms)),
            "ms_from_frame": lambda f: host.fps.time_at(int(f)),
            "video_size": video_size,
            "keyframes": lambda: rt.table_from(list(host.keyframes)),
            "project_properties": project_properties,
            "audio_selection": lambda: tuple(host.audio_selection) if host.audio_selection else None,
            "undo_point": lambda d=None: host.undo_point(d),
            "decode_path": decode_path,
            "file_name": lambda: os.path.basename(host.script_path) if host.script_path else None,
            "text_extents": text_extents,
            "clipboard_get": lambda: host.clipboard, "clipboard_set": lambda s: setattr(host, "clipboard", s) or True,
            "dialog": dialog, "file_dialog": lambda *a: host.file_dialog(*a),
            "script_dir": lambda: os.path.dirname(script.path),
            "user_include": lambda: os.path.join(host.user_dir, "include"),
        })
        K["lfs"] = _make_lfs(rt)
        K["include_dirs"] = lambda: ";".join(_include_dirs(host))
        K["re"] = _make_re(rt)
        K["unicode"] = _make_unicode(rt)
        setup = rt.execute(open(os.path.join(LIBDIR, "prelude.lua"), encoding="utf-8").read())
        self.aegisub = setup(K, LIBDIR)
        self.wrap_subs = self.aegisub["__wrap_subs"]

    # ---- running ---------------------------------------------------------
    def load(self):
        try:
            code = open(self.script.path, "rb").read().decode("utf-8-sig", errors="replace")
            chunk = self.rt.eval("function(src, name) return assert(loadstring(src, name)) end")(
                code, "@" + self.script.path)
            chunk()
        except LuaError as e:
            raise AutomationError(f"{self.script.path}: {e}") from None
        g = self.rt.globals()
        for attr, key in (("name", "script_name"), ("description", "script_description"),
                          ("author", "script_author"), ("version", "script_version"),
                          ("namespace", "script_namespace")):
            v = g[key]
            setattr(self.script, attr, str(v) if v is not None else "")

    def call(self, fn, doc: Document, selection: list[int], active: int, *extra):
        """Run fn(subs, selection, active) over a working copy of doc."""
        self.items = _Items(doc)
        subs = self.wrap_subs()
        base = self.items.n_before_events()
        sel_flat = [base + i + 1 for i in selection]
        act_flat = base + active + 1 if active is not None and active >= 0 else 0
        sel_t = self.rt.table_from(sel_flat)
        try:
            res = fn(subs, sel_t, act_flat, *extra) if not extra else fn(subs, *extra)
        except LuaError as e:
            if CANCEL in str(e):
                raise Cancelled() from None
            raise AutomationError(_clean(str(e))) from None
        return res, self.items

    def finish(self, res, items: _Items):
        new_doc = items.to_doc()
        mapping = items.flat_to_event()
        new_sel, new_active = None, None
        vals = res if isinstance(res, tuple) else (res,)
        if vals and vals[0] is not None and not isinstance(vals[0], (bool, int, float, str)):
            try:
                ks = _to_py(vals[0])
                if isinstance(ks, dict):
                    ks = list(ks.values())
                new_sel = [mapping[int(k)] for k in ks if int(k) in mapping]
            except Exception:
                new_sel = None
        elif isinstance(vals[0] if vals else None, int) and not isinstance(vals[0], bool):
            new_sel = [mapping[vals[0]]] if vals[0] in mapping else None
        if len(vals) > 1 and isinstance(vals[1], (int, float)):
            new_active = mapping.get(int(vals[1]))
        return new_doc, new_sel, new_active


def _clean(msg: str) -> str:
    return msg.replace("\r", "")


@dataclass
class RunResult:
    doc: Document
    selection: Optional[list[int]]
    active: Optional[int]
    logs: list[str]


class Engine:
    def __init__(self, host: Optional[Host] = None):
        self.host = host or Host()
        self.scripts: list[Script] = []

    def load_script(self, path: str) -> Script:
        path = os.path.abspath(path)
        script = Script(path=path, name=os.path.basename(path))
        try:
            runner = ScriptRunner(script, self)
            script.runner = runner
            runner.load()
            if not script.name:
                script.name = os.path.basename(path)
        except AutomationError as e:
            script.error = str(e)
        except Exception as e:  # pragma: no cover
            script.error = f"{type(e).__name__}: {e}\n{traceback.format_exc()}"
        self.scripts.append(script)
        return script

    def load_dir(self, d: str) -> list[Script]:
        out = []
        if os.path.isdir(d):
            for fn in sorted(os.listdir(d)):
                p = os.path.join(d, fn)
                if fn.lower().endswith((".lua", ".moon")) and os.path.isfile(p):
                    out.append(self.load_script(p))
        return out

    def all_macros(self) -> list[Macro]:
        return [m for s in self.scripts for m in s.macros]

    def all_filters(self) -> list[Macro]:
        return sorted((f for s in self.scripts for f in s.filters), key=lambda f: -f.priority)

    def validate(self, macro: Macro, doc: Document, selection: list[int], active: int) -> bool:
        if macro.validate is None:
            return True
        r = macro.script.runner
        try:
            res, _ = r.call(macro.validate, doc, selection, active)
        except (AutomationError, Cancelled):
            return False
        v = res[0] if isinstance(res, tuple) else res
        return bool(v) if v is not None else False

    def run_macro(self, macro: Macro, doc: Document, selection: list[int], active: int = 0) -> RunResult:
        r = macro.script.runner
        self.host.script_path = macro.script.path
        self.host.logs.clear()
        res, items = r.call(macro.fn, doc, selection, active)
        new_doc, sel, act = r.finish(res, items)
        return RunResult(new_doc, sel, act, list(self.host.logs))

    def run_filter(self, f: Macro, doc: Document, config: Optional[dict] = None) -> RunResult:
        r = f.script.runner
        self.host.script_path = f.script.path
        self.host.logs.clear()
        items_doc = doc
        r.items = _Items(items_doc)
        subs = r.wrap_subs()
        cfg = r.rt.table_from(config or {})
        try:
            f.fn(subs, cfg)
        except LuaError as e:
            if CANCEL in str(e):
                raise Cancelled() from None
            raise AutomationError(_clean(str(e))) from None
        return RunResult(r.items.to_doc(), None, None, list(self.host.logs))


# ------------------------------------------------------------------ re module
_RE_FLAGS = {"ICASE": 1, "NOSUB": 2, "COLLATE": 4, "NO_MOD_M": 8, "MOD_S": 16, "MOD_X": 32,
             "NO_MOD_S": 64, "PERL": 0, "EXTENDED": 0, "BASIC": 0}


def _pyflags(flags: int) -> int:
    f = _re.MULTILINE if not (flags & 8) else 0
    if flags & 1: f |= _re.IGNORECASE
    if flags & 16: f |= _re.DOTALL
    if flags & 32: f |= _re.VERBOSE
    return f


def _make_re(rt):
    cache: dict = {}

    def comp(pat, flags=0):
        key = (pat, int(flags or 0))
        if key not in cache:
            try:
                cache[key] = _re.compile(pat, _pyflags(int(flags or 0)))
            except _re.error as e:
                raise AutomationError(f"regex error: {e}")
        return cache[key]

    def bpos(s: str, i: int) -> int:  # char index -> 1-based byte offset
        return len(s[:i].encode("utf-8")) + 1

    def mt(s, m, g):
        a, b = m.span(g)
        if a < 0:
            return rt.table_from({"str": "", "first": 0, "last": -1})
        return rt.table_from({"str": m.group(g), "first": bpos(s, a), "last": bpos(s, b) - 1})

    def find(s, pat, flags=0, init=None):
        p = comp(pat, flags)
        ms = [mt(s, m, 0) for m in p.finditer(s)]
        return rt.table_from(ms) if ms else None

    def match(s, pat, flags=0, init=None):
        p = comp(pat, flags)
        m = p.search(s)
        if not m:
            return None
        return rt.table_from([mt(s, m, g) for g in range(0, (p.groups or 0) + 1)])

    def _repl(rep: str):
        def f(m):
            def g(mm):
                tok = mm.group(0)
                if tok in ("$$", "\\$"): return "$"
                if tok == "$&": return m.group(0)
                n = tok.lstrip("$\\{").rstrip("}")
                try: return m.group(int(n)) or ""
                except (IndexError, ValueError): return ""
            return _re.sub(r"\$\$|\\\$|\$&|\$\{\d+\}|\$\d+|\\\d", g, rep)
        return f

    def sub(s, pat, rep, count=0, flags=0):
        # Aegisub: re.sub(str, pattern, replacement, [count], [flags])
        if isinstance(count, str):
            count = 0
        return comp(pat, flags).sub(_repl(rep), s, count=int(count or 0))

    def split(s, pat, flags=0):
        return rt.table_from(comp(pat, flags).split(s))

    mod = rt.table_from({**{k: v for k, v in _RE_FLAGS.items()},
                         "find": find, "match": match, "sub": sub, "gsub": sub, "split": split})
    mk = rt.eval("""function(re)
      function re.compile(pat, flags)
        local o = {}
        function o:find(s, f) return re.find(s, pat, flags or f) end
        function o:match(s) return re.match(s, pat, flags) end
        function o:sub(s, rep, count) return re.sub(s, pat, rep, count, flags) end
        o.gsub = o.sub
        function o:split(s) return re.split(s, pat, flags) end
        return o
      end
      return re end""")
    return mk(mod)


def _include_dirs(host) -> list[str]:
    """Search paths for include()/require: Kaede user dir, KAEDE_INCLUDE env, and existing Aegisub dirs."""
    home = os.path.expanduser("~")
    cands = [os.path.join(host.user_dir, "automation", "include"), os.path.join(host.user_dir, "include")]
    cands += [p for p in os.environ.get("KAEDE_INCLUDE", "").split(os.pathsep) if p]
    cands += [os.path.join(home, ".aegisub", "automation", "include"),
              os.path.join(home, "Library", "Application Support", "Aegisub", "automation", "include")]
    appdata = os.environ.get("APPDATA")
    if appdata:
        cands.append(os.path.join(appdata, "Aegisub", "automation", "include"))
    cands += getattr(host, "extra_include_dirs", [])
    return [c for c in cands if os.path.isdir(c)]


def _make_lfs(rt):
    import time as _t
    def attributes(path, attr=None):
        try:
            st = os.stat(path)
        except OSError as e:
            return None, str(e), e.errno
        d = {"mode": "directory" if os.path.isdir(path) else "file" if os.path.isfile(path) else "other",
             "size": st.st_size, "modification": int(st.st_mtime), "access": int(st.st_atime),
             "change": int(st.st_ctime), "nlink": st.st_nlink, "dev": st.st_dev, "ino": st.st_ino,
             "uid": st.st_uid, "gid": st.st_gid, "permissions": "rwxrwxrwx"}
        return d[attr] if attr else rt.table_from(d)
    def mkdir(p):
        try: os.mkdir(p); return True
        except OSError as e: return None, str(e), e.errno
    def rmdir(p):
        try: os.rmdir(p); return True
        except OSError as e: return None, str(e), e.errno
    def chdir(p):
        try: os.chdir(p); return True
        except OSError as e: return None, str(e), e.errno
    def dir_(p):
        names = [".", ".."] + sorted(os.listdir(p))
        it = iter(names)
        return lambda *a: next(it, None)
    def touch(p, atime=None, mtime=None):
        try:
            os.utime(p, None if atime is None else (atime, mtime or atime)); return True
        except OSError as e: return None, str(e), e.errno
    return rt.table_from({"attributes": attributes, "mkdir": mkdir, "rmdir": rmdir, "chdir": chdir,
                          "currentdir": lambda: os.getcwd(), "dir": dir_, "touch": touch,
                          "_VERSION": "LuaFileSystem 1.8.0 (kaede)"})


def _make_unicode(rt):
    def codepoint(c): return ord(c[0]) if c else None
    def chars(s): return rt.table_from(list(s))
    def to_upper(s): return s.upper()
    def to_lower(s): return s.lower()
    def to_fold_case(s): return s.casefold()
    def length(s): return len(s)
    def charwidth(c): return 2 if unicodedata.east_asian_width(c[0]) in "WF" else 1
    mod = rt.table_from({"codepoint": codepoint, "to_upper": to_upper, "to_lower": to_lower,
                         "to_fold_case": to_fold_case, "len": length, "charwidth": charwidth})
    it = rt.eval("""function(u, chars)
      function u.chars(s) local t = chars(s); local i = 0
        return function() i = i + 1; return t[i] end end
      return u end""")
    return it(mod, chars)
