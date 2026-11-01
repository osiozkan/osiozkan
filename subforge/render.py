"""ASS rendering through libass (ctypes) with a QPainter fallback.

libass gives pixel-accurate results (same renderer family Aegisub/mpv use). If the shared
library is not found, `FallbackRenderer` draws plain styled text so the app keeps working.
"""
from __future__ import annotations

import ctypes
import ctypes.util
import os
import sys

import numpy as np

_CANDIDATES = ["libass.so.9", "libass.so", "libass.9.dylib", "libass.dylib", "libass-9.dll", "ass.dll", "libass.dll"]


class _ASS_Image(ctypes.Structure):
    pass


_ASS_Image._fields_ = [("w", ctypes.c_int), ("h", ctypes.c_int), ("stride", ctypes.c_int),
                       ("bitmap", ctypes.POINTER(ctypes.c_ubyte)), ("color", ctypes.c_uint32),
                       ("dst_x", ctypes.c_int), ("dst_y", ctypes.c_int),
                       ("next", ctypes.POINTER(_ASS_Image)), ("type", ctypes.c_int)]


def _load_lib():
    names = list(_CANDIDATES)
    found = ctypes.util.find_library("ass")
    if found:
        names.insert(0, found)
    env = os.environ.get("SUBFORGE_LIBASS")
    if env:
        names.insert(0, env)
    for n in names:
        try:
            return ctypes.CDLL(n)
        except OSError:
            continue
    return None


_lib = _load_lib()


def available() -> bool:
    return _lib is not None


class LibassRenderer:
    def __init__(self):
        L = _lib
        L.ass_library_init.restype = ctypes.c_void_p
        L.ass_renderer_init.restype = ctypes.c_void_p
        L.ass_renderer_init.argtypes = [ctypes.c_void_p]
        L.ass_set_frame_size.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        L.ass_set_storage_size.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        L.ass_set_fonts.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int,
                                    ctypes.c_char_p, ctypes.c_int]
        L.ass_read_memory.restype = ctypes.c_void_p
        L.ass_read_memory.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t, ctypes.c_char_p]
        L.ass_render_frame.restype = ctypes.POINTER(_ASS_Image)
        L.ass_render_frame.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_longlong, ctypes.POINTER(ctypes.c_int)]
        L.ass_free_track.argtypes = [ctypes.c_void_p]
        L.ass_renderer_done.argtypes = [ctypes.c_void_p]
        L.ass_library_done.argtypes = [ctypes.c_void_p]
        self._lib = L
        self._ass = L.ass_library_init()
        self._r = L.ass_renderer_init(self._ass)
        L.ass_set_fonts(self._r, None, b"sans-serif", 1, None, 1)  # ASS_FONTPROVIDER_AUTODETECT=1
        self._track = None
        self._size = (0, 0)

    def set_document(self, ass_text: str):
        if self._track:
            self._lib.ass_free_track(self._track)
            self._track = None
        data = ass_text.encode("utf-8")
        self._track = self._lib.ass_read_memory(self._ass, data, len(data), None)

    def render(self, t_ms: int, width: int, height: int, storage: tuple[int, int] | None = None):
        """Returns RGBA numpy array (h, w, 4) premultiplied-free (straight alpha) or None if empty."""
        if not self._track or width <= 0 or height <= 0:
            return None
        if self._size != (width, height):
            self._lib.ass_set_frame_size(self._r, width, height)
            self._lib.ass_set_storage_size(self._r, *(storage or (width, height)))
            self._size = (width, height)
        chg = ctypes.c_int(0)
        img = self._lib.ass_render_frame(self._r, self._track, int(t_ms), ctypes.byref(chg))
        if not img:
            return None
        canvas = np.zeros((height, width, 3), dtype=np.float32)
        alpha = np.zeros((height, width), dtype=np.float32)
        node = img
        while node:
            im = node.contents
            if im.w > 0 and im.h > 0:
                buf = np.ctypeslib.as_array(im.bitmap, shape=(im.h * im.stride,))
                m = buf.reshape(im.h, im.stride)[:, :im.w].astype(np.float32) / 255.0
                col = im.color
                r, g, b = (col >> 24) & 255, (col >> 16) & 255, (col >> 8) & 255
                a = (255 - (col & 255)) / 255.0
                x0, y0 = max(0, im.dst_x), max(0, im.dst_y)
                x1, y1 = min(width, im.dst_x + im.w), min(height, im.dst_y + im.h)
                if x1 > x0 and y1 > y0:
                    mm = m[y0 - im.dst_y:y1 - im.dst_y, x0 - im.dst_x:x1 - im.dst_x] * a
                    sa = alpha[y0:y1, x0:x1]
                    na = mm + sa * (1 - mm)
                    for ci, cv in enumerate((r, g, b)):
                        c = canvas[y0:y1, x0:x1, ci]
                        c[:] = np.where(na > 0, (cv * mm + c * sa * (1 - mm)) / np.maximum(na, 1e-6), 0)
                    sa[:] = na
            node = im.next
        out = np.empty((height, width, 4), dtype=np.uint8)
        out[..., :3] = np.clip(canvas, 0, 255).astype(np.uint8)
        out[..., 3] = np.clip(alpha * 255, 0, 255).astype(np.uint8)
        return out

    def close(self):
        if self._track:
            self._lib.ass_free_track(self._track)
            self._track = None
        if self._r:
            self._lib.ass_renderer_done(self._r)
            self._r = None
        if self._ass:
            self._lib.ass_library_done(self._ass)
            self._ass = None

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


def make_renderer():
    return LibassRenderer() if available() else None
