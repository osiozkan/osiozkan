"""Automation 4 çalışma zamanı: Lua betiklerini yükler, makro/süzgeç çalıştırır.

Her betik kendi Lua durumunda çalışır (Aegisub'daki gibi). LuaJIT tercih
edilir (lupa.luajit21) çünkü topluluk kütüphaneleri (Yutils, ILL ...) FFI
kullanır; yoksa lupa'nın varsayılan Lua'sına düşülür.

Değişiklikler İŞLEMSELDİR: makro, belgenin Lua kopyası üzerinde çalışır;
yalnız başarıyla biterse sonuç belgeye yazılır. Hata ya da aegisub.cancel()
durumunda belge hiç değişmez.
"""

from __future__ import annotations

import os
import time
from dataclasses import dataclass, field

from ..core.assfile import AssDocument, Event, InfoItem, RawLine, Style
from ..core.asstime import round_ms
from .host import Host
from .regex_bridge import RegexBridge

try:
    import lupa.luajit21 as _lupa
    LUA_FLAVOR = "LuaJIT 2.1"
except ImportError:  # pragma: no cover
    try:
        import lupa.luajit20 as _lupa
        LUA_FLAVOR = "LuaJIT 2.0"
    except ImportError:
        import lupa as _lupa
        LUA_FLAVOR = "Lua (JIT yok — FFI kullanan betikler çalışmaz)"

lua_type = _lupa.lua_type   # her lupa varyantının kendi lua_type'ı var

LUA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "lua")


# ---------------------------------------------------------------------------
# Dönüşüm yardımcıları
# ---------------------------------------------------------------------------

def _s(v) -> str:
    if isinstance(v, bytes):
        return v.decode("utf-8", errors="surrogateescape")
    if v is None:
        return ""
    return str(v)


def _b(v) -> bytes:
    if isinstance(v, bytes):
        return v
    return str(v).encode("utf-8", errors="surrogateescape")


def _num(v, default=0):
    if v is None:
        return default
    if isinstance(v, bool):
        return int(v)
    if isinstance(v, (int, float)):
        return v
    try:
        return float(_s(v))
    except ValueError:
        return default


def from_lua(v):
    """Lua değerini Python'a (derin) çevirir: dizi tablolar list, diğerleri dict."""
    if isinstance(v, bytes):
        return _s(v)
    if lua_type(v) == "table":
        keys = list(v.keys())
        if keys and all(isinstance(k, int) for k in keys) and sorted(keys) == list(range(1, len(keys) + 1)):
            return [from_lua(v[i]) for i in range(1, len(keys) + 1)]
        return {(_s(k) if isinstance(k, bytes) else k): from_lua(val) for k, val in v.items()}
    return v


@dataclass
class Macro:
    script: "LuaScript"
    index: int                  # Lua tarafındaki 1 tabanlı sıra
    name: str
    description: str
    has_validate: bool
    has_is_active: bool

    @property
    def menu_path(self) -> list[str]:
        """'Osi/Karaoke/Efekt' -> ['Osi', 'Karaoke', 'Efekt'] (Aegisub alt menüleri)."""
        return [p for p in self.name.split("/") if p] or [self.name]


@dataclass
class Filter:
    script: "LuaScript"
    index: int
    name: str
    description: str
    priority: int
    has_config: bool


@dataclass
class RunResult:
    status: str                         # ok | cancel | error
    error: str = ""
    changed: bool = False
    selection: list[int] | None = None  # olay (event) indeksleri, 0 tabanlı
    active: int | None = None
    undo_points: list[str] = field(default_factory=list)
    seconds: float = 0.0


# ---------------------------------------------------------------------------
# Betik
# ---------------------------------------------------------------------------

class LuaScript:
    def __init__(self, path: str, include_dirs: list[str], host: Host | None = None):
        self.path = os.path.abspath(path)
        self.include_dirs = include_dirs
        self.host = host or Host()
        self.lua = None
        self.osi = None
        self.name = os.path.basename(path)
        self.description = ""
        self.author = ""
        self.version = ""
        self.namespace = ""
        self.error: str | None = None
        self.macros: list[Macro] = []
        self.filters: list[Filter] = []
        self.regex = RegexBridge()
        self.hard_cancel = False
        self.loaded_at = 0.0

    # ------------------------------------------------------------- yükleme
    def load(self) -> bool:
        self.macros, self.filters, self.error = [], [], None
        self.regex = RegexBridge()
        if self.path.lower().endswith(".moon"):
            self.error = "MoonScript betikleri henüz desteklenmiyor (lpeg gerekli)."
            return False
        try:
            self.lua = _lupa.LuaRuntime(unpack_returned_tuples=True, encoding=None,
                                        register_eval=False)
            runner = self.lua.eval(b"function(src, name, ...) "
                                   b"local f, e = loadstring(src, name) "
                                   b"if not f then error(e, 0) end "
                                   b"return f(...) end")
            with open(os.path.join(LUA_DIR, "osisub_prelude.lua"), "rb") as f:
                prelude = f.read()
            with open(os.path.join(LUA_DIR, "osisub_karaoke.lua"), "rb") as f:
                kara = f.read()
            self.osi = runner(prelude, b"@osisub_prelude.lua", self._py_table())
            runner(kara, b"@osisub_karaoke.lua")
            script_dir = os.path.dirname(self.path)
            inc = self.lua.table_from([_b(d.replace("\\", "/")) for d in self.include_dirs])
            self.osi.set_paths(_b(script_dir.replace("\\", "/")), inc)
            self._preload_builtins()
            ok, info = self.osi.load_script(_b(self.path))
        except Exception as e:  # noqa: BLE001
            self.error = f"Lua başlatılamadı: {e}"
            return False
        if not ok:
            self.error = _s(info)
            return False
        meta = from_lua(info) or {}
        self.name = meta.get("name") or os.path.splitext(os.path.basename(self.path))[0]
        self.description = meta.get("description") or ""
        self.author = meta.get("author") or ""
        self.version = str(meta.get("version") or "")
        self.namespace = meta.get("namespace") or ""
        for i, m in self.osi.macros.items():
            self.macros.append(Macro(self, i, _s(m[b"name"]), _s(m[b"description"]),
                                     m[b"validate"] is not None, m[b"is_active"] is not None))
        for i, f in self.osi.filters.items():
            self.filters.append(Filter(self, i, _s(f[b"name"]), _s(f[b"description"]),
                                       int(_num(f[b"priority"])), f[b"config"] is not None))
        self.loaded_at = time.time()
        return True

    def _preload_builtins(self):
        inc = os.path.join(LUA_DIR, "include", "aegisub")
        for mod in ("util", "unicode", "re", "lfs", "clipboard"):
            path = _b(os.path.join(inc, mod + ".lua").replace("\\", "/"))
            self.osi.preload(_b("aegisub." + mod), path)
        self.osi.preload(b"lfs", _b(os.path.join(inc, "lfs.lua").replace("\\", "/")))
        self.lua.execute(b"unicode = require 'aegisub.unicode'")

    def reload(self) -> bool:
        return self.load()

    # ---------------------------------------------------- Python geri çağırımları
    def _py_table(self):
        L = self.lua

        def ret(v):
            if isinstance(v, tuple):
                return tuple(conv(x) for x in v)
            return conv(v)

        def conv(v):
            if v is None or isinstance(v, (bool, int, float, bytes)):
                return v
            if isinstance(v, str):
                return _b(v)
            if isinstance(v, dict):
                return L.table_from({_b(k) if isinstance(k, str) else k: conv(x) for k, x in v.items()})
            if isinstance(v, (list, tuple)):
                return L.table_from([conv(x) for x in v])
            return v

        def wrap(fn):
            def inner(*args):
                return ret(fn(*args))
            return inner

        h = lambda: self.host  # noqa: E731 — çalıştırma anında güncel host

        def text_extents(style, text):
            st = {
                "fontname": _s(style[b"fontname"]), "fontsize": _num(style[b"fontsize"], 20),
                "bold": bool(style[b"bold"]), "italic": bool(style[b"italic"]),
                "underline": bool(style[b"underline"]), "strikeout": bool(style[b"strikeout"]),
                "scale_x": _num(style[b"scale_x"], 100), "scale_y": _num(style[b"scale_y"], 100),
                "spacing": _num(style[b"spacing"], 0),
            }
            from .textmetrics import text_extents as te
            return te(st, _s(text))

        def video_size():
            v = h().video_size()
            return tuple(v) if v else None

        def audio_sel():
            v = h().audio_selection()
            return tuple(v) if v else None

        def dialog_display(dialog, buttons, button_ids):
            controls = []
            for _, c in dialog.items():
                if lua_type(c) == "table":
                    controls.append(from_lua(c))
            btns = from_lua(buttons) if buttons is not None else None
            ids = from_lua(button_ids) if button_ids is not None else None
            pressed, values = h().dialog_display(controls, btns, ids)
            return pressed, values

        def file_exists(p):
            return os.path.isfile(_s(p))

        def read_file(p):
            try:
                with open(_s(p), "rb") as f:
                    return f.read()
            except OSError:
                return None

        def lfs_attributes(p):
            p = _s(p)
            try:
                st = os.stat(p)
            except OSError as e:
                return None, None, None, f"{p}: {e.strerror}"
            mode = "directory" if os.path.isdir(p) else "file"
            return mode, int(st.st_mtime), int(st.st_size), None

        def lfs_dir(p):
            p = _s(p)
            try:
                return [".", ".."] + sorted(os.listdir(p)), None
            except OSError as e:
                return None, f"{p}: {e.strerror}"

        def _bool_op(fn):
            def inner(p):
                try:
                    fn(_s(p))
                    return True, None
                except OSError as e:
                    return False, f"{_s(p)}: {e.strerror}"
            return inner

        def _touch(p):
            with open(p, "a"):
                os.utime(p, None)

        def dialog_open(title, dfile, ddir, wild, multi, must):
            r = h().dialog_open(_s(title), _s(dfile), _s(ddir), _s(wild), bool(multi), bool(must))
            return r

        def dialog_save(title, dfile, ddir, wild, noprompt):
            return h().dialog_save(_s(title), _s(dfile), _s(ddir), _s(wild), bool(noprompt))

        funcs = {
            "text_extents": text_extents,
            "frame_from_ms": lambda ms: h().frame_from_ms(int(ms)),
            "ms_from_frame": lambda f: h().ms_from_frame(int(f)),
            "video_size": video_size,
            "keyframes": lambda: list(h().keyframes()),
            "audio_selection": audio_sel,
            "decode_path": lambda p: h().decode_path(_s(p)),
            "file_name": lambda: h().file_name(),
            "project_properties": lambda: h().project_properties(),
            "set_undo_point": lambda d: h().set_undo_point(_s(d)),
            "get_frame": lambda f, ws: self._get_frame(int(f), bool(ws)),
            "frame_pixel": lambda fid, x, y: self._frame_pixel(int(fid), int(x), int(y)),
            "log": lambda lvl, msg: h().log(int(_num(lvl, -1)), _s(msg)),
            "progress_set": lambda p: h().progress_set(float(_num(p))),
            "progress_task": lambda s: h().progress_task(_s(s)),
            "progress_title": lambda s: h().progress_title(_s(s)),
            "is_cancelled": lambda: bool(h().is_cancelled()),
            "hard_cancel": lambda: self.hard_cancel,
            "dialog_display": dialog_display,
            "dialog_open": dialog_open,
            "dialog_save": dialog_save,
            "clipboard_get": lambda: h().clipboard_get(),
            "clipboard_set": lambda s: h().clipboard_set(_s(s)),
            "gui_get_cursor": lambda: h().gui_get_cursor(),
            "gui_set_cursor": lambda p: h().gui_set_cursor(int(p)),
            "gui_get_selection": lambda: h().gui_get_selection(),
            "gui_set_selection": lambda a, b: h().gui_set_selection(int(a), int(b)),
            "gui_is_modified": lambda: bool(h().gui_is_modified()),
            "file_exists": file_exists,
            "read_file": read_file,
            "str_upper": lambda s: _s(s).upper(),
            "str_lower": lambda s: _s(s).lower(),
            "str_fold": lambda s: _s(s).casefold(),
            "re_compile": lambda p, f: self.regex.compile(p, int(_num(f))),
            "re_search": lambda hh, s, st: self.regex.search(hh, s, st),
            "re_match": lambda hh, s, st: self.regex.match(hh, s, st),
            "re_replace": lambda hh, r, s, n: self.regex.replace(hh, r, s, n),
            "lfs_attributes": lfs_attributes,
            "lfs_dir": lfs_dir,
            "lfs_chdir": _bool_op(os.chdir),
            "lfs_currentdir": lambda: os.getcwd(),
            "lfs_mkdir": _bool_op(os.mkdir),
            "lfs_rmdir": _bool_op(os.rmdir),
            "lfs_touch": _bool_op(_touch),
        }
        return L.table_from({_b(k): wrap(v) for k, v in funcs.items()})

    # ------------------------------------------------- kare nesneleri
    def _get_frame(self, frame, with_subs):
        img = self.host.get_frame(frame, with_subs)
        if img is None:
            return None
        frames = self.__dict__.setdefault("_frames", {})
        if len(frames) > 8:
            frames.clear()
        fid = self.__dict__.get("_next_fid", 1)
        self._next_fid = fid + 1
        frames[fid] = img
        return fid, int(img.shape[1]), int(img.shape[0])

    def _frame_pixel(self, fid, x, y):
        img = self.__dict__.get("_frames", {}).get(fid)
        if img is None or not (0 <= y < img.shape[0] and 0 <= x < img.shape[1]):
            return None
        r, g, b = (int(v) for v in img[y, x, :3])
        return (r << 16) | (g << 8) | b

    # ------------------------------------------------- belge <-> Lua satırları
    def _build_lines(self, doc: AssDocument):
        L = self.lua
        items: list = []
        tables = []
        info_sec = doc.info_section
        for it in info_sec.items:
            if isinstance(it, InfoItem):
                items.append(it)
                tables.append(L.table_from({
                    b"class": b"info", b"section": _b(info_sec.name), b"raw": _b(it.serialize()),
                    b"key": _b(it.key), b"value": _b(it.value)}))
        st_sec = doc.ensure_section("styles")
        for st in st_sec.items:
            items.append(st)
            tables.append(L.table_from({
                b"class": b"style", b"section": _b(st_sec.name),
                b"raw": _b(st.serialize(st_sec.format_fields)),
                b"name": _b(st.name), b"fontname": _b(st.fontname), b"fontsize": st.fontsize,
                b"color1": _b(st.color1), b"color2": _b(st.color2),
                b"color3": _b(st.color3), b"color4": _b(st.color4),
                b"bold": st.bold, b"italic": st.italic, b"underline": st.underline,
                b"strikeout": st.strikeout, b"scale_x": st.scale_x, b"scale_y": st.scale_y,
                b"spacing": st.spacing, b"angle": st.angle, b"borderstyle": st.borderstyle,
                b"outline": st.outline, b"shadow": st.shadow, b"align": st.align,
                b"margin_l": st.margin_l, b"margin_r": st.margin_r,
                b"margin_t": st.margin_v, b"margin_b": st.margin_v, b"margin_v": st.margin_v,
                b"encoding": st.encoding, b"relative_to": 2}))
        ev_sec = doc.ensure_section("events")
        first_event = len(items)
        for ev in ev_sec.items:
            items.append(ev)
            tables.append(L.table_from({
                b"class": b"dialogue", b"section": _b(ev_sec.name),
                b"raw": _b(ev.serialize(ev_sec.format_fields)),
                b"comment": ev.comment, b"layer": ev.layer,
                b"start_time": ev.start, b"end_time": ev.end,
                b"style": _b(ev.style), b"actor": _b(ev.actor),
                b"margin_l": ev.margin_l, b"margin_r": ev.margin_r,
                b"margin_t": ev.margin_v, b"margin_b": ev.margin_b,
                b"effect": _b(ev.effect), b"text": _b(ev.text),
                b"extra": L.table_from({})}))
        return items, L.table_from(tables), first_event

    def _convert_back(self, t, src, slot, events_fmt):
        cls = _s(t[b"class"])
        if cls == "dialogue":
            ev = Event(
                comment=bool(t[b"comment"]), layer=int(_num(t[b"layer"])),
                start=round_ms(_num(t[b"start_time"])), end=round_ms(_num(t[b"end_time"])),
                style=_s(t[b"style"]) if t[b"style"] is not None else "Default",
                actor=_s(t[b"actor"]), margin_l=int(_num(t[b"margin_l"])),
                margin_r=int(_num(t[b"margin_r"])), margin_v=int(_num(t[b"margin_t"])),
                margin_b=int(_num(t[b"margin_b"])), effect=_s(t[b"effect"]), text=_s(t[b"text"]))
            if "MarginB" not in events_fmt:
                ev.margin_b = ev.margin_v
            if isinstance(src, Event):
                ev.raw, ev.sig, ev.prefix, ev.extra = src.raw, src.sig, src.prefix, src.extra
                ev.rawvals = src.rawvals
            if isinstance(slot, Event):
                ev.lead = slot.lead
            return ev
        if cls == "style":
            st = Style(
                name=_s(t[b"name"]), fontname=_s(t[b"fontname"]), fontsize=float(_num(t[b"fontsize"], 20)),
                color1=_s(t[b"color1"]), color2=_s(t[b"color2"]), color3=_s(t[b"color3"]),
                color4=_s(t[b"color4"]), bold=bool(t[b"bold"]), italic=bool(t[b"italic"]),
                underline=bool(t[b"underline"]), strikeout=bool(t[b"strikeout"]),
                scale_x=float(_num(t[b"scale_x"], 100)), scale_y=float(_num(t[b"scale_y"], 100)),
                spacing=float(_num(t[b"spacing"])), angle=float(_num(t[b"angle"])),
                borderstyle=int(_num(t[b"borderstyle"], 1)), outline=float(_num(t[b"outline"])),
                shadow=float(_num(t[b"shadow"])), align=int(_num(t[b"align"], 2)),
                margin_l=int(_num(t[b"margin_l"])), margin_r=int(_num(t[b"margin_r"])),
                margin_v=int(_num(t[b"margin_t"])), encoding=int(_num(t[b"encoding"], 1)))
            if isinstance(src, Style):
                st.raw, st.sig, st.prefix, st.extra = src.raw, src.sig, src.prefix, src.extra
                st.rawvals = src.rawvals
            if isinstance(slot, Style):
                st.lead = slot.lead
            return st
        if cls == "info":
            it = InfoItem(_s(t[b"key"]), _s(t[b"value"]))
            if isinstance(src, InfoItem):
                it.raw, it.sig = src.raw, src.sig
            return it
        return None   # bilinmeyen sınıf: Aegisub da yazmaz

    def _apply_back(self, doc: AssDocument, items: list, state):
        """Lua satır dizisini belgeye yazar. (sel eşlemesi için) satır->olay dizini döndürür."""
        lines = state[b"lines"]
        n = len(lines)
        new_info, new_styles, new_events = [], [], []
        line_to_event: dict[int, int] = {}
        ev_fmt = doc.ensure_section("events").format_fields
        meta_of = self.osi.meta_of
        for i in range(1, n + 1):
            t = lines[i]
            pristine, src, slot = meta_of(state, t)
            if pristine:
                obj = items[pristine - 1]
            else:
                obj = self._convert_back(t, items[src - 1] if src else None,
                                         items[slot - 1] if slot else None, ev_fmt)
            if isinstance(obj, Event):
                line_to_event[i] = len(new_events)
                new_events.append(obj)
            elif isinstance(obj, Style):
                new_styles.append(obj)
            elif isinstance(obj, InfoItem):
                new_info.append(obj)
        # Script Info: yorum satırlarının yerleri korunur
        sec = doc.info_section
        rebuilt, it_new = [], iter(new_info)
        for it in sec.items:
            if isinstance(it, RawLine):
                rebuilt.append(it)
            else:
                nxt = next(it_new, None)
                if nxt is not None:
                    rebuilt.append(nxt)
        rebuilt.extend(it_new)
        sec.items = rebuilt
        doc.ensure_section("styles").items = new_styles
        doc.ensure_section("events").items = new_events
        return line_to_event

    # --------------------------------------------------------- çalıştırma
    def _sel_table(self, sel_events, active_event, first_event):
        L = self.lua
        sel = L.table_from([first_event + i + 1 for i in sorted(sel_events or [])])
        active = (first_event + active_event + 1) if active_event is not None and active_event >= 0 else None
        return sel, active

    def validate(self, macro: Macro, doc: AssDocument, sel=None, active=None):
        """(etkin_mi, açıklama). Hata olursa (False, hata) döner."""
        if not macro.has_validate:
            return True, None
        items, lines, first = self._build_lines(doc)
        s, a = self._sel_table(sel, active, first)
        r = self.osi.validate_macro(macro.index, lines, s, a)
        status = _s(r[0])
        if status != "ok":
            return False, _s(r[1]) if len(r) > 1 else status
        ok = bool(r[1]) if len(r) > 1 else False
        desc = _s(r[2]) if len(r) > 2 and r[2] is not None else None
        return ok, desc

    def run_macro(self, macro: Macro, doc: AssDocument, sel=None, active=None) -> RunResult:
        t0 = time.perf_counter()
        self.hard_cancel = False
        self.host.cancel_requested = False
        self.host.doc = doc
        items, lines, first = self._build_lines(doc)
        s, a = self._sel_table(sel, active, first)
        undo_before = len(self.host.undo_points)
        out = self.osi.run_macro(macro.index, lines, s, a)
        state, status = out[0], _s(out[1])
        res = RunResult(status=status)
        res.undo_points = self.host.undo_points[undo_before:]
        if status == "error":
            res.error = _s(out[2])
        elif status == "ok":
            if state[b"modified"]:
                line_to_event = self._apply_back(doc, items, state)
                res.changed = True
            else:
                line_to_event = {first + i + 1: i for i in range(len(doc.events))}
            if len(out) > 2 and out[2] is not None and lua_type(out[2]) == "table":
                new_sel = [line_to_event[int(v)] for v in out[2].values()
                           if isinstance(v, (int, float)) and int(v) in line_to_event]
                res.selection = sorted(set(new_sel))
            if len(out) > 3 and isinstance(out[3], (int, float)) and int(out[3]) in line_to_event:
                res.active = line_to_event[int(out[3])]
        res.seconds = time.perf_counter() - t0
        return res

    def run_filter(self, flt: Filter, doc: AssDocument, config: dict | None = None) -> RunResult:
        t0 = time.perf_counter()
        self.host.doc = doc
        items, lines, _ = self._build_lines(doc)
        L = self.lua
        cfg_tbl = L.table_from({_b(k): (_b(v) if isinstance(v, str) else v)
                                for k, v in (config or {}).items()})
        out = self.osi.run_filter(flt.index, lines, cfg_tbl)
        state, status = out[0], _s(out[1])
        res = RunResult(status=status)
        if status == "error":
            res.error = _s(out[2])
        elif status == "ok" and state[b"modified"]:
            self._apply_back(doc, items, state)
            res.changed = True
        res.seconds = time.perf_counter() - t0
        return res


# ---------------------------------------------------------------------------
# Betik yöneticisi
# ---------------------------------------------------------------------------

class ScriptManager:
    def __init__(self, autoload_dirs: list[str], include_dirs: list[str], host: Host | None = None):
        self.autoload_dirs = autoload_dirs
        self.include_dirs = include_dirs
        self.host = host or Host()
        self.scripts: list[LuaScript] = []

    def set_host(self, host: Host):
        self.host = host
        for s in self.scripts:
            s.host = host

    def discover(self) -> list[str]:
        found, seen = [], set()
        for d in self.autoload_dirs:
            if not os.path.isdir(d):
                continue
            for fn in sorted(os.listdir(d), key=str.lower):
                if fn.lower().endswith((".lua", ".moon")):
                    p = os.path.join(d, fn)
                    if fn.lower() in seen:
                        continue        # aynı ad: önce gelen klasör kazanır
                    seen.add(fn.lower())
                    found.append(p)
        return found

    def load_all(self):
        self.scripts = []
        for p in self.discover():
            self.add(p)

    def add(self, path: str) -> LuaScript:
        sc = LuaScript(path, self.include_dirs, self.host)
        sc.load()
        self.scripts.append(sc)
        return sc

    def macros(self) -> list[Macro]:
        out = []
        for s in self.scripts:
            out.extend(s.macros)
        return out

    def filters(self) -> list[Filter]:
        out = []
        for s in self.scripts:
            out.extend(s.filters)
        return sorted(out, key=lambda f: -f.priority)

    def find_macro(self, name: str) -> Macro | None:
        for m in self.macros():
            if m.name == name or m.name.split("/")[-1] == name:
                return m
        return None
