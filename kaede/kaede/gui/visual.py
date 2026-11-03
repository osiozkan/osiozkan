"""Visual typesetting tools on the video: position, rotate Z, scale, origin, clip."""
from __future__ import annotations
import math
from typing import Optional

from PySide6.QtCore import Qt, QObject, QRectF, QPointF
from PySide6.QtGui import QPen, QColor, QBrush
from PySide6.QtWidgets import QGraphicsLineItem, QGraphicsRectItem, QGraphicsEllipseItem

from ..core import tags
from ..core.tags import set_tag, remove_tag, get_first_tag, get_pos, default_position, fmt_num

TOOLS = [("none", "⌖ Yok"), ("pos", "Konum"), ("rotz", "Döndür Z"), ("scale", "Ölçek"), ("org", "Orijin"), ("clip", "Kırp")]


class VisualTools(QObject):
    def __init__(self, win):
        super().__init__(win)
        self.win = win
        self.mode = "none"
        self.items = []
        self._drag = None

    # --- coordinate helpers
    def _res(self):
        return self.win.doc.res

    def _scale(self):
        rx, ry = self._res()
        return rx / max(1, self.win.video.vw), ry / max(1, self.win.video.vh)

    def to_script(self, x, y):
        sx, sy = self._scale()
        return x * sx, y * sy

    def to_scene(self, x, y):
        sx, sy = self._scale()
        return x / sx, y / sy

    def set_mode(self, mode):
        self.mode = mode
        self.win.video.tool_active = mode != "none"
        self.win.video.setCursor(Qt.CrossCursor if mode != "none" else Qt.ArrowCursor)
        self.redraw()

    # --- line info
    def _rows(self):
        rows = self.win.sel_rows() or ([self.win.cur_row()] if self.win.cur_row() >= 0 else [])
        return [r for r in rows if 0 <= r < len(self.win.doc.events)]

    def line_pos(self, ev):
        p = get_pos(ev.text)
        if p: return p
        st = self.win.doc.style(ev.style)
        if not st: return (self._res()[0] / 2, self._res()[1] - 50)
        an = None
        v = get_first_tag(ev.text, "an")
        if v and v.strip().isdigit(): an = int(v)
        return default_position(st, self._res(), (ev.margin_l, ev.margin_r, ev.margin_t), an)

    # --- gizmos
    def clear(self):
        for it in self.items:
            if it.scene(): it.scene().removeItem(it)
        self.items.clear()

    def redraw(self):
        self.clear()
        if self.mode == "none": return
        rows = self._rows()
        if not rows: return
        ev = self.win.doc.events[rows[0]]
        x, y = self.to_scene(*self.line_pos(ev))
        sc = self.win.video.scene_
        pen = QPen(QColor("#ffcc00"), 0); pen.setCosmetic(True)
        L = max(self.win.video.vw, self.win.video.vh) / 40
        for a, b, c, d in ((x - L, y, x + L, y), (x, y - L, x, y + L)):
            it = QGraphicsLineItem(a, b, c, d); it.setPen(pen); it.setZValue(30); sc.addItem(it); self.items.append(it)
        el = QGraphicsEllipseItem(x - L / 3, y - L / 3, 2 * L / 3, 2 * L / 3); el.setPen(pen); el.setZValue(30); sc.addItem(el); self.items.append(el)
        org = get_first_tag(ev.text, "org")
        if org and self.mode in ("rotz", "org"):
            try:
                ox, oy = [float(v) for v in org.strip("()").split(",")[:2]]
                ox, oy = self.to_scene(ox, oy)
                p2 = QPen(QColor("#4aa3ff"), 0); p2.setCosmetic(True)
                e2 = QGraphicsEllipseItem(ox - L / 3, oy - L / 3, 2 * L / 3, 2 * L / 3); e2.setPen(p2); e2.setZValue(30); sc.addItem(e2); self.items.append(e2)
                l2 = QGraphicsLineItem(ox, oy, x, y); l2.setPen(p2); l2.setZValue(30); sc.addItem(l2); self.items.append(l2)
            except ValueError:
                pass
        clip = get_first_tag(ev.text, "clip")
        if clip:
            try:
                nums = [float(v) for v in clip.strip("()").split(",")]
                if len(nums) == 4:
                    x1, y1 = self.to_scene(nums[0], nums[1]); x2, y2 = self.to_scene(nums[2], nums[3])
                    p3 = QPen(QColor("#ff5a5a"), 0); p3.setCosmetic(True)
                    r = QGraphicsRectItem(QRectF(QPointF(x1, y1), QPointF(x2, y2)).normalized()); r.setPen(p3); r.setZValue(30)
                    sc.addItem(r); self.items.append(r)
            except ValueError:
                pass

    # --- mouse
    def press(self, sx, sy, button):
        rows = self._rows()
        if not rows or self.mode == "none": return
        x, y = self.to_script(sx, sy)
        w = self.win
        if button == Qt.RightButton and self.mode == "clip":
            w.mutate("Kırpmayı kaldır", lambda d, s: [setattr(d.events[r], "text", remove_tag(remove_tag(d.events[r].text, "clip"), "iclip")) for r in rows])
            self.redraw(); return
        w.history.push({"pos": "Konum sürükle", "rotz": "Z döndür", "scale": "Ölçekle", "org": "Orijin", "clip": "Kırp"}[self.mode],
                       w.doc, w.sel_rows())
        w._last_edit_key = (None, 0.0)
        base = {}
        for r in rows:
            ev = w.doc.events[r]
            px, py = self.line_pos(ev)
            frz = get_first_tag(ev.text, "frz") or get_first_tag(ev.text, "fr") or "0"
            fx = get_first_tag(ev.text, "fscx"); fy = get_first_tag(ev.text, "fscy")
            st = w.doc.style(ev.style)
            org = get_first_tag(ev.text, "org")
            ox, oy = (px, py)
            if org:
                try: ox, oy = [float(v) for v in org.strip("()").split(",")[:2]]
                except ValueError: pass
            base[r] = dict(text=ev.text, pos=(px, py), frz=_f(frz), fscx=_f(fx, st.scale_x if st else 100),
                           fscy=_f(fy, st.scale_y if st else 100), org=(ox, oy))
        self._drag = dict(start=(x, y), base=base, rows=rows)
        self.move(sx, sy)

    def move(self, sx, sy):
        d = self._drag
        if not d: return
        x, y = self.to_script(sx, sy)
        x0, y0 = d["start"]; w = self.win
        for r in d["rows"]:
            b = d["base"][r]; ev = w.doc.events[r]; t = b["text"]
            if self.mode == "pos":
                nx, ny = b["pos"][0] + (x - x0), b["pos"][1] + (y - y0)
                if QApp_shift():  # constrain
                    if abs(x - x0) > abs(y - y0): ny = b["pos"][1]
                    else: nx = b["pos"][0]
                t = set_tag(t, "pos", f"({fmt_num(nx)},{fmt_num(ny)})")
            elif self.mode == "org":
                t = set_tag(t, "org", f"({fmt_num(x)},{fmt_num(y)})")
            elif self.mode == "rotz":
                cx, cy = b["org"]
                a0 = math.atan2(y0 - cy, x0 - cx); a1 = math.atan2(y - cy, x - cx)
                ang = b["frz"] - math.degrees(a1 - a0)
                if QApp_shift(): ang = round(ang / 15) * 15
                t = set_tag(t, "frz", fmt_num(ang % 360 if ang % 360 <= 180 else ang % 360 - 360))
            elif self.mode == "scale":
                k = (x - x0) / 200.0
                kx = max(1, b["fscx"] * (1 + k)); ky = max(1, b["fscy"] * (1 + k)) if QApp_shift() else None
                if QApp_shift() is False or ky is None:
                    ky = max(1, b["fscy"] * (1 - (y - y0) / 200.0))
                t = set_tag(t, "fscx", fmt_num(kx)); t = set_tag(t, "fscy", fmt_num(ky))
            elif self.mode == "clip":
                t = remove_tag(t, "clip")
                t = set_tag(t, "clip", f"({fmt_num(min(x0, x))},{fmt_num(min(y0, y))},{fmt_num(max(x0, x))},{fmt_num(max(y0, y))})")
            ev.text = t
        rows = d["rows"]
        w.model.refresh_rows(min(rows), max(rows)); w.mark_dirty(); w._load_edit_panel(); w._schedule_overlay(); self.redraw()

    def release(self, sx, sy):
        if self._drag:
            self.move(sx, sy); self._drag = None


def _f(v, default=0.0):
    try: return float(v)
    except (TypeError, ValueError): return float(default)


def QApp_shift() -> bool:
    from PySide6.QtWidgets import QApplication
    return bool(QApplication.keyboardModifiers() & Qt.ShiftModifier)
