"""Aegisub uyumlu Lua otomasyon çalışma zamanı (LuaJIT 2.1 / lupa)."""
from __future__ import annotations
import os
import re as _re
from dataclasses import dataclass
from typing import Callable, Optional

try:
    from lupa import luajit21 as _lua
except Exception:  # pragma: no cover
    from lupa import lua51 as _lua
LuaError = _lua.LuaError

from ..core.ass import AssFile, Line, Style

HERE = os.path.dirname(os.path.abspath(__file__))
LIB = os.path.join(HERE, "lib")


class LuaScriptError(Exception):
    pass


class Cancelled(Exception):
    pass


@dataclass
class MacroInfo:
    name: str
    description: str
    kind: str          # macro | filter | generator
    ref: object        # Lua tablosu
    script: str = ""


def _approx_extents(style: dict, text: str):
    """Yazı tipi ölçüsü yoksa kaba tahmin (GUI Qt ölçümü ile değiştirir)."""
    size = float(style.get("fontsize", 40) or 40)
    sx = float(style.get("scale_x", 100) or 100) / 100
    sp = float(style.get("spacing", 0) or 0)
    w = 0.0
    for ch in text:
        o = ord(ch)
        em = 1.0 if (0x3000 <= o <= 0x9FFF or 0xFF00 <= o <= 0xFFEF) else (0.3 if ch == " " else 0.55)
        w += em * size * sx + sp
    return w, size * 1.2, size * 0.2, 0.0


class LuaRuntime:
    def __init__(self, *, log: Callable[[str], None] | None = None,
                 dialog: Callable | None = None,
                 text_extents: Callable | None = None,
                 fps: float = 23.976, video_size=(1920, 1080), keyframes=None,
                 progress: Callable[[float], None] | None = None,
                 include_dirs: list[str] | None = None):
        self.lua = _lua.LuaRuntime(unpack_returned_tuples=True, register_eval=False)
        self.log_cb = log or (lambda s: print(s))
        self.dialog_cb = dialog
        self.extents_cb = text_extents
        self.fps = fps
        self.video_size = video_size
        self.keyframes_ms = keyframes or []
        self.progress_cb = progress or (lambda p: None)
        self.include_dirs = [LIB] + (include_dirs or [])
        self.cancel_flag = False
        self.macros: list[MacroInfo] = []
        self.clip = ""
        self._setup()

    # ---------------------------------------------------------------- kurulum
    def _setup(self):
        lua = self.lua
        g = lua.globals()
        Y = lua.table()
        g._Y = Y
        Y.log = lambda s: self.log_cb(str(s))
        Y.progress = lambda p: self.progress_cb(float(p))
        Y.task = lambda t: None
        Y.title = lambda t: None
        Y.cancelled = lambda: self.cancel_flag
        self._copy = lua.eval("function(t) local r={} for k,v in pairs(t) do if type(v)=='table' then local c={} for a,b in pairs(v) do c[a]=b end r[k]=c else r[k]=v end end return r end")
        Y.copy = lua.eval("function(t) local r={} for k,v in pairs(t) do if type(v)=='table' then local c={} for a,b in pairs(v) do c[a]=b end r[k]=c else r[k]=v end end return r end")
        Y.decode_path = lambda p: p
        Y.frame_from_ms = lambda ms: int(round(ms * self.fps / 1000.0))
        Y.ms_from_frame = lambda f: int(round(f * 1000.0 / self.fps))
        Y.video_size = lambda: self.video_size
        Y.keyframes = lambda: lua.table_from([int(k * self.fps / 1000) for k in self.keyframes_ms])
        Y.project_properties = lambda: lua.table_from({"video_file": "", "audio_file": "", "ar_mode": 0})
        Y.clip_get = lambda: self.clip
        Y.clip_set = lambda s: setattr(self, "clip", str(s))
        Y.codepoint = lambda c: ord(c) if c else 0
        Y.upper = lambda s: s.upper()
        Y.lower = lambda s: s.lower()
        Y.fileopen = lambda *a: None
        Y.filesave = lambda *a: None
        Y.find_include = self._find_include
        Y.text_extents = self._text_extents
        Y.dialog = self._dialog
        Y.re_module = self._re_module
        with open(os.path.join(HERE, "bootstrap.lua"), encoding="utf-8") as f:
            lua.execute(f.read())

    def _find_include(self, name):
        name = str(name)
        cands = [name] if os.path.isabs(name) else [os.path.join(d, name) for d in self.include_dirs]
        for p in cands:
            if os.path.isfile(p):
                with open(p, encoding="utf-8-sig") as f:
                    return f.read(), p
        return None, None

    def _text_extents(self, style, text):
        st = {k: style[k] for k in ("fontname", "fontsize", "scale_x", "scale_y", "spacing", "bold", "italic") if style[k] is not None}
        if self.extents_cb:
            r = self.extents_cb(st, str(text))
        else:
            r = _approx_extents(st, str(text))
        return tuple(float(x) for x in r)

    def _dialog(self, dlg, buttons, ids=None):
        items = [dict(v.items()) for v in dlg.values()] if dlg else []
        btns = list(buttons.values()) if buttons else ["OK", "Cancel"]
        if self.dialog_cb:
            res = self.dialog_cb(items, btns)
            if res is None:
                return False, self.lua.table()
            btn, vals = res
            return btn, self.lua.table_from(vals)
        vals = {}
        for it in items:
            if it.get("name") and it.get("class") not in ("label", "image", None):
                vals[it["name"]] = it.get("value", it.get("text"))
        return btns[0], self.lua.table_from(vals)

    def _re_module(self):
        lua = self.lua

        def mk(pattern, flags=0):
            return _re.compile(pattern, flags)

        def entry(m, g):
            return lua.table_from({"str": m.group(g) or "", "first": m.start(g) + 1, "last": m.end(g)})

        def find(s, pat, flags=0, anchored=False):
            rx = mk(pat, int(flags or 0) & _re.I)
            out = []
            it = [rx.match(s)] if anchored else rx.finditer(s)
            for m in it:
                if not m:
                    continue
                t = lua.table_from({"str": m.group(0), "first": m.start() + 1, "last": m.end(), "gcount": rx.groups})
                t[1] = entry(m, 0)
                for gi in range(1, rx.groups + 1):
                    t[gi + 1] = entry(m, gi)
                out.append(t)
            return lua.table_from(out) if out else None

        def sub(s, pat, rep, count=0, flags=0):
            rx = mk(pat, int(flags or 0) & _re.I)
            py_rep = _re.sub(r"\\(\d)", r"\\g<\1>", rep) if isinstance(rep, str) else rep
            return rx.sub(py_rep, s, count=int(count or 0))

        mod = lua.table()
        mod.find = lambda s, p, f=0: find(s, p, f)
        mod.match = lambda s, p, f=0: find(s, p, f, anchored=True)
        mod.sub = sub
        mod.gsub = sub
        mod.ICASE = _re.I
        mod.NOSUB = 0
        mod.NO_MOD_M = 0
        mod.NO_MOD_S = 0
        mod.NO_MOD_X = 0
        mod.compile = lambda p, f=0: lua.table_from({
            "find": lambda self, s: find(s, p, f), "match": lambda self, s: find(s, p, f, True),
            "sub": lambda self, s, r, c=0: sub(s, p, r, c, f), "gsub": lambda self, s, r, c=0: sub(s, p, r, c, f)})
        return mod

    # ---------------------------------------------------------------- betik yükleme
    def load_script(self, path: str) -> list[MacroInfo]:
        with open(path, encoding="utf-8-sig") as f:
            src = f.read()
        d = os.path.dirname(os.path.abspath(path))
        if d not in self.include_dirs:
            self.include_dirs.append(d)
        return self.load_source(src, os.path.basename(path), os.path.abspath(path))

    def load_source(self, src: str, name: str = "script.lua", path: str = "") -> list[MacroInfo]:
        g = self.lua.globals()
        a = g.aegisub
        before = (len(a.macros), len(a.filters), len(a.generators))
        try:
            fn = self.lua.eval("function(src, name) local f, e = loadstring(src, '@' .. name) if not f then error(e) end return f() end")
            fn(src, name)
        except LuaError as e:
            raise LuaScriptError(str(e)) from e
        found = []
        for key, kind, b in (("macros", "macro", before[0]), ("filters", "filter", before[1]),
                             ("generators", "generator", before[2])):
            tbl = a[key]
            for i in range(b + 1, len(tbl) + 1):
                m = tbl[i]
                nm = m.name if kind != "filter" else m.name
                found.append(MacroInfo(str(nm), str(m.description or ""), kind, m, path or name))
        self.macros.extend(found)
        return found

    # ---------------------------------------------------------------- AssFile <-> Lua
    def to_lua_lines(self, a: AssFile):
        lua = self.lua
        rows = []
        for k, v in a.info:
            if v == "\0comment":
                continue
            rows.append(lua.table_from({"class": "info", "section": "Script Info", "key": k, "value": v,
                                        "raw": f"{k}: {v}"}))
        for s in a.styles:
            rows.append(lua.table_from({
                "class": "style", "section": "V4+ Styles", "name": s.name, "fontname": s.fontname,
                "fontsize": s.fontsize, "color1": _c(s.primary), "color2": _c(s.secondary), "color3": _c(s.outline_c),
                "color4": _c(s.back), "bold": s.bold, "italic": s.italic, "underline": s.underline,
                "strikeout": s.strikeout, "scale_x": s.scale_x, "scale_y": s.scale_y, "spacing": s.spacing,
                "angle": s.angle, "borderstyle": s.borderstyle, "outline": s.outline, "shadow": s.shadow,
                "align": s.align, "margin_l": s.margin_l, "margin_r": s.margin_r, "margin_t": s.margin_t,
                "margin_b": s.margin_t, "encoding": s.encoding, "relative_to": 2}))
        for e in a.events:
            rows.append(lua.table_from({
                "class": "dialogue", "section": "Events", "comment": e.comment, "layer": e.layer,
                "start_time": e.start, "end_time": e.end, "style": e.style, "actor": e.actor,
                "margin_l": e.margin_l, "margin_r": e.margin_r, "margin_t": e.margin_t, "margin_b": e.margin_t,
                "effect": e.effect, "text": e.text, "extra": lua.table()}))
        return self.lua.table_from(rows)

    def from_lua_lines(self, tbl, base: AssFile) -> AssFile:
        out = AssFile(extra_sections=base.extra_sections, header_comments=base.header_comments)
        comments = [(k, v) for k, v in base.info if v == "\0comment"]
        out.info = comments[:]
        for i in range(1, len(tbl) + 1):
            r = tbl[i]
            cls = r["class"]
            if cls == "info":
                out.info.append((str(r["key"]), str(r["value"])))
            elif cls == "style":
                out.styles.append(Style(
                    name=str(r["name"]), fontname=str(r["fontname"]), fontsize=_n(r["fontsize"]),
                    primary=_u(r["color1"]), secondary=_u(r["color2"]), outline_c=_u(r["color3"]), back=_u(r["color4"]),
                    bold=bool(r["bold"]), italic=bool(r["italic"]), underline=bool(r["underline"]),
                    strikeout=bool(r["strikeout"]), scale_x=_n(r["scale_x"]), scale_y=_n(r["scale_y"]),
                    spacing=_n(r["spacing"]), angle=_n(r["angle"]), borderstyle=int(r["borderstyle"]),
                    outline=_n(r["outline"]), shadow=_n(r["shadow"]), align=int(r["align"]),
                    margin_l=int(r["margin_l"]), margin_r=int(r["margin_r"]), margin_t=int(r["margin_t"] or r["margin_b"] or 0),
                    encoding=int(r["encoding"])))
            elif cls == "dialogue":
                out.events.append(Line(
                    text=str(r["text"] or ""), start=int(r["start_time"]), end=int(r["end_time"]),
                    style=str(r["style"] or "Default"), actor=str(r["actor"] or ""), layer=int(r["layer"] or 0),
                    margin_l=int(r["margin_l"] or 0), margin_r=int(r["margin_r"] or 0),
                    margin_t=int(r["margin_t"] or r["margin_b"] or 0), effect=str(r["effect"] or ""),
                    comment=bool(r["comment"])))
        if not out.styles:
            out.styles = [Style()]
        # Info yoksa orijinali koru
        if not any(v != "\0comment" for _, v in out.info):
            out.info = base.info
        return out

    # ---------------------------------------------------------------- çalıştırma
    def run(self, macro: MacroInfo, a: AssFile, selection: list[int] | None = None,
            active: int | None = None, config: dict | None = None):
        """Makroyu çalıştırır; (yeni AssFile, yeni_seçim, yeni_aktif) döndürür. a değişmez."""
        self.cancel_flag = False
        lua = self.lua
        lines = self.to_lua_lines(a)
        subs = lua.globals()._Y.make_subs(lines)
        n_pre = len([1 for k, v in a.info if v != "\0comment"]) + len(a.styles)
        sel = lua.table_from([i + 1 + n_pre for i in (selection or [])])
        act = (active if active is not None else (selection[0] if selection else 0)) + 1 + n_pre
        try:
            if macro.kind == "filter":
                macro.ref.fn(subs, lua.table_from(config or {}))
                res = None
            elif macro.kind == "generator":
                macro.ref.fn(subs, lua.table_from(config or {}))
                res = None
            else:
                res = macro.ref.fn(subs, sel, act)
        except LuaError as e:
            if "aegisub.cancel" in str(e):
                raise Cancelled() from e
            raise LuaScriptError(str(e)) from e
        out = self.from_lua_lines(lines, a)
        n_post = len([1 for k, v in out.info if v != "\0comment"]) + len(out.styles)
        new_sel, new_act = None, None
        if res is not None:
            if hasattr(res, "values") and not isinstance(res, (str, int, float)):
                try:
                    new_sel = [int(v) - 1 - n_post for v in res.values()]
                except Exception:
                    new_sel = None
        return out, new_sel, new_act

    def validate(self, macro: MacroInfo, a: AssFile, selection: list[int]) -> bool:
        v = macro.ref.validate if macro.kind == "macro" else None
        if not v:
            return True
        lines = self.to_lua_lines(a)
        subs = lua.globals()._Y.make_subs(lines)
        n_pre = len([1 for k, vv in a.info if vv != "\0comment"]) + len(a.styles)
        try:
            r = v(subs, self.lua.table_from([i + 1 + n_pre for i in selection]), (selection[0] if selection else 0) + 1 + n_pre)
        except LuaError:
            return False
        return r is not False and r is not None


def _c(s: str) -> str:
    s = s.strip()
    return s if s.endswith("&") else s + "&"


def _u(s: str) -> str:
    return str(s).rstrip("&")


def _n(v) -> float:
    return float(v) if v is not None else 0.0
