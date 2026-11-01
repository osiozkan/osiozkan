"""Aegisub Auto4-compatible Lua automation host (LuaJIT via lupa)."""
from __future__ import annotations

import os
import re as _re
import sys
import tempfile
import unicodedata
from dataclasses import dataclass, field
from typing import Callable, Optional

from lupa.luajit21 import LuaRuntime, LuaError

from ..core.ass import ASSFile, Event, Info, Other, Style, norm_color, S_INFO, S_STYLES, S_EVENTS, S_FONTS, S_GRAPHICS
from . import metrics

DEFAULT_INCLUDE_DIR = os.path.join(os.path.dirname(__file__), "include")
CANCEL = "__aegisub_cancel__"

SECTION_BRACKET = {S_INFO: "[Script Info]", S_STYLES: "[V4+ Styles]", S_EVENTS: "[Events]",
                   S_FONTS: "[Fonts]", S_GRAPHICS: "[Graphics]"}


def enc(s):
    """Python text -> byte-transparent Lua string (UTF-8 bytes carried as latin-1 chars)."""
    return s.encode("utf-8", "replace").decode("latin-1") if isinstance(s, str) else s


def dec(s):
    """Lua byte-string (latin-1 view) -> real text; invalid UTF-8 becomes U+FFFD."""
    return s.encode("latin-1", "replace").decode("utf-8", "replace") if isinstance(s, str) else s


def _enc_deep(o):
    if isinstance(o, str):
        return enc(o)
    if isinstance(o, dict):
        return {_enc_deep(k): _enc_deep(v) for k, v in o.items()}
    if isinstance(o, (list, tuple)):
        return [_enc_deep(v) for v in o]
    return o


class ScriptError(Exception):
    pass


class CancelledError(Exception):
    pass


# ----------------------------------------------------------------------------- UI

class HeadlessUI:
    """Non-interactive UI: dialogs auto-accept using control defaults (or `answers`)."""

    def __init__(self, answers: dict | None = None, button: str | None = None):
        self.answers = answers or {}
        self.button = button
        self.logs: list[tuple[int, str]] = []
        self.progress_state = {"value": 0.0, "task": "", "title": ""}

    def dialog(self, controls: list[dict], buttons: list[str] | None, ids: dict | None):
        results = {}
        for c in controls:
            name = c.get("name")
            if not name:
                continue
            cls = c.get("class")
            if cls in ("label", "image"):
                continue
            if name in self.answers:
                results[name] = self.answers[name]
                continue
            if cls in ("edit", "textbox"):
                results[name] = c.get("text", "")
            elif cls == "dropdown":
                results[name] = c.get("value", (c.get("items") or [""])[0] if c.get("items") else "")
            elif cls == "checkbox":
                results[name] = bool(c.get("value", False))
            elif cls in ("intedit", "floatedit", "slider"):
                results[name] = c.get("value", c.get("min", 0) or 0)
            elif cls in ("color", "coloralpha", "alpha"):
                results[name] = c.get("value", "&H00FFFFFF&")
            else:
                results[name] = c.get("value", c.get("text", ""))
        if buttons:
            btn = self.button if self.button in buttons else buttons[0]
            if ids and ids.get("cancel") == btn:
                return None, results
            return btn, results
        return True, results

    def file_dialog(self, kind, *args):
        return None

    def clipboard_get(self):
        return ""

    def clipboard_set(self, s):
        return True

    def log(self, level: int, msg: str):
        self.logs.append((level, msg))

    def progress(self, kind: str, value):
        self.progress_state[kind] = value

    def cancelled(self) -> bool:
        return False


@dataclass
class ProjectInfo:
    fps: float | None = None
    keyframes: list[int] = field(default_factory=list)  # frame numbers
    video_size: tuple[int, int] | None = None
    video_file: str = ""
    audio_file: str = ""
    keyframes_file: str = ""
    timecodes_file: str = ""
    script_path: str = ""  # path of the .ass being edited
    video_position: int = 0
    video_zoom: float = 1.0


class Host:
    """Everything a script can see: document, selection, project, UI."""

    def __init__(self, doc: ASSFile, selection: list[int] | None = None, active: int | None = None,
                 ui: HeadlessUI | None = None, project: ProjectInfo | None = None):
        self.doc = doc
        self.selection = list(selection or [])  # 1-based indices into doc.lines
        self.active = active or (self.selection[0] if self.selection else 0)
        self.ui = ui or HeadlessUI()
        self.project = project or ProjectInfo()
        self.modified = False

    def event_indices(self) -> list[int]:
        """1-based indices of all dialogue lines (handy default selection)."""
        return [i + 1 for i, l in enumerate(self.doc.lines) if isinstance(l, Event)]


@dataclass
class Macro:
    script: "AutomationScript"
    name: str
    description: str
    index: int  # 1-based position in aegisub.__macros

    def validate(self, host: Host) -> tuple[bool, str]:
        return self.script._validate(self, host)

    def run(self, host: Host):
        return self.script._run_macro(self, host)


@dataclass
class Filter:
    script: "AutomationScript"
    name: str
    description: str
    priority: int
    index: int

    def run(self, host: Host, config: dict | None = None):
        return self.script._run_filter(self, host, config or {})


# ----------------------------------------------------------------------------- conversions

def _int(v, default=0) -> int:
    try:
        return int(round(float(v)))
    except (TypeError, ValueError):
        return default


def _str(v, default="") -> str:
    if v is None:
        return default
    return dec(v) if isinstance(v, str) else str(v)


def _is_lua_table(o) -> bool:
    return hasattr(o, "items") and hasattr(o, "keys") and not isinstance(o, dict)


def lua_to_py(o):
    """Recursively convert Lua tables (array-like -> list, else dict); strings are decoded."""
    if isinstance(o, str):
        return dec(o)
    if _is_lua_table(o):
        keys = list(o.keys())
        if keys and all(isinstance(k, (int, float)) for k in keys):
            ks = sorted(keys)
            if ks == list(range(1, len(ks) + 1)):
                return [lua_to_py(o[k]) for k in ks]
        return {lua_to_py(k): lua_to_py(o[k]) for k in keys}
    return o


class AutomationScript:
    def __init__(self, path: str, extra_include_dirs: list[str] | None = None):
        self.path = os.path.abspath(path)
        self.filename = os.path.basename(path)
        self.include_dirs = [os.path.dirname(self.path), DEFAULT_INCLUDE_DIR] + list(extra_include_dirs or [])
        self.name = os.path.splitext(self.filename)[0]
        self.description = ""
        self.author = ""
        self.version = ""
        self.macros: list[Macro] = []
        self.filters: list[Filter] = []
        self.error: str | None = None
        self.host: Host | None = None
        self._cancel_flag = False
        self.lua = LuaRuntime(unpack_returned_tuples=True, register_eval=False, register_builtins=False,
                               encoding="latin-1", source_encoding="latin-1")
        self._load()

    # ---------------------------------------------------------------- bridge
    def _bridge(self):
        L = self.lua
        tf = lambda o: L.table_from(_enc_deep(o))

        def h() -> Host:
            if self.host is None:
                raise RuntimeError("script API used outside of a macro run")
            return self.host

        def log(level, msg):
            h().ui.log(int(level), dec(msg) if isinstance(msg, str) else str(msg))

        def progress(kind, value):
            h().ui.progress(kind, dec(value))

        def progress_cancelled():
            return bool(h().ui.cancelled())

        def text_extents(style, text):
            st = lua_to_py(style) if style is not None else {}
            if not isinstance(st, dict):
                raise RuntimeError("text_extents: style must be a table")
            return metrics.text_extents(st, dec(text) or "")

        def frame_from_ms(ms):
            fps = h().project.fps
            if not fps:
                return None
            return int(float(ms) * fps / 1000.0 + 1e-9)

        def ms_from_frame(fr):
            fps = h().project.fps
            if not fps:
                return None
            fr = int(fr)
            # Aegisub semantic: a time in the middle of the frame's display window
            return int(round(fr * 1000.0 / fps))

        def video_size():
            vs = h().project.video_size
            if not vs:
                return None
            return vs[0], vs[1], 0, 0

        def keyframes():
            return tf(list(h().project.keyframes))

        def decode_path(p):
            p = dec(str(p))
            pr = h().project
            m = _re.match(r"^\?(script|data|user|temp|video|audio|local|dialog)(.*)$", p)
            if not m:
                return enc(p)
            kind, rest = m.groups()
            base = {
                "script": os.path.dirname(pr.script_path) if pr.script_path else "",
                "data": os.path.dirname(DEFAULT_INCLUDE_DIR),
                "user": os.path.expanduser("~/.subforge"),
                "temp": tempfile.gettempdir(),
                "video": os.path.dirname(pr.video_file) if pr.video_file else "",
                "audio": os.path.dirname(pr.audio_file) if pr.audio_file else "",
                "local": os.path.expanduser("~/.subforge"),
                "dialog": os.getcwd(),
            }[kind]
            return enc(base + rest)

        def project_properties():
            pr = h().project
            d = {"audio_file": pr.audio_file, "video_file": pr.video_file, "timecodes_file": pr.timecodes_file,
                 "keyframes_file": pr.keyframes_file, "video_position": pr.video_position,
                 "video_zoom": pr.video_zoom, "ar_mode": 0, "ar_value": 0.0, "script_prefix": "",
                 "automation_scripts": "", "export_filters": "", "export_encoding": "",
                 "style_storage": "Default", "active_row": h().active, "subtitles_file": pr.script_path}
            return tf(d)

        def dialog(dlg, buttons, ids):
            controls = lua_to_py(dlg)
            if isinstance(controls, dict):
                controls = list(controls.values())
            btns = lua_to_py(buttons) if buttons is not None else None
            idd = lua_to_py(ids) if ids is not None else None
            btn, results = h().ui.dialog(controls or [], btns, idd)
            return (False if btn is None else enc(btn)), tf(results or {})

        def file_dialog(kind, *args):
            a = [lua_to_py(x) for x in args]
            r = h().ui.file_dialog(kind, *a)
            return tf(r) if isinstance(r, list) else enc(r)

        def clipboard_get():
            return enc(h().ui.clipboard_get())

        def clipboard_set(s):
            return h().ui.clipboard_set(dec(str(s)))

        def include_dirs():
            return tf(self.include_dirs)

        _cache: dict = {}

        def compile_re(pat, flags):
            pat = dec(pat)
            key = (pat, int(flags or 0))
            r = _cache.get(key)
            if r is None:
                fl = 0
                if key[1] & 1:
                    fl |= _re.I
                if not key[1] & 4:
                    fl |= 0  # NO_MOD_M: ^$ match only at string ends; python default
                else:
                    fl |= 0
                if key[1] & 16:
                    fl |= _re.M
                if key[1] & 8:
                    fl |= _re.S
                if key[1] & 32:
                    fl |= _re.X
                r = _re.compile(_perl_to_py(pat), fl)
                _cache[key] = r
            return r

        def b_of(s, i):  # char index -> 1-based utf8 byte offset
            return len(s[:i].encode("utf-8")) + 1

        def match_tables(s, m, nosub=False):
            res = [tf({"str": m.group(0), "first": b_of(s, m.start()), "last": b_of(s, m.end()) - 1})]
            if not nosub:
                for g in range(1, (m.re.groups or 0) + 1):
                    if m.group(g) is None:
                        res.append(tf({"str": "", "first": 0, "last": -1}))
                    else:
                        res.append(tf({"str": m.group(g), "first": b_of(s, m.start(g)), "last": b_of(s, m.end(g)) - 1}))
            return tf(res)

        def re_find(s, pat, flags=0, init=0):
            s = dec(s)
            r = compile_re(pat, flags)
            m = r.search(s)
            return match_tables(s, m, bool(int(flags or 0) & 2)) if m else None

        def re_match(s, pat, flags=0):
            s = dec(s)
            r = compile_re(pat, flags)
            m = r.fullmatch(s)
            return match_tables(s, m, bool(int(flags or 0) & 2)) if m else None

        def re_all(s, pat, flags=0):
            s = dec(s)
            r = compile_re(pat, flags)
            return tf([match_tables(s, m, bool(int(flags or 0) & 2)) for m in r.finditer(s)])

        def re_sub(s, pat, repl, flags=0, count=0):
            s, repl = dec(s), dec(repl)
            r = compile_re(pat, flags)

            def rep(m):
                out = []
                i = 0
                rs = str(repl)
                while i < len(rs):
                    c = rs[i]
                    if c in "$\\" and i + 1 < len(rs):
                        n = rs[i + 1]
                        if n.isdigit():
                            out.append(m.group(int(n)) or "" if int(n) <= (m.re.groups or 0) else "")
                            i += 2
                            continue
                        if c == "$" and n == "&":
                            out.append(m.group(0)); i += 2; continue
                        if c == "\\":
                            out.append({"n": "\n", "t": "\t"}.get(n, n)); i += 2; continue
                    out.append(c)
                    i += 1
                return "".join(out)
            res, n = r.subn(rep, s, count=int(count or 0))
            return enc(res), n

        def re_split(s, pat, flags=0):
            s = dec(s)
            r = compile_re(pat, flags)
            return tf(r.split(s))

        re_mod = tf({"find": re_find, "match": re_match, "all": re_all, "sub": re_sub, "split": re_split})

        def make_unicode():
            def chars(s):
                return tf(list(dec(s)))

            def width(c):
                c = dec(c)
                if not c:
                    return 0
                return 2 if unicodedata.east_asian_width(c[0]) in ("W", "F") else 1
            return tf({"codepoint": lambda c: ord(dec(c)[0]) if c else 0, "chars_list": chars,
                       "len": lambda s: len(dec(s)), "charwidth": width,
                       "to_upper": lambda s: enc(dec(s).upper()), "to_lower": lambda s: enc(dec(s).lower()),
                       "to_fold": lambda s: enc(dec(s).casefold()), "to_title": lambda s: enc(dec(s).title()),
                       "normalize": lambda s, f="NFC": enc(unicodedata.normalize(f, dec(s)))})

        def lfs_module():
            def attributes(p, a=None):
                p = dec(p)
                try:
                    st = os.stat(p)
                except OSError:
                    return None
                import stat as S
                d = {"mode": "directory" if S.S_ISDIR(st.st_mode) else "file", "size": st.st_size,
                     "modification": int(st.st_mtime), "access": int(st.st_atime), "change": int(st.st_ctime)}
                return d[a] if a else tf(d)
            return tf({"attributes": attributes, "currentdir": lambda: enc(os.getcwd()),
                       "mkdir": lambda p: (os.makedirs(dec(p), exist_ok=True) or True),
                       "dir_list": lambda p: tf(sorted(os.listdir(dec(p))))})

        return tf({
            "log": log, "progress": progress, "progress_cancelled": progress_cancelled,
            "text_extents": text_extents, "frame_from_ms": frame_from_ms, "ms_from_frame": ms_from_frame,
            "video_size": video_size, "keyframes": keyframes, "decode_path": decode_path,
            "project_properties": project_properties, "dialog": dialog, "file_dialog": file_dialog,
            "clipboard_get": clipboard_get, "clipboard_set": clipboard_set, "include_dirs": include_dirs,
            "re_raw": re_mod, "unicode_raw": make_unicode(), "lfs_module": lfs_module,
            "re_module": lambda: None, "unicode_module": lambda: None,
        })

    # ---------------------------------------------------------------- load
    def _load(self):
        L = self.lua
        bridge = self._bridge()
        # Wire Lua-side module constructors for re/unicode on top of raw callbacks
        L.execute(_RE_UNICODE_PRELUDE)
        L.eval("function(B) __install_modules(B) end")(bridge)
        boot = os.path.join(DEFAULT_INCLUDE_DIR, "bootstrap.lua")
        with open(boot, "r", encoding="utf-8") as fh:
            chunk = L.eval("function(src, B) return loadstring(src, '=bootstrap')(B) end")
            chunk(fh.read(), bridge)
        g = L.globals()
        g["script_dir"] = enc(os.path.dirname(self.path))
        loader = L.eval(
            "function(path) local f, err = loadfile(path) if not f then error(err, 0) end return f() end")
        # scripts may call aegisub.* at load time (e.g. text_extents) -> provide a neutral host
        self.host = Host(ASSFile.new())
        try:
            loader(enc(self.path))
        except LuaError as e:
            self.error = _clean_err(e)
            self.host = None
            return
        finally:
            pass
        self.host = None
        self.name = _str(g["script_name"], self.name)
        self.description = _str(g["script_description"])
        self.author = _str(g["script_author"])
        self.version = _str(g["script_version"])
        for i, m in enumerate(g["aegisub"]["__macros"].values(), 1):
            self.macros.append(Macro(self, _str(m["name"]), _str(m["description"]), i))
        for i, f in enumerate(g["aegisub"]["__filters"].values(), 1):
            self.filters.append(Filter(self, _str(f["name"]), _str(f["description"]), _int(f["priority"]), i))

    # ---------------------------------------------------------------- subtitles proxy
    def _make_proxy(self, host: Host):
        L = self.lua
        tf = lambda o: L.table_from(_enc_deep(o))
        doc = host.doc

        def to_lua(obj):
            sec = SECTION_BRACKET.get(obj.section, f"[{obj.section}]")
            if isinstance(obj, Event):
                d = {"class": "dialogue", "section": sec, "raw": obj.to_ass(), "comment": obj.comment,
                     "layer": obj.layer, "start_time": obj.start_time, "end_time": obj.end_time,
                     "style": obj.style, "actor": obj.actor, "margin_l": obj.margin_l, "margin_r": obj.margin_r,
                     "margin_t": obj.margin_t, "margin_b": obj.margin_b, "margin_v": obj.margin_t,
                     "effect": obj.effect, "text": obj.text, "extra": tf({})}
            elif isinstance(obj, Style):
                d = {"class": "style", "section": sec, "raw": obj.to_ass(), "name": obj.name,
                     "fontname": obj.fontname, "fontsize": obj.fontsize, "color1": obj.color1,
                     "color2": obj.color2, "color3": obj.color3, "color4": obj.color4, "bold": obj.bold,
                     "italic": obj.italic, "underline": obj.underline, "strikeout": obj.strikeout,
                     "scale_x": obj.scale_x, "scale_y": obj.scale_y, "spacing": obj.spacing,
                     "angle": obj.angle, "borderstyle": obj.borderstyle, "outline": obj.outline,
                     "shadow": obj.shadow, "align": obj.align, "margin_l": obj.margin_l,
                     "margin_r": obj.margin_r, "margin_t": obj.margin_t, "margin_b": obj.margin_b,
                     "margin_v": obj.margin_t, "encoding": obj.encoding, "relative_to": obj.relative_to,
                     "extra": tf({})}
            elif isinstance(obj, Info):
                d = {"class": "info", "section": sec, "raw": obj.to_ass(), "key": obj.key, "value": obj.value}
            else:
                d = {"class": "unknown", "section": sec, "raw": obj.raw}
            return tf(d)

        def num(t, k, default=0.0):
            v = t[k]
            return float(v) if v is not None else default

        def from_lua(t, default_section=None):
            if not _is_lua_table(t):
                raise RuntimeError("subtitle line must be a table")
            cls = t["class"]
            if cls == "dialogue":
                e = Event()
                e.comment = bool(t["comment"])
                e.layer = _int(t["layer"])
                e.start_time = _int(t["start_time"])
                e.end_time = _int(t["end_time"])
                e.style = _str(t["style"], "Default")
                e.actor = _str(t["actor"])
                e.margin_l, e.margin_r = _int(t["margin_l"]), _int(t["margin_r"])
                mt = t["margin_t"] if t["margin_t"] is not None else t["margin_v"]
                e.margin_t = _int(mt)
                e.margin_b = _int(t["margin_b"] if t["margin_b"] is not None else mt)
                e.effect = _str(t["effect"])
                e.text = _str(t["text"])
                return e
            if cls == "style":
                s = Style()
                s.name = _str(t["name"], "Default")
                s.fontname = _str(t["fontname"], "Arial")
                s.fontsize = num(t, "fontsize", 48)
                s.color1, s.color2 = norm_color(t["color1"] or "&H00FFFFFF"), norm_color(t["color2"] or "&H000000FF")
                s.color3, s.color4 = norm_color(t["color3"] or "&H00000000"), norm_color(t["color4"] or "&H00000000")
                s.bold, s.italic = bool(t["bold"]), bool(t["italic"])
                s.underline, s.strikeout = bool(t["underline"]), bool(t["strikeout"])
                s.scale_x, s.scale_y = num(t, "scale_x", 100), num(t, "scale_y", 100)
                s.spacing, s.angle = num(t, "spacing"), num(t, "angle")
                s.borderstyle = _int(t["borderstyle"], 1)
                s.outline, s.shadow = num(t, "outline", 2), num(t, "shadow", 2)
                s.align = _int(t["align"], 2)
                s.margin_l, s.margin_r = _int(t["margin_l"]), _int(t["margin_r"])
                mt = t["margin_t"] if t["margin_t"] is not None else t["margin_v"]
                s.margin_t = _int(mt)
                s.margin_b = _int(t["margin_b"] if t["margin_b"] is not None else mt)
                s.encoding = _int(t["encoding"], 1)
                s.relative_to = _int(t["relative_to"], 2)
                return s
            if cls == "info":
                return Info(_str(t["key"]), _str(t["value"]), S_INFO)
            if cls == "unknown":
                return Other(_str(t["raw"]), default_section or S_INFO)
            raise RuntimeError(f"unsupported line class: {cls}")

        def fix_section(obj, near):
            # Keep the section consistent with line class
            if isinstance(obj, Event):
                obj.section = S_EVENTS
            elif isinstance(obj, Style):
                obj.section = S_STYLES
            elif isinstance(obj, Info) and obj.section == S_INFO:
                if near is not None and getattr(near, "section", None):
                    obj.section = near.section
            return obj

        def idx(i, n_extra=0):
            i = _int(i)
            if i < 1 or i > len(doc.lines) + n_extra:
                raise RuntimeError(f"Requested out-of-range line from subtitle file: {i}")
            return i - 1

        def remap_insert(pos, n):
            host.selection = [s + n if s >= pos else s for s in host.selection]
            if host.active >= pos:
                host.active += n

        def remap_delete(a, b):
            n = b - a + 1
            sel = []
            for s in host.selection:
                if s < a:
                    sel.append(s)
                elif s > b:
                    sel.append(s - n)
            host.selection = sel
            if host.active > b:
                host.active -= n
            elif host.active >= a:
                host.active = sel[0] if sel else 0

        def get(i):
            i = _int(i)
            if i < 1 or i > len(doc.lines):
                return None
            return to_lua(doc.lines[i - 1])

        def set_(i, t):
            k = idx(i)
            doc.lines[k] = fix_section(from_lua(t, doc.lines[k].section), doc.lines[k])
            host.modified = True

        def count():
            return len(doc.lines)

        def insert(i, *lines):
            pos = _int(i)
            if pos < 1 or pos > len(doc.lines) + 1:
                raise RuntimeError(f"insert: out-of-range position {pos}")
            near = doc.lines[pos - 2] if pos >= 2 else (doc.lines[0] if doc.lines else None)
            objs = [fix_section(from_lua(l, near.section if near else None), near) for l in lines]
            doc.lines[pos - 1:pos - 1] = objs
            remap_insert(pos, len(objs))
            host.modified = True

        def append(*lines):
            near = doc.lines[-1] if doc.lines else None
            for l in lines:
                doc.lines.append(fix_section(from_lua(l, near.section if near else None), near))
            host.modified = True

        def delete(*indices):
            ids = sorted({_int(i) for i in indices}, reverse=True)
            for i in ids:
                idx(i)
                del doc.lines[i - 1]
                remap_delete(i, i)
            host.modified = True

        def deleterange(a, b):
            a, b = _int(a), _int(b)
            idx(a), idx(b)
            if a > b:
                a, b = b, a
            del doc.lines[a - 1:b]
            remap_delete(a, b)
            host.modified = True

        return tf({"get": get, "set": set_, "count": count, "insert": insert, "append": append,
                   "delete": delete, "deleterange": deleterange})

    # ---------------------------------------------------------------- running
    def _fail(self, e: Exception):
        msg = _clean_err(e)
        if CANCEL in msg:
            raise CancelledError()
        raise ScriptError(msg)

    def _validate(self, macro: Macro, host: Host):
        self.host = host
        try:
            m = self.lua.globals()["aegisub"]["__macros"][macro.index]
            if m["validate"] is None:
                return True, ""
            proxy = self._make_proxy(host)
            sel = self.lua.table_from(host.selection)
            r = self.lua.eval("__run_macro")(m, proxy, sel, host.active, True)
            ok, msg = (r if isinstance(r, tuple) else (r, None))
            return bool(ok), _str(msg)
        except LuaError as e:
            self._fail(e)
        finally:
            self.host = None

    def _run_macro(self, macro: Macro, host: Host):
        self.host = host
        try:
            m = self.lua.globals()["aegisub"]["__macros"][macro.index]
            proxy = self._make_proxy(host)
            sel = self.lua.table_from(host.selection)
            r = self.lua.eval("__run_macro")(m, proxy, sel, host.active, False)
            r1, r2 = (r if isinstance(r, tuple) else (r, None))
            if _is_lua_table(r1):
                host.selection = sorted(_int(v) for v in r1.values())
            if r2 is not None and not isinstance(r2, bool):
                host.active = _int(r2)
            elif host.selection and (host.active < 1 or host.active > len(host.doc.lines)):
                host.active = host.selection[0]
            return host
        except LuaError as e:
            self._fail(e)
        finally:
            self.host = None

    def _run_filter(self, flt: Filter, host: Host, config: dict):
        self.host = host
        try:
            f = self.lua.globals()["aegisub"]["__filters"][flt.index]
            proxy = self._make_proxy(host)
            self.lua.eval("__run_filter")(f, proxy, self.lua.table_from(config))
            return host
        except LuaError as e:
            self._fail(e)
        finally:
            self.host = None


def _clean_err(e) -> str:
    s = dec(str(e))
    return s.replace("\r", "")


def _perl_to_py(p: str) -> str:
    p = p.replace("(?<!", "(?<!")  # same syntax
    p = _re.sub(r"\[\[:alpha:\]\]", "[^\\\\W\\\\d_]", p)
    p = _re.sub(r"\[\[:digit:\]\]", "\\\\d", p)
    p = _re.sub(r"\[\[:space:\]\]", "\\\\s", p)
    p = _re.sub(r"\(\?<([A-Za-z_]\w*)>", r"(?P<\1>", p)
    p = p.replace("\\Z", "\\Z")
    return p


_RE_UNICODE_PRELUDE = r"""
function __install_modules(B)
  local raw = B.re_raw
  local re = {ICASE = 1, NOSUB = 2, NO_MOD_M = 4, MOD_S = 8, MOD_M = 16, EXTENDED = 32, NORMAL = 0, ECMA = 0, PERL = 0}
  local function flagsum(f) return f or 0 end
  function re.find(s, pat, flags) return raw.find(s, pat, flagsum(flags)) end
  function re.match(s, pat, flags) return raw.match(s, pat, flagsum(flags)) end
  function re.gfind(s, pat, flags)
    local all = raw.all(s, pat, flagsum(flags))
    local i = 0
    return function() i = i + 1 return all[i] end
  end
  re.gmatch = re.gfind
  function re.sub(s, pat, repl, a, b)
    local flags, count = 0, 0
    if b ~= nil then flags, count = a or 0, b else count = a or 0 end
    return raw.sub(s, pat, repl, flags, count)
  end
  function re.split(s, pat, flags) return raw.split(s, pat, flagsum(flags)) end
  function re.compile(pat, flags)
    local o = {}
    function o:find(s) return re.find(s, pat, flags) end
    function o:match(s) return re.match(s, pat, flags) end
    function o:gfind(s) return re.gfind(s, pat, flags) end
    o.gmatch = o.gfind
    function o:sub(s, repl, c) return re.sub(s, pat, repl, flags, c or 0) end
    function o:split(s) return re.split(s, pat, flags) end
    return o
  end
  local U = B.unicode_raw
  local unicode = {}
  for k, v in pairs(U) do unicode[k] = v end
  function unicode.chars(s)
    local list = U.chars_list(s)
    local i = 0
    return function() i = i + 1 return list[i] end
  end
  unicode.chars_list = nil
  B.re_module = function() return re end
  B.unicode_module = function() return unicode end
end
"""


# ----------------------------------------------------------------------------- discovery

def discover_scripts(dirs: list[str]) -> list[str]:
    found = []
    for d in dirs:
        if not os.path.isdir(d):
            continue
        for root, _dirs, files in os.walk(d):
            for fn in sorted(files):
                if fn.lower().endswith((".lua", ".moon")) and fn.lower().endswith(".lua"):
                    found.append(os.path.join(root, fn))
    return found


def load_scripts(dirs: list[str], extra_include_dirs: list[str] | None = None) -> list[AutomationScript]:
    out = []
    for p in discover_scripts(dirs):
        try:
            out.append(AutomationScript(p, extra_include_dirs))
        except Exception as e:  # keep going on one bad script
            s = AutomationScript.__new__(AutomationScript)
            s.path, s.filename, s.name, s.description, s.author, s.version = p, os.path.basename(p), os.path.basename(p), "", "", ""
            s.macros, s.filters, s.error = [], [], str(e)
            out.append(s)
    return out
