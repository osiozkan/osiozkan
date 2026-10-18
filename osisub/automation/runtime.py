"""Aegisub Automation 4 (Lua) compatible runtime built on lupa/LuaJIT."""
from __future__ import annotations
import os
import re as _re
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

try:
    from lupa.luajit21 import LuaRuntime
except Exception:  # pragma: no cover
    from lupa.lua51 import LuaRuntime  # type: ignore

from ..core.ass import Document, Event, Style
from ..core.timeutil import FrameRate

LUA_DIR = os.path.join(os.path.dirname(__file__), "lua")


def _s(v: Any) -> str:
    if isinstance(v, bytes):
        return v.decode("utf-8", "replace")
    return "" if v is None else str(v)


def _b(v: Any) -> bytes:
    if isinstance(v, bytes):
        return v
    return str(v).encode("utf-8")


def _num(v: Any) -> float:
    try:
        f = float(v)
    except (TypeError, ValueError):
        return 0
    return int(f) if f == int(f) else f


class AutomationError(Exception):
    pass


class Cancelled(Exception):
    pass


@dataclass
class Macro:
    script: "Script"
    index: int
    name: str
    description: str
    lua: Any = None
    kind: str = "macro"  # or "filter"
    priority: int = 0

    @property
    def menu_path(self) -> list[str]:
        return [p for p in self.name.split("/") if p]


@dataclass
class Script:
    path: str
    name: str = ""
    description: str = ""
    author: str = ""
    version: str = ""
    error: str = ""
    macros: list[Macro] = field(default_factory=list)
    filters: list[Macro] = field(default_factory=list)
    lua: Any = None


@dataclass
class RunResult:
    ok: bool
    document: Optional[Document] = None
    selection: Optional[list[int]] = None  # 0-based event indices
    active: Optional[int] = None
    error: str = ""
    cancelled: bool = False
    log: list[tuple[int, str]] = field(default_factory=list)


# ------------------------------------------------------------------ text measurement
class TextMeasurer:
    """Measure text extents for a style (script-resolution pixels)."""

    def __init__(self) -> None:
        self._qt = None
        self._cache: dict = {}

    def _init_qt(self):
        if self._qt is not None:
            return self._qt
        try:
            from PySide6.QtGui import QGuiApplication, QFont, QFontMetricsF  # noqa
            if QGuiApplication.instance() is None:
                os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
                self._app = QGuiApplication([])
            self._qt = (QFont, QFontMetricsF)
        except Exception:
            self._qt = False
        return self._qt

    def measure(self, st: dict, text: str) -> tuple[float, float, float, float]:
        fontname = st.get("fontname", "Arial")
        size = float(st.get("fontsize", 40) or 40)
        bold, italic = bool(st.get("bold")), bool(st.get("italic"))
        sx = float(st.get("scale_x", 100) or 100) / 100.0
        sy = float(st.get("scale_y", 100) or 100) / 100.0
        spacing = float(st.get("spacing", 0) or 0)
        key = (fontname, size, bold, italic, text, sx, sy, spacing)
        if key in self._cache:
            return self._cache[key]
        qt = self._init_qt()
        if qt:
            QFont, QFontMetricsF = qt
            f = QFont(fontname)
            f.setPixelSize(max(1, int(round(size))))
            f.setBold(bold)
            f.setItalic(italic)
            fm = QFontMetricsF(f)
            w = fm.horizontalAdvance(text) if text else 0.0
            h = fm.height()
            d = fm.descent()
            el = fm.leading()
        else:
            w = len(text) * size * 0.5
            h, d, el = size * 1.2, size * 0.2, 0.0
        n = len(text)
        res = (w * sx + spacing * n, h * sy, d * sy, el * sy)
        self._cache[key] = res
        return res


# ------------------------------------------------------------------ engine
class Engine:
    def __init__(self, include_dirs: Optional[list[str]] = None,
                 dialog_handler: Optional[Callable] = None,
                 file_dialog_handler: Optional[Callable] = None,
                 log_handler: Optional[Callable[[int, str], None]] = None,
                 progress_handler: Optional[Callable[[str, Any], None]] = None):
        self.rt = LuaRuntime(encoding=None, unpack_returned_tuples=True, register_eval=False)
        self.include_dirs = [os.path.join(LUA_DIR, "include")] + list(include_dirs or [])
        self.dialog_handler = dialog_handler
        self.file_dialog_handler = file_dialog_handler
        self.log_handler = log_handler
        self.progress_handler = progress_handler
        self.measurer = TextMeasurer()
        self.fps = FrameRate(23.976)
        self.video_res: Optional[tuple[int, int]] = None
        self.keyframe_list: list[int] = []
        self.clipboard_text = ""
        self.audio_sel: Optional[tuple[int, int]] = None
        self.project_props: dict = {}
        self.cancel_flag = False
        self.log: list[tuple[int, str]] = []
        self.undo_points: list[str] = []
        self._regexes: list = []
        self._doc: Optional[Document] = None
        self._build_host()
        self.lua = self._load_prelude()
        self.lua.set_include_dirs(self.rt.table_from([_b(d) for d in self.include_dirs]))
        self.scripts: list[Script] = []

    # ---- host bridge
    def _build_host(self) -> None:
        rt = self.rt
        L = self

        def log(level, msg):
            m = _s(msg)
            L.log.append((int(level), m))
            if L.log_handler:
                L.log_handler(int(level), m)

        def regex_flags(f):
            f = int(f or 0)
            fl = _re.UNICODE
            if f & 1:
                fl |= _re.I
            if not f & 4:
                pass
            if f & 8:
                fl |= _re.S
            if f & 16:
                fl |= _re.M
            return fl

        def re_compile(pat, flags):
            try:
                L._regexes.append(_re.compile(_s(pat), regex_flags(flags)))
            except _re.error as e:
                raise RuntimeError(f"regex error: {e}")
            return len(L._regexes)

        def byte_pos(s, ci):
            return len(s[:ci].encode("utf-8"))

        def mk_match(s, m):
            groups = [m] if m is not None else []
            tbl = []
            for gi in range(0, (m.re.groups or 0) + 1):
                if m.start(gi) < 0:
                    tbl.append({b"str": b"", b"first": 0, b"last": 0})
                    continue
                tbl.append({b"str": _b(m.group(gi)), b"first": byte_pos(s, m.start(gi)) + 1,
                            b"last": byte_pos(s, m.end(gi))})
            return tbl

        def to_lua_matches(tbls):
            return rt.table_from([rt.table_from([rt.table_from(d) for d in t]) for t in tbls])

        def re_find(rid, s, first_only=False):
            rx = L._regexes[int(rid) - 1]
            st = _s(s)
            res = []
            for m in rx.finditer(st):
                res.append(mk_match(st, m))
                if first_only:
                    break
            if not res:
                return None
            if first_only:
                return rt.table_from([rt.table_from(d) for d in res[0]])
            # Aegisub returns a flat array of matches: list of {str,first,last}
            return rt.table_from([rt.table_from(t[0]) for t in res])

        def conv_repl(r):
            r = _s(r)
            return _re.sub(r"\$(\d+)|\\(\d+)", lambda m: "\\g<%s>" % (m.group(1) or m.group(2)), r.replace("\\", "\\\\") if False else r)

        def re_sub(rid, s, repl, mx):
            rx = L._regexes[int(rid) - 1]
            st = _s(s)
            if callable(repl) or not isinstance(repl, (bytes, str)):
                def fn(m):
                    mt = mk_match(st, m)
                    tbl = dict(mt[0])
                    for gi, g in enumerate(mt[1:], 1):
                        tbl[gi] = rt.table_from(g)
                    return _s(repl(rt.table_from(tbl)))
                return _b(rx.sub(fn, st, count=int(mx or 0)))
            return _b(rx.sub(conv_repl(repl), st, count=int(mx or 0)))

        def re_split(rid, s):
            rx = L._regexes[int(rid) - 1]
            return rt.table_from([_b(x or "") for x in rx.split(_s(s))])

        def style_to_dict(t):
            d = {}
            for k, v in t.items():
                d[_s(k)] = _s(v) if isinstance(v, bytes) else v
            return d

        def text_extents(style, text):
            w, h, d, e = L.measurer.measure(style_to_dict(style), _s(text))
            return w, h, d, e

        def dialog(dlg, buttons, ids):
            controls = [from_lua(dlg[i]) for i in range(1, len(dlg) + 1)]
            btns = [from_lua(b) for b in (list(buttons.values()) if buttons is not None else [])] or ["OK", "Cancel"]
            idsd = from_lua(ids) if ids is not None else {}
            if L.dialog_handler:
                button, results = L.dialog_handler(controls, btns, idsd)
            else:
                button, results = headless_dialog(controls, btns, idsd)
            if button is False or button is None:
                return False, None
            out = {}
            for k, v in (results or {}).items():
                out[_b(k)] = _b(v) if isinstance(v, str) else v
            return (_b(button) if isinstance(button, str) else button), rt.table_from(out)

        def file_dialog(kind, *args):
            if L.file_dialog_handler:
                r = L.file_dialog_handler(kind, [from_lua(a) for a in args])
                return _b(r) if isinstance(r, str) else (rt.table_from([_b(x) for x in r]) if r else None)
            return None

        def script_resolution():
            if L._doc is None:
                return 384, 288
            return L._doc.res

        def progress_set(v):
            if L.progress_handler:
                L.progress_handler("set", float(v))

        def progress_task(s):
            if L.progress_handler:
                L.progress_handler("task", _s(s))

        def progress_title(s):
            if L.progress_handler:
                L.progress_handler("title", _s(s))

        def video_size():
            if L.video_res:
                return L.video_res[0], L.video_res[1]
            return None

        def keyframes():
            return rt.table_from(list(L.keyframe_list))

        def audio_selection():
            return L.audio_sel if L.audio_sel else (None, None)

        def project_properties():
            return rt.table_from({_b(k): _b(v) if isinstance(v, str) else v for k, v in L.project_props.items()})

        def decode_path(p):
            p = _s(p)
            for tag, val in (("?script", os.path.dirname(L._doc.filename) if (L._doc and L._doc.filename) else ""),
                             ("?data", os.path.expanduser("~/.osisub")),
                             ("?user", os.path.expanduser("~/.osisub")),
                             ("?temp", os.environ.get("TMPDIR", "/tmp"))):
                if p.startswith(tag):
                    p = val + p[len(tag):]
            return _b(p)

        def file_name():
            if L._doc and L._doc.filename:
                return _b(os.path.basename(L._doc.filename))
            return None

        def clip_get():
            return _b(L.clipboard_text)

        def clip_set(s):
            L.clipboard_text = _s(s)

        funcs = dict(
            log=log, re_compile=re_compile, re_find=re_find, re_sub=re_sub, re_split=re_split,
            text_extents=text_extents, dialog=dialog, file_dialog=file_dialog,
            script_resolution=script_resolution, progress_set=progress_set, progress_task=progress_task,
            progress_title=progress_title, progress_cancelled=lambda: L.cancel_flag,
            video_size=video_size, keyframes=keyframes, audio_selection=audio_selection,
            project_properties=project_properties, decode_path=decode_path, file_name=file_name,
            clipboard_get=clip_get, clipboard_set=clip_set,
            frame_from_ms=lambda ms: L.fps.ms_to_frame(int(ms)),
            ms_from_frame=lambda f: L.fps.frame_to_ms(int(f)),
            str_upper=lambda s: _b(_s(s).upper()), str_lower=lambda s: _b(_s(s).lower()),
            undo_point=lambda d: L.undo_points.append(_s(d)),
        )
        self._host_funcs = {k.encode(): v for k, v in funcs.items()}

    def _load_prelude(self):
        self.rt.globals()[b"__host"] = self.rt.table_from(self._host_funcs)
        src = open(os.path.join(LUA_DIR, "prelude.lua"), "rb").read()
        return self.rt.execute(src)

    # ---- scripts
    def load_script(self, path: str) -> Script:
        lua_script = self.lua.load_script(_b(os.path.abspath(path)))
        s = Script(path=path, lua=lua_script)
        s.name = _s(lua_script[b"name"])
        s.description = _s(lua_script[b"description"])
        s.author = _s(lua_script[b"author"])
        s.version = _s(lua_script[b"version"])
        s.error = _s(lua_script[b"error"]) if lua_script[b"error"] else ""
        for i, m in lua_script[b"macros"].items():
            s.macros.append(Macro(s, int(i), _s(m[b"name"]), _s(m[b"description"]), m))
        for i, m in lua_script[b"filters"].items():
            s.filters.append(Macro(s, int(i), _s(m[b"name"]), _s(m[b"description"]), m, "filter",
                                   int(m[b"priority"] or 0)))
        self.scripts.append(s)
        return s

    def load_dir(self, directory: str) -> list[Script]:
        out = []
        if not os.path.isdir(directory):
            return out
        for fn in sorted(os.listdir(directory)):
            p = os.path.join(directory, fn)
            if os.path.isfile(p) and fn.lower().endswith(".lua"):
                out.append(self.load_script(p))
            elif os.path.isdir(p):
                for cand in (fn + ".lua", "init.lua"):
                    if os.path.isfile(os.path.join(p, cand)):
                        out.append(self.load_script(os.path.join(p, cand)))
                        break
        return out

    # ---- document <-> lines
    def _lines_from_doc(self, doc: Document):
        rt = self.rt
        lines = []
        for k, v in doc.info:
            if k == "":
                continue
            lines.append(rt.table_from({b"class": b"info", b"section": b"[Script Info]", b"key": _b(k),
                                        b"value": _b(v), b"raw": _b(f"{k}: {v}")}))
        for st in doc.styles:
            lines.append(rt.table_from(self._style_to_table(st)))
        for ev in doc.events:
            lines.append(rt.table_from(self._event_to_table(ev)))
        return lines

    @staticmethod
    def _style_to_table(st: Style) -> dict:
        return {
            b"class": b"style", b"section": b"[V4+ Styles]", b"name": _b(st.name), b"fontname": _b(st.fontname),
            b"fontsize": st.fontsize, b"color1": _b(st.primary if st.primary.endswith("&") is False else st.primary),
            b"color2": _b(st.secondary), b"color3": _b(st.outline_c), b"color4": _b(st.back),
            b"bold": st.bold, b"italic": st.italic, b"underline": st.underline, b"strikeout": st.strikeout,
            b"scale_x": st.scale_x, b"scale_y": st.scale_y, b"spacing": st.spacing, b"angle": st.angle,
            b"borderstyle": st.border_style, b"outline": st.outline, b"shadow": st.shadow, b"align": st.align,
            b"margin_l": st.margin_l, b"margin_r": st.margin_r, b"margin_t": st.margin_v, b"margin_b": st.margin_v,
            b"encoding": st.encoding, b"relative_to": 2,
        }

    @staticmethod
    def _event_to_table(ev: Event) -> dict:
        return {
            b"class": b"dialogue", b"section": b"[Events]", b"comment": ev.comment, b"layer": ev.layer,
            b"start_time": ev.start, b"end_time": ev.end, b"style": _b(ev.style), b"actor": _b(ev.actor),
            b"margin_l": ev.margin_l, b"margin_r": ev.margin_r, b"margin_t": ev.margin_v, b"margin_b": ev.margin_v,
            b"effect": _b(ev.effect), b"text": _b(ev.text),
        }

    def _doc_from_lines(self, lines, base: Document) -> Document:
        doc = Document()
        doc.filename = base.filename
        doc.garbage = list(base.garbage)
        doc.attachments = list(base.attachments)
        doc.extra_sections = list(base.extra_sections)
        # preserve comment-only info lines from the base document
        comments = [(k, v) for k, v in base.info if k == ""]
        n = len(lines)
        for i in range(1, n + 1):
            t = lines[i - 1]
            cls = _s(t[b"class"])
            if cls == "info":
                doc.info.append((_s(t[b"key"]), _s(t[b"value"])))
            elif cls == "style":
                doc.styles.append(self._style_from_table(t))
            elif cls == "dialogue":
                doc.events.append(self._event_from_table(t))
        doc.info = comments + doc.info
        return doc

    @staticmethod
    def _style_from_table(t) -> Style:
        g = lambda k, d=None: t[k] if t[k] is not None else d
        s = Style()
        s.name = _s(g(b"name", b"Default"))
        s.fontname = _s(g(b"fontname", b"Arial"))
        s.fontsize = _num(g(b"fontsize", 48))
        s.primary = _s(g(b"color1", b"&H00FFFFFF"))
        s.secondary = _s(g(b"color2", b"&H000000FF"))
        s.outline_c = _s(g(b"color3", b"&H00000000"))
        s.back = _s(g(b"color4", b"&H00000000"))
        s.bold, s.italic = bool(g(b"bold", False)), bool(g(b"italic", False))
        s.underline, s.strikeout = bool(g(b"underline", False)), bool(g(b"strikeout", False))
        s.scale_x, s.scale_y = _num(g(b"scale_x", 100)), _num(g(b"scale_y", 100))
        s.spacing, s.angle = _num(g(b"spacing", 0)), _num(g(b"angle", 0))
        s.border_style = int(_num(g(b"borderstyle", 1)))
        s.outline, s.shadow = _num(g(b"outline", 2)), _num(g(b"shadow", 2))
        s.align = int(_num(g(b"align", 2)))
        s.margin_l, s.margin_r = int(_num(g(b"margin_l", 10))), int(_num(g(b"margin_r", 10)))
        s.margin_v = int(_num(g(b"margin_t", g(b"margin_v", 10))))
        s.encoding = int(_num(g(b"encoding", 1)))
        return s

    @staticmethod
    def _event_from_table(t) -> Event:
        g = lambda k, d=None: t[k] if t[k] is not None else d
        e = Event()
        e.comment = bool(g(b"comment", False))
        e.layer = int(_num(g(b"layer", 0)))
        e.start, e.end = int(round(_num(g(b"start_time", 0)))), int(round(_num(g(b"end_time", 0))))
        e.style = _s(g(b"style", b"Default"))
        e.actor = _s(g(b"actor", b""))
        e.margin_l, e.margin_r = int(_num(g(b"margin_l", 0))), int(_num(g(b"margin_r", 0)))
        e.margin_v = int(_num(g(b"margin_t", g(b"margin_v", 0))))
        e.effect = _s(g(b"effect", b""))
        e.text = _s(g(b"text", b""))
        return e

    def _lua_array(self, items):
        return self.rt.table_from(items)

    # ---- running
    def _run(self, doc: Document, selection: list[int], active: int, fn):
        self._doc = doc
        self.cancel_flag = False
        self.log = []
        # selection is given in 1-based global line indices: events follow info+styles
        offset = len([1 for k, _ in doc.info if k != ""]) + len(doc.styles)
        lines = self._lines_from_doc(doc)
        lua_lines = self.lua.new_lines()
        for i, l in enumerate(lines, 1):
            lua_lines[i] = l
        sel = self.lua.new_lines()
        for i, x in enumerate(selection, 1):
            sel[i] = offset + x + 1
        act = offset + (active if active is not None else (selection[0] if selection else 0)) + 1
        return fn(lua_lines, sel, act, offset)

    def run_macro(self, macro: Macro, doc: Document, selection: Optional[list[int]] = None,
                  active: Optional[int] = None) -> RunResult:
        selection = list(selection or [])

        def go(lua_lines, sel, act, offset):
            r = self.lua.run_macro(macro.lua, lua_lines, sel, act)
            r = tuple(r) if isinstance(r, tuple) else (r,)
            ok, newsel, newact, err = (r + (None,) * 4)[:4]
            return ok, newsel, newact, err, lua_lines, offset

        ok, newsel, newact, err, lua_lines, offset = self._run(doc, selection, active, go)
        res = RunResult(ok=bool(ok), log=list(self.log))
        if not ok:
            res.error = _s(err)
            res.cancelled = res.error == "cancelled"
            return res
        newdoc = self._doc_from_lines([lua_lines[i] for i in range(1, len(lua_lines) + 1)], doc)
        new_offset = len([1 for k, _ in newdoc.info if k != ""]) + len(newdoc.styles)
        res.document = newdoc
        if newsel is not None:
            res.selection = [int(newsel[i]) - 1 - new_offset for i in range(1, len(newsel) + 1)
                             if int(newsel[i]) - 1 - new_offset >= 0]
        else:
            res.selection = None
        if newact is not None:
            res.active = int(newact) - 1 - new_offset
        return res

    def validate_macro(self, macro: Macro, doc: Document, selection: Optional[list[int]] = None,
                       active: Optional[int] = None) -> bool:
        if macro.lua[b"validate"] is None:
            return True

        def go(lua_lines, sel, act, offset):
            return self.lua.validate_macro(macro.lua, lua_lines, sel, act)

        r = self._run(doc, list(selection or []), active, go)
        return bool(r[0] if isinstance(r, tuple) else r)

    def run_filter(self, flt: Macro, doc: Document, config: Optional[dict] = None) -> RunResult:
        def go(lua_lines, sel, act, offset):
            cfg = self.rt.table_from({_b(k): (_b(v) if isinstance(v, str) else v) for k, v in (config or {}).items()})
            r = self.lua.run_filter(flt.lua, lua_lines, cfg)
            r = tuple(r) if isinstance(r, tuple) else (r,)
            ok, err = (r + (None,) * 2)[:2]
            return ok, err, lua_lines

        ok, err, lua_lines = self._run(doc, [], 0, go)
        res = RunResult(ok=bool(ok), log=list(self.log))
        if not ok:
            res.error = _s(err)
            return res
        res.document = self._doc_from_lines([lua_lines[i] for i in range(1, len(lua_lines) + 1)], doc)
        return res


def from_lua(v: Any) -> Any:
    """Recursively convert lupa objects to Python values."""
    if isinstance(v, bytes):
        return v.decode("utf-8", "replace")
    if v is None or isinstance(v, (int, float, bool)):
        return v
    if hasattr(v, "items"):
        d = {}
        for k, x in v.items():
            d[from_lua(k)] = from_lua(x)
        return d
    return v


def _color_result(cls: str, v: str) -> str:
    """Normalise a colour string to what Aegisub returns for the control class."""
    m = _re.match(r"^&?[Hh]?([0-9A-Fa-f]{1,8})&?$", v.strip())
    if not m:
        h = _re.match(r"^#([0-9A-Fa-f]{6})$", v.strip())
        hx = ("00" + h.group(1)[4:6] + h.group(1)[2:4] + h.group(1)[0:2]) if h else "00000000"
    else:
        hx = m.group(1).zfill(8)
        if cls == "alpha" and len(m.group(1)) <= 2:
            hx = m.group(1).zfill(2) + "000000"
    if cls == "alpha":
        return "&H%s&" % hx[0:2]
    if cls == "color":
        return "&H%s&" % hx[2:8]
    return "&H" + hx


def headless_dialog(controls: list[dict], buttons: list, ids: dict):
    """Non-interactive dialog: accept defaults and press the first (OK) button."""
    results = {}
    for c in controls:
        name = c.get("name")
        if not name:
            continue
        cls = c.get("class")
        if cls in ("color", "coloralpha", "alpha"):
            results[name] = _color_result(cls, str(c.get("value", c.get("text", "&H00FFFFFF"))))
        elif cls in ("edit", "textbox"):
            results[name] = c.get("text", c.get("value", ""))
        elif cls in ("intedit", "floatedit", "checkbox", "dropdown"):
            v = c.get("value")
            if v is None:
                if cls == "checkbox":
                    v = False
                elif cls == "dropdown":
                    items = c.get("items") or {}
                    v = items[min(items)] if items else ""
                else:
                    v = c.get("min", 0) if cls != "floatedit" else 0
            results[name] = v
    first = buttons[0] if buttons else "OK"
    ok_id = ids.get("ok") if isinstance(ids, dict) else None
    return (ok_id or first), results
