"""Automation çalışma zamanının dış dünyayla konuştuğu arayüz.

Lua tarafındaki her `aegisub.*` çağrısı sonunda bu sınıfın bir yöntemine
iner. Arayüz (Qt) kendi alt sınıfını verir; komut satırı ve testler
`HeadlessHost` kullanır.
"""

from __future__ import annotations

import os
import sys
import tempfile


class Host:
    """Varsayılan davranışlar: video/ses yok, diyaloglar varsayılan değerle döner."""

    def __init__(self):
        self.doc = None                  # AssDocument
        self.frame_map = None            # core.framemap.FrameMap
        self.video_path: str | None = None
        self.audio_path: str | None = None
        self.video_dims: tuple | None = None    # (w, h, ar, artype)
        self.audio_sel: tuple | None = None     # (start_ms, end_ms)
        self.cancel_requested = False
        self.log_lines: list[tuple[int, str]] = []
        self.undo_points: list[str] = []
        self.clipboard = ""
        self.dialog_answers: list = []   # HeadlessHost: sıradaki dialog cevapları

    # --------------------------------------------------------------- video
    def frame_from_ms(self, ms: int):
        return self.frame_map.frame_from_ms(int(ms)) if self.frame_map else None

    def ms_from_frame(self, frame: int):
        return self.frame_map.ms_from_frame(int(frame)) if self.frame_map else None

    def video_size(self):
        return self.video_dims

    def keyframes(self) -> list[int]:
        return list(self.frame_map.keyframes) if self.frame_map else []

    def audio_selection(self):
        return self.audio_sel

    def get_frame(self, frame: int, with_subs: bool):
        return None

    # --------------------------------------------------------------- yollar
    def file_name(self):
        if self.doc is not None and self.doc.path:
            return os.path.basename(self.doc.path)
        return None

    def decode_path(self, p: str) -> str:
        from .. import config
        doc_dir = os.path.dirname(self.doc.path) if self.doc is not None and self.doc.path else ""
        table = {
            "?data": config.app_data_dir(),
            "?user": config.user_dir(),
            "?local": config.user_dir(),
            "?temp": tempfile.gettempdir(),
            "?dictionary": os.path.join(config.user_dir(), "dictionaries"),
            "?script": doc_dir,
            "?video": os.path.dirname(self.video_path) if self.video_path else "",
            "?audio": os.path.dirname(self.audio_path) if self.audio_path else "",
        }
        for k, v in table.items():
            if p.startswith(k):
                rest = p[len(k):].lstrip("/\\")
                return os.path.join(v, rest) if rest else v
        return p

    def project_properties(self) -> dict:
        g = self.doc.garbage() if self.doc is not None else {}
        return {
            "video_file": self.video_path or g.get("Video File", ""),
            "audio_file": self.audio_path or g.get("Audio File", ""),
            "keyframes_file": g.get("Keyframes File", ""),
            "timecodes_file": g.get("Timecodes File", ""),
            "video_position": int(g.get("Video Position", "0") or 0),
            "active_row": int(g.get("Active Line", "0") or 0),
            "scroll_position": int(g.get("Scroll Position", "0") or 0),
        }

    # ------------------------------------------------------- günlük/ilerleme
    def log(self, level: int, msg: str):
        self.log_lines.append((level, msg))

    def progress_set(self, pct: float): pass
    def progress_task(self, msg: str): pass
    def progress_title(self, msg: str): pass

    def is_cancelled(self) -> bool:
        return self.cancel_requested

    def set_undo_point(self, desc: str):
        self.undo_points.append(desc)

    # ------------------------------------------------------------ diyaloglar
    def dialog_display(self, controls: list[dict], buttons: list[str] | None,
                       button_ids: dict | None):
        """(basılan_düğme, {ad: değer}) döndürür.

        Başsız kipte `dialog_answers` kuyruğundan cevap alınır; kuyruk boşsa
        ilk düğmeye basılmış ve tüm değerler varsayılan sayılır."""
        values = default_dialog_values(controls)
        if self.dialog_answers:
            ans = self.dialog_answers.pop(0)
            if callable(ans):
                return ans(controls, buttons, values)
            btn, override = ans
            values.update(override or {})
            return btn, values
        btn = buttons[0] if buttons else True
        return btn, values

    def dialog_open(self, title, default_file, default_dir, wildcards, multiple, must_exist):
        return None

    def dialog_save(self, title, default_file, default_dir, wildcards, no_prompt):
        return None

    # ------------------------------------------------------------ pano/arayüz
    def clipboard_get(self) -> str:
        return self.clipboard

    def clipboard_set(self, s: str) -> bool:
        self.clipboard = s
        return True

    def gui_get_cursor(self): return None
    def gui_set_cursor(self, pos: int): pass
    def gui_get_selection(self): return None
    def gui_set_selection(self, a: int, b: int): pass
    def gui_is_modified(self): return False


class HeadlessHost(Host):
    """Komut satırı/test: günlükleri stderr'e de basar."""

    def __init__(self, echo: bool = False):
        super().__init__()
        self.echo = echo

    def log(self, level, msg):
        super().log(level, msg)
        if self.echo:
            sys.stderr.write(msg if msg.endswith("\n") else msg + "\n")


def default_dialog_values(controls: list[dict]) -> dict:
    """Aegisub dialog.display sonuç tablosunun varsayılan değerleri."""
    out = {}
    for c in controls:
        name = c.get("name")
        if not name:
            continue
        cls = str(c.get("class", "")).lower()
        if cls in ("edit", "textbox"):
            out[name] = str(c.get("text", c.get("value", "")) or "")
        elif cls in ("intedit",):
            out[name] = int(c.get("value", 0) or 0)
        elif cls in ("floatedit",):
            out[name] = float(c.get("value", 0) or 0)
        elif cls == "checkbox":
            out[name] = bool(c.get("value", False))
        elif cls == "dropdown":
            out[name] = str(c.get("value", "") or "")
        elif cls in ("color", "coloralpha", "alpha"):
            out[name] = str(c.get("value", "") or "")
        elif cls == "label":
            continue
        else:
            out[name] = c.get("value")
    return out
