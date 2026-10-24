"""Aegisub-compatible Lua automation host (LuaJIT via lupa)."""
from __future__ import annotations
import os, re, traceback
from dataclasses import dataclass, field
from typing import Callable, List, Optional, Tuple

from ..ass import Document, Event, Style, color_to_ass

try:
    from lupa.luajit21 import LuaRuntime
except Exception:  # pragma: no cover
    try:
        from lupa import LuaRuntime
    except Exception:
        LuaRuntime = None

HERE = os.path.dirname(os.path.abspath(__file__))
CANCEL = "__OSISUB_CANCEL__"


class AutomationError(Exception):
    pass


@dataclass
class Result:
    doc: Document
    selection: List[int]
    active: int
    log: List[str] = field(default_factory=list)


def _style_to_lua(s: Style) -> dict:
    d = {k: getattr(s, k) for k in ("name", "fontname", "fontsize", "color1", "color2", "color3", "color4",
                                    "bold", "italic", "underline", "strikeout", "scale_x", "scale_y", "spacing",
                                    "angle", "borderstyle", "outline", "shadow", "align", "margin_l", "margin_r",
                                    "margin_t", "encoding")}
    d.update(margin_b=s.margin_t, margin_v=s.margin_t, relative_to=0, vertical=False,
             **{"class": "style", "section": "V4+ Styles", "raw": s.to_line()})
    return d


def _event_to_lua(e: Event) -> dict:
    return {"class": "dialogue", "section": "Events", "comment": e.comment, "layer": e.layer,
            "start_time": e.start, "end_time": e.end, "style": e.style, "actor": e.actor,
            "margin_l": e.margin_l, "margin_r": e.margin_r, "margin_t": e.margin_t, "margin_b": e.margin_t,
            "margin_v": e.margin_t, "effect": e.effect, "text": e.text, "extra": {}, "raw": e.to_line()}


def doc_to_rows(doc: Document) -> list:
    rows = [{"class": "info", "section": "Script Info", "key": k, "value": v, "raw": f"{k}: {v}"} for k, v in doc.info]
    rows += [_style_to_lua(s) for s in doc.styles]
    rows += [_event_to_lua(e) for e in doc.events]
    return rows


def _g(t, k, default=None):
    v = t[k]
    return default if v is None else v


def rows_to_doc(lua_subs, n: int, base: Document) -> Document:
    out = Document(info=[], styles=[], events=[], extra=list(base.extra))
    for i in range(1, n + 1):
        t = lua_subs[i]
        if t is None:
            continue
        cls = _g(t, "class", "")
        if cls == "info":
            out.info.append((str(t["key"]), str(_g(t, "value", ""))))
        elif cls == "style":
            s = Style()
            for k in ("name", "fontname", "encoding", "borderstyle", "align", "margin_l", "margin_r"):
                v = t[k]
                if v is not None:
                    setattr(s, k, int(v) if k in ("encoding", "borderstyle", "align", "margin_l", "margin_r") else str(v))
            s.margin_t = int(_g(t, "margin_t", _g(t, "margin_v", s.margin_t)))
            for k in ("fontsize", "scale_x", "scale_y", "spacing", "angle", "outline", "shadow"):
                if t[k] is not None:
                    setattr(s, k, float(t[k]))
            for k in ("bold", "italic", "underline", "strikeout"):
                setattr(s, k, bool(t[k]))
            for k in ("color1", "color2", "color3", "color4"):
                if t[k] is not None:
                    setattr(s, k, color_to_ass(str(t[k])))
            out.styles.append(s)
        elif cls == "dialogue":
            out.events.append(Event(
                comment=bool(t["comment"]), layer=int(_g(t, "layer", 0)),
                start=int(round(_g(t, "start_time", 0))), end=int(round(_g(t, "end_time", 0))),
                style=str(_g(t, "style", "Default")), actor=str(_g(t, "actor", "")),
                margin_l=int(_g(t, "margin_l", 0)), margin_r=int(_g(t, "margin_r", 0)),
                margin_t=int(_g(t, "margin_t", _g(t, "margin_v", 0))),
                effect=str(_g(t, "effect", "")), text=str(_g(t, "text", ""))))
    if not out.info:
        out.info = list(base.info)
    if not out.styles:
        out.styles = [Style()]
    return out


def approx_extents(style: dict, text: str):
    """Fallback text metrics when no font engine is available (PIL used when present)."""
    size = float(style.get("fontsize", 40) or 40)
    sx = float(style.get("scale_x", 100) or 100) / 100.0
    sy = float(style.get("scale_y", 100) or 100) / 100.0
    sp = float(style.get("spacing", 0) or 0)
    w = _measure(style, text)
    if w is None:
        w = sum((0.62 if ch.isupper() else 0.28 if ch in " .,:;'!|il" else 0.5) * size for ch in text)
    return w * sx + sp * len(text), size * sy, size * 0.2 * sy, 0.0


_font_cache = {}


def _measure(style, text):
    try:
        from PIL import ImageFont
    except Exception:
        return None
    key = (style.get("fontname"), int(style.get("fontsize", 40)), bool(style.get("bold")))
    f = _font_cache.get(key)
    if f is None:
        f = False
        for cand in (key[0] + (" Bold" if key[2] else ""), key[0]):
            for name in (cand + ".ttf", cand.replace(" ", "") + ".ttf", cand):
                try:
                    f = ImageFont.truetype(name, key[1]); break
                except Exception:
                    continue
            if f:
                break
        _font_cache[key] = f
    return f.getlength(text) if f else None


class Script:
    """One loaded Lua automation file."""

    def __init__(self, path: str, include_dirs: Optional[List[str]] = None, host_cb: Optional[dict] = None):
        if LuaRuntime is None:
            raise AutomationError("lupa kurulu değil: pip install lupa")
        self.path = os.path.abspath(path)
        self.logs: List[str] = []
        self.cb = host_cb or {}
        self.lua = LuaRuntime(unpack_returned_tuples=True, register_eval=False, encoding="utf-8")
        self.include_dirs = [os.path.dirname(self.path), HERE] + (include_dirs or [])
        self._setup()
        self.macros, self.filters = self._load()

    # ---------- host callbacks
    def _setup(self):
        lua = self.lua
        L = lambda d: lua.table_from(d, recursive=True)
        host = {
            "progress": lambda v: self.cb.get("progress", lambda x: None)(v),
            "task": lambda t: self.cb.get("task", lambda x: None)(t),
            "title": lambda t: None,
            "log": lambda lvl, s: self.logs.append(str(s)),
            "include_dirs": lua.table_from(self.include_dirs),
            "text_extents": lambda style, text: approx_extents(
                {k: style[k] for k in ("fontname", "fontsize", "scale_x", "scale_y", "spacing", "bold")
                 if style[k] is not None}, str(text)),
            "frame_from_ms": lambda ms: int(ms * self.cb.get("fps", 23.976) / 1000.0) if self.cb.get("fps", 23.976) else None,
            "ms_from_frame": lambda f: int(round(f * 1000.0 / self.cb.get("fps", 23.976))),
            "video_size": lambda: (self.cb.get("video_w", 1920), self.cb.get("video_h", 1080), 1.0, 0),
            "decode_path": lambda p: str(p).replace("?script", os.path.dirname(self.path)),
            "project_properties": lambda: L({"video_file": "", "audio_file": "", "keyframes_file": "", "timecodes_file": ""}),
            "u_len": lambda s: len(str(s)),
            "u_codepoint": lambda c: ord(str(c)[0]) if c else 0,
            "u_upper": lambda s: str(s).upper(),
            "u_lower": lambda s: str(s).lower(),
            "re_find": lambda s, p, f: self._re(s, p, f, False),
            "re_match": lambda s, p, f: self._re(s, p, f, True),
            "re_sub": self._re_sub,
            "dialog": lambda dlg, btns: self._dialog(dlg, btns),
        }
        lua.globals()["__host"] = lua.table_from(host)
        lua.execute("package.path = package.path .. ';'")
        for d in self.include_dirs:
            lua.execute("package.path = package.path .. [[%s/?.lua;]]" % d)
        with open(os.path.join(HERE, "prelude.lua"), encoding="utf-8") as f:
            lua.execute(f.read())
        lua.execute("dofile([[%s]])" % os.path.join(HERE, "utils.lua").replace("\\", "/"))

    def _dialog(self, dlg, btns):
        fn = self.cb.get("dialog")
        if not fn:
            return None
        # convert lua tables to python for the host UI; expect (button, {name:value}) back
        ctrls = [{k: (v if not hasattr(v, "items") else dict(v.items())) for k, v in c.items()} for c in dlg.values()]
        buttons = list(btns.values()) if btns is not None else None
        res = fn(ctrls, buttons)
        if res is None:
            return None
        b, vals = res
        return self.lua.table_from([b, self.lua.table_from(vals)])

    @staticmethod
    def _flags(f):
        return re.I if int(f) & 1 else 0

    def _re(self, s, p, f, anchored):
        pat = re.compile(str(p), self._flags(f))
        m = pat.match(str(s)) if anchored else pat.search(str(s))
        if not m:
            return None
        groups = [[m.group(0), m.start() + 1, m.end()]] + [
            [g or "", m.start(i + 1) + 1, m.end(i + 1)] for i, g in enumerate(m.groups())]
        return self.lua.table_from([self.lua.table_from(x) for x in groups])

    def _re_sub(self, s, p, r, count, f):
        pat = re.compile(str(p), self._flags(f))
        rep = re.sub(r"\\(\d)", r"\\g<\1>", str(r))
        return pat.sub(rep, str(s), count=int(count))

    def _load(self):
        lua = self.lua
        with open(self.path, encoding="utf-8-sig") as fh:
            src = fh.read()
        # script-level globals commonly used by scripts
        lua.execute("script_name, script_description, script_author, script_version = nil, nil, nil, nil")
        chunk = lua.eval("function(src, name) local f, e = loadstring(src, name) if not f then error(e, 0) end return f end")
        try:
            chunk(src, "@" + self.path)()
        except Exception as ex:
            raise AutomationError(f"{os.path.basename(self.path)}: {ex}")
        g = lua.globals()
        self.name = g["script_name"] or os.path.basename(self.path)
        self.description = g["script_description"] or ""
        self.author = g["script_author"] or ""
        self.version = g["script_version"] or ""
        macros = [dict(name=m["name"], description=m["description"], idx=i + 1)
                  for i, m in enumerate(g["aegisub"]["__macros"].values())]
        filters = [dict(name=m["name"], description=m["description"], idx=i + 1)
                   for i, m in enumerate(g["aegisub"]["__filters"].values())]
        return macros, filters

    # ---------- execution
    def _run(self, kind: str, idx: int, doc: Document, selection: List[int], active: int, config=None) -> Result:
        lua = self.lua
        g = lua.globals()
        reg = g["aegisub"]["__macros"] if kind == "macro" else g["aegisub"]["__filters"]
        item = reg[idx]
        rows = lua.table_from([lua.table_from(r) for r in doc_to_rows(doc)])
        subs = g["__make_subs"](rows)
        off = len(doc.info) + len(doc.styles)  # selection is in event indices; Lua sees the whole file
        sel = lua.table_from([i + 1 + off for i in selection])
        self.logs.clear()
        try:
            if kind == "macro":
                ret = item["fn"](subs, sel, active + 1 + off)
            else:
                ret = item["fn"](subs, lua.table_from(config or {}))
        except Exception as ex:
            msg = str(ex)
            if CANCEL in msg:
                raise AutomationError("İptal edildi")
            raise AutomationError(msg)
        new_doc = rows_to_doc(subs, len(subs), doc)
        off2 = len(new_doc.info) + len(new_doc.styles)
        new_sel, new_act = list(selection), active
        if ret is not None:
            vals = ret if isinstance(ret, tuple) else (ret,)
            if vals and hasattr(vals[0], "values"):
                new_sel = [int(v) - 1 - off2 for v in vals[0].values()]
            elif vals and isinstance(vals[0], (int, float)) and not isinstance(vals[0], bool):
                new_sel = [int(vals[0]) - 1 - off2]
            if len(vals) > 1 and isinstance(vals[1], (int, float)):
                new_act = int(vals[1]) - 1 - off2
        return Result(new_doc, new_sel, new_act, list(self.logs))

    def run_macro(self, idx: int, doc: Document, selection: List[int], active: int = 0) -> Result:
        return self._run("macro", idx, doc, selection, active)

    def run_filter(self, idx: int, doc: Document, config=None) -> Result:
        return self._run("filter", idx, doc, [], 0, config)


def discover(dirs: List[str]) -> List[str]:
    out = []
    for d in dirs:
        if not os.path.isdir(d):
            continue
        for root, _, files in os.walk(d):
            if os.path.basename(root) == "include":
                continue
            out += [os.path.join(root, f) for f in sorted(files) if f.lower().endswith(".lua")]
    return out
