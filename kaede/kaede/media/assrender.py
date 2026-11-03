"""libass renderer via ctypes -> RGBA numpy frames (same engine Aegisub uses for preview)."""
from __future__ import annotations
import ctypes
import ctypes.util
from typing import Optional

import numpy as np


class _Img(ctypes.Structure):
    pass


_Img._fields_ = [("w", ctypes.c_int), ("h", ctypes.c_int), ("stride", ctypes.c_int),
                 ("bitmap", ctypes.POINTER(ctypes.c_ubyte)), ("color", ctypes.c_uint32),
                 ("dst_x", ctypes.c_int), ("dst_y", ctypes.c_int),
                 ("next", ctypes.POINTER(_Img)), ("type", ctypes.c_int)]


def _load():
    for name in (ctypes.util.find_library("ass"), "libass.so.9", "libass.so", "libass.9.dylib", "libass-9.dll"):
        if not name:
            continue
        try:
            return ctypes.CDLL(name)
        except OSError:
            continue
    return None


_lib = _load()


_MSGCB = ctypes.CFUNCTYPE(None, ctypes.c_int, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p)
_quiet = _MSGCB(lambda level, fmt, va, data: None)


def available() -> bool:
    return _lib is not None


class AssRenderer:
    def __init__(self, width: int = 1280, height: int = 720):
        if _lib is None:
            raise RuntimeError("libass not found")
        L = _lib
        L.ass_library_init.restype = ctypes.c_void_p
        L.ass_renderer_init.restype = ctypes.c_void_p
        L.ass_renderer_init.argtypes = [ctypes.c_void_p]
        L.ass_read_memory.restype = ctypes.c_void_p
        L.ass_read_memory.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t, ctypes.c_char_p]
        L.ass_render_frame.restype = ctypes.POINTER(_Img)
        L.ass_render_frame.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_longlong, ctypes.POINTER(ctypes.c_int)]
        L.ass_set_frame_size.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        L.ass_set_storage_size.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        L.ass_set_fonts.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int,
                                    ctypes.c_char_p, ctypes.c_int]
        L.ass_free_track.argtypes = [ctypes.c_void_p]
        L.ass_renderer_done.argtypes = [ctypes.c_void_p]
        L.ass_library_done.argtypes = [ctypes.c_void_p]
        self.lib = ctypes.c_void_p(L.ass_library_init())
        self.r = ctypes.c_void_p(L.ass_renderer_init(self.lib))
        self.track = None
        self.size = (width, height)
        L.ass_set_message_cb.argtypes = [ctypes.c_void_p, _MSGCB, ctypes.c_void_p]
        L.ass_set_message_cb(self.lib, _quiet, None)
        L.ass_set_fonts(self.r, None, b"sans-serif", 1, None, 1)
        self.set_size(width, height)

    def set_size(self, w: int, h: int) -> None:
        self.size = (w, h)
        _lib.ass_set_frame_size(self.r, w, h)
        _lib.ass_set_storage_size(self.r, w, h)

    def set_script(self, text: str) -> None:
        if self.track:
            _lib.ass_free_track(self.track)
            self.track = None
        data = text.encode("utf-8")
        t = _lib.ass_read_memory(self.lib, data, len(data), None)
        self.track = ctypes.c_void_p(t) if t else None

    def render(self, ms: int, background: Optional[np.ndarray] = None) -> np.ndarray:
        """Return HxWx4 uint8 RGBA (premultiplied=False). If background (RGBA) is given, composite onto it."""
        w, h = self.size
        out = np.zeros((h, w, 4), dtype=np.float32) if background is None else background.astype(np.float32).copy()
        if not self.track:
            return out.astype(np.uint8)
        chg = ctypes.c_int(0)
        img = _lib.ass_render_frame(self.r, self.track, int(ms), ctypes.byref(chg))
        while img:
            im = img.contents
            if im.w > 0 and im.h > 0:
                buf = np.ctypeslib.as_array(im.bitmap, shape=(im.h, im.stride))[:, : im.w].astype(np.float32) / 255.0
                c = im.color
                r, g, b, a = (c >> 24) & 255, (c >> 16) & 255, (c >> 8) & 255, c & 255
                alpha = buf * (1.0 - a / 255.0)
                y0, x0 = max(0, im.dst_y), max(0, im.dst_x)
                y1, x1 = min(h, im.dst_y + im.h), min(w, im.dst_x + im.w)
                if y1 > y0 and x1 > x0:
                    al = alpha[y0 - im.dst_y:y1 - im.dst_y, x0 - im.dst_x:x1 - im.dst_x]
                    reg = out[y0:y1, x0:x1]
                    ina = reg[..., 3:4] / 255.0
                    outa = al[..., None] + ina * (1 - al[..., None])
                    col = np.array([r, g, b], dtype=np.float32)
                    num = col * al[..., None] + reg[..., :3] * ina * (1 - al[..., None])
                    reg[..., :3] = np.where(outa > 0, num / np.maximum(outa, 1e-6), 0)
                    reg[..., 3:4] = outa * 255.0
            img = im.next
        return np.clip(out, 0, 255).astype(np.uint8)

    def close(self) -> None:
        if self.track:
            _lib.ass_free_track(self.track); self.track = None
        if self.r:
            _lib.ass_renderer_done(self.r); self.r = None
        if self.lib:
            _lib.ass_library_done(self.lib); self.lib = None

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass
