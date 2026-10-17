"""Video bölmesi.

Arka uç 1 — libmpv (önerilen): pencereye gömülü oynatıcı, kareye tam arama
(hr-seek), libass ile CANLI altyazı önizlemesi: belge her değiştiğinde
(150 ms gecikmeyle) geçici .ass yazılır ve `sub-reload` yapılır.
Aegisub ms'si ilk kareyi 0 saydığı için mpv'ye sub-delay = ilk kare pts'i verilir.

Arka uç 2 — kare gösterici (libmpv yoksa): PyAV ile kare çözülüp çizilir;
kare adımlama ve satıra atlama çalışır, oynatma ve altyazı önizlemesi yoktur.
"""

from __future__ import annotations

import os
import sys
import tempfile

from PySide6.QtCore import QSize, Qt, QTimer, Signal
from PySide6.QtGui import QImage, QPainter, QPixmap
from PySide6.QtWidgets import (QHBoxLayout, QLabel, QSizePolicy, QSlider, QToolButton,
                               QVBoxLayout, QWidget)

from ..core.asstime import format_time
from . import theme

_MPV_ERR = None


def load_mpv():
    """python-mpv + libmpv yükle; Windows'ta uygulama klasörü ve bin/ DLL aramasına eklenir."""
    global _MPV_ERR
    try:
        if sys.platform.startswith("win"):
            from ..paths import bin_dirs
            for d in bin_dirs():
                if os.path.isdir(d):
                    os.environ["PATH"] = d + os.pathsep + os.environ.get("PATH", "")
                    try:
                        os.add_dll_directory(d)
                    except (AttributeError, OSError):
                        pass
        import mpv  # noqa: F401
        return mpv
    except Exception as e:  # noqa: BLE001
        _MPV_ERR = f"{type(e).__name__}: {e}"
        return None


def mpv_error() -> str | None:
    return _MPV_ERR


class _Surface(QWidget):
    """mpv'nin çizdiği yerel pencere."""

    def __init__(self):
        super().__init__()
        self.setAttribute(Qt.WA_NativeWindow)
        self.setAttribute(Qt.WA_DontCreateNativeAncestors)
        self.setAttribute(Qt.WA_OpaquePaintEvent)
        self.setStyleSheet("background: black;")
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.setMinimumSize(160, 90)


class _StillView(QWidget):
    """Yedek arka uç: tek kareyi en-boy oranını koruyarak çizer."""

    def __init__(self):
        super().__init__()
        self.img: QImage | None = None
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.setMinimumSize(160, 90)

    def set_rgb(self, arr):
        h, w, _ = arr.shape
        self._buf = arr.copy()
        self.img = QImage(self._buf.data, w, h, 3 * w, QImage.Format_RGB888)
        self.update()

    def paintEvent(self, _e):
        p = QPainter(self)
        p.fillRect(self.rect(), Qt.black)
        if self.img is not None:
            s = self.img.size().scaled(self.size(), Qt.KeepAspectRatio)
            x = (self.width() - s.width()) // 2
            y = (self.height() - s.height()) // 2
            p.drawImage(x, y, self.img.scaled(s, Qt.IgnoreAspectRatio, Qt.SmoothTransformation))
        p.end()


class VideoPanel(QWidget):
    frame_shown = Signal(int)
    _mpv_time = Signal(float)
    _mpv_pause = Signal(bool)

    def __init__(self, ctx, media, use_mpv: bool = True):
        super().__init__()
        self.ctx = ctx
        self.media = media
        self.mpv = None
        self.backend = "none"
        self.playing = False
        self._sub_loaded = False
        self._sub_file = os.path.join(tempfile.gettempdir(), f"osisub_onizleme_{os.getpid()}.ass")
        self._pending_time = None

        self.stack = QWidget()
        self.stack_lay = QVBoxLayout(self.stack)
        self.stack_lay.setContentsMargins(0, 0, 0, 0)
        self.surface = None
        self.still = None
        self.message = QLabel("Video yok — Video menüsünden aç ya da dosyayı pencereye sürükle.")
        self.message.setAlignment(Qt.AlignCenter)
        self.message.setWordWrap(True)
        self.message.setStyleSheet(f"color: {theme.colors()['muted']};")
        self.stack_lay.addWidget(self.message)

        def btn(text, tip, slot):
            b = QToolButton()
            b.setText(text)
            b.setToolTip(tip)
            b.setAutoRaise(True)
            b.clicked.connect(slot)
            return b

        self.b_prev_kf = btn("⏮", "Önceki anahtar kare (Shift+←)", lambda: self.step_keyframe(-1))
        self.b_prev = btn("◀", "Önceki kare (←)", lambda: self.step(-1))
        self.b_play = btn("▶", "Oynat / duraklat (Ctrl+P değil: video odaktayken Boşluk)", self.toggle_play)
        self.b_next = btn("▶|", "Sonraki kare (→)", lambda: self.step(1))
        self.b_next_kf = btn("⏭", "Sonraki anahtar kare (Shift+→)", lambda: self.step_keyframe(1))
        self.b_line = btn("↦", "Satırı oynat", self.play_line)
        self.pos = QLabel("—")
        self.pos.setTextInteractionFlags(Qt.TextSelectableByMouse)
        f = self.pos.font()
        f.setFamily("Consolas")
        self.pos.setFont(f)
        self.slider = QSlider(Qt.Horizontal)
        self.slider.setRange(0, 0)
        self.slider.sliderMoved.connect(self.seek_frame)
        self.slider.setTracking(True)
        bar = QHBoxLayout()
        bar.setContentsMargins(4, 2, 4, 2)
        for w in (self.b_prev_kf, self.b_prev, self.b_play, self.b_next, self.b_next_kf, self.b_line):
            bar.addWidget(w)
        bar.addWidget(self.pos)
        bar.addWidget(self.slider, 1)
        # ---- görsel tipografi araç çubuğu
        from PySide6.QtWidgets import QButtonGroup, QCheckBox, QComboBox
        from .visual import TOOLS, VisualCanvas
        self.canvas = VisualCanvas(ctx, media, self)
        self.canvas.hide()
        self.stack_lay.addWidget(self.canvas)
        tbar = QHBoxLayout()
        tbar.setContentsMargins(4, 2, 4, 2)
        tbar.setSpacing(2)
        self.tool_group = QButtonGroup(self)
        self.tool_group.setExclusive(True)
        self.tool_buttons = {}
        for key, label, tip in [("video", "Video", "Normal video görünümü (oynatma)")] + TOOLS:
            b = QToolButton()
            b.setText(label)
            b.setToolTip(tip)
            b.setCheckable(True)
            b.setAutoRaise(True)
            b.clicked.connect(lambda _=False, k=key: self.set_tool(k))
            self.tool_group.addButton(b)
            self.tool_buttons[key] = b
            tbar.addWidget(b)
        self.tool_buttons["video"].setChecked(True)
        self.inv_box = QCheckBox("Ters")
        self.inv_box.setToolTip("Clip araçları \\iclip yazsın")
        self.inv_box.toggled.connect(lambda v: setattr(self.canvas, "inverse", v))
        self.color_box = QComboBox()
        self.color_box.addItems(["1c", "2c", "3c", "4c"])
        self.color_box.setToolTip("Damlalığın yazacağı renk etiketi")
        self.color_box.currentTextChanged.connect(lambda v: setattr(self.canvas, "color_target", v))
        tbar.addStretch(1)
        tbar.addWidget(self.inv_box)
        tbar.addWidget(self.color_box)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addLayout(tbar)
        lay.addWidget(self.stack, 1)
        lay.addLayout(bar)
        self.setFocusPolicy(Qt.StrongFocus)

        self.use_mpv = use_mpv
        self._sub_timer = QTimer(self)
        self._sub_timer.setSingleShot(True)
        self._sub_timer.setInterval(150)
        self._sub_timer.timeout.connect(self.refresh_subs)
        self._ui_timer = QTimer(self)
        self._ui_timer.setInterval(33)
        self._ui_timer.timeout.connect(self._apply_time)
        self._mpv_time.connect(self._on_mpv_time)
        self._mpv_pause.connect(self._on_mpv_pause)
        self._play_until = None

        media.video_opened.connect(self._on_open)
        media.video_closed.connect(self._on_close)
        media.index_ready.connect(self._on_index)
        ctx.rows_changed.connect(lambda *_: self._sub_timer.start())
        ctx.document_reset.connect(self._sub_timer.start)
        ctx.styles_changed.connect(self._sub_timer.start)
        self._update_buttons()

    # ------------------------------------------------------------ açma/kapama
    def set_tool(self, key: str):
        """'video' = normal görünüm; diğerleri görsel tipografi tuvalini açar."""
        if key not in self.tool_buttons:
            return
        self.tool_buttons[key].setChecked(True)
        backend_view = self.surface if self.surface is not None else self.still
        if key == "video":
            self.canvas.hide()
            if backend_view is not None:
                backend_view.show()
            return
        if self.media.index is None:
            self.tool_buttons["video"].setChecked(True)
            return
        if self.playing:
            self.pause()
        self.canvas.tool = key
        if backend_view is not None:
            backend_view.hide()
        if self.message is not None and self.message.parent() is not None:
            self.message.hide()
        self.canvas.show()
        self.canvas.refresh_image()
        self.canvas.setFocus()

    @property
    def tool(self) -> str:
        return "video" if self.canvas.isHidden() else self.canvas.tool

    def _clear_stack(self):
        self.canvas.hide()
        if hasattr(self, "tool_buttons"):
            self.tool_buttons["video"].setChecked(True)
        for w in (self.surface, self.still, self.message):
            if w is not None:
                w.setParent(None)
        self.surface = self.still = None

    def _on_open(self, path):
        self._clear_stack()
        mpv_mod = load_mpv() if self.use_mpv else None
        vo = os.environ.get("OSISUB_MPV_VO")
        if mpv_mod is not None:
            try:
                self.surface = _Surface()
                self.stack_lay.addWidget(self.surface)
                kw = dict(pause=True, keep_open="yes", hr_seek="yes", hr_seek_framedrop="no",
                          input_default_bindings=False, input_vo_keyboard=False, osc=False,
                          sub_auto="no", audio_file_auto="no", idle="yes", embeddedfonts="yes",
                          log_handler=None)
                if vo:
                    kw["vo"] = vo
                    kw["ao"] = os.environ.get("OSISUB_MPV_AO", "null")
                else:
                    kw["wid"] = str(int(self.surface.winId()))
                self.mpv = mpv_mod.MPV(**{k: v for k, v in kw.items() if v is not None})
                self.mpv.observe_property("time-pos", lambda _n, v: v is not None and self._mpv_time.emit(float(v)))
                self.mpv.observe_property("pause", lambda _n, v: self._mpv_pause.emit(bool(v)))
                self.mpv.play(path)
                self.backend = "mpv"
            except Exception as e:  # noqa: BLE001
                global _MPV_ERR
                _MPV_ERR = f"{type(e).__name__}: {e}"
                self.mpv = None
                self._clear_stack()
        if self.mpv is None:
            self.still = _StillView()
            self.stack_lay.addWidget(self.still)
            self.backend = "still"
            self.ctx.status.emit("libmpv yok: video yalnız kare kare gösteriliyor (oynatma ve altyazı "
                                 "önizlemesi için libmpv gerekli)")
        self._sub_loaded = False
        self._update_buttons()

    def _on_index(self):
        idx = self.media.index
        self.slider.setRange(0, max(0, len(idx) - 1))
        if self.mpv is not None:
            try:
                self.mpv.sub_delay = idx.offset_s
            except Exception:  # noqa: BLE001
                pass
        self.refresh_subs()
        ev = self.ctx.active_event()
        self.seek_frame(self.media.frame_for_ms(ev.start) if ev else 0)
        self._update_buttons()

    def _on_close(self):
        self.stop_backend()
        self._clear_stack()
        self.message = QLabel("Video kapatıldı.")
        self.message.setAlignment(Qt.AlignCenter)
        self.stack_lay.addWidget(self.message)
        self.slider.setRange(0, 0)
        self.pos.setText("—")
        self._update_buttons()

    def stop_backend(self):
        self._ui_timer.stop()
        if self.mpv is not None:
            try:
                self.mpv.terminate()
            except Exception:  # noqa: BLE001
                pass
        self.mpv = None
        self.backend = "none"
        try:
            os.remove(self._sub_file)
        except OSError:
            pass

    def _update_buttons(self):
        has = self.media.index is not None
        for b in (self.b_prev_kf, self.b_prev, self.b_next, self.b_next_kf, self.b_line):
            b.setEnabled(has)
        self.b_play.setEnabled(has and self.mpv is not None)

    # ------------------------------------------------------------ altyazı
    def refresh_subs(self):
        if self.mpv is None:
            return
        try:
            with open(self._sub_file, "wb") as f:
                f.write(self.ctx.doc.to_bytes())
            if not self._sub_loaded:
                self.mpv.command("sub-add", self._sub_file, "select")
                self._sub_loaded = True
            else:
                self.mpv.command("sub-reload")
        except Exception as e:  # noqa: BLE001
            self.ctx.status.emit(f"Altyazı önizlemesi güncellenemedi: {e}")

    # ------------------------------------------------------------ konum
    def seek_frame(self, frame: int):
        idx = self.media.index
        if idx is None:
            return
        frame = max(0, min(int(frame), len(idx) - 1))
        self.media.set_frame(frame)
        if self.mpv is not None:
            try:
                self.mpv.command("seek", f"{idx.seconds_of(frame):.6f}", "absolute+exact")
            except Exception:  # noqa: BLE001
                pass
        elif self.still is not None and self.media.grabber is not None:
            try:
                self.still.set_rgb(self.media.grabber.get(frame))
            except Exception as e:  # noqa: BLE001
                self.ctx.status.emit(f"Kare çözülemedi: {e}")
        self._show_pos(frame)

    def seek_ms(self, ms: int):
        if self.media.index is not None:
            self.seek_frame(self.media.frame_for_ms(ms))

    def _show_pos(self, frame):
        ms = self.media.ms_of_frame(frame)
        kf = "  ◆" if self.media.index is not None and frame in self._kfset() else ""
        self.pos.setText(f"{format_time(ms)} · {frame}{kf}")
        if not self.slider.isSliderDown():
            self.slider.blockSignals(True)
            self.slider.setValue(frame)
            self.slider.blockSignals(False)
        self.frame_shown.emit(frame)

    def _kfset(self):
        idx = self.media.index
        if getattr(self, "_kf_cache_for", None) is not idx:
            self._kf_cache = set(idx.keyframes) if idx else set()
            self._kf_cache_for = idx
        return self._kf_cache

    def step(self, d: int):
        if self.playing:
            self.pause()
        self.seek_frame(self.media.frame + d)

    def step_keyframe(self, d: int):
        idx = self.media.index
        if idx is None:
            return
        import bisect
        kfs = idx.keyframes
        f = self.media.frame
        if d > 0:
            i = bisect.bisect_right(kfs, f)
            target = kfs[i] if i < len(kfs) else len(idx) - 1
        else:
            i = bisect.bisect_left(kfs, f) - 1
            target = kfs[i] if i >= 0 else 0
        self.seek_frame(target)

    # ------------------------------------------------------------ oynatma
    def toggle_play(self):
        if self.playing:
            self.pause()
        else:
            self.play()

    def play(self, until_ms: int | None = None):
        if self.mpv is None or self.media.index is None:
            return
        if not self.canvas.isHidden():
            self.set_tool("video")       # oynatma normal görünümde
        self._play_until = until_ms
        self.mpv.pause = False
        self.playing = True
        self._ui_timer.start()
        self.b_play.setText("⏸")

    def pause(self):
        if self.mpv is not None:
            self.mpv.pause = True
        self.playing = False
        self._play_until = None
        self._ui_timer.stop()
        self.b_play.setText("▶")
        # durunca kesin kareye otur
        if self._pending_time is not None:
            self._apply_time()

    def play_line(self):
        ev = self.ctx.active_event()
        if ev is None or self.media.index is None:
            return
        self.seek_frame(self.media.frame_for_ms(ev.start))
        self.play(until_ms=ev.end)

    def _on_mpv_time(self, t):
        self._pending_time = t
        if not self.playing:
            self._apply_time()

    def _on_mpv_pause(self, paused):
        if paused and self.playing:
            self.playing = False
            self._ui_timer.stop()
            self.b_play.setText("▶")

    def _apply_time(self):
        t = self._pending_time
        idx = self.media.index
        if t is None or idx is None:
            return
        self._pending_time = None
        f = idx.frame_at_seconds(t)
        if self.playing and self._play_until is not None and self.media.ms_of_frame(f) >= self._play_until:
            self.pause()
            f = max(0, self.media.frame_for_ms(self._play_until) - 1)
            self.seek_frame(f)
            return
        if f != self.media.frame:
            self.media.set_frame(f)
        self._show_pos(f)

    # ------------------------------------------------------------ dışa aktar
    def grab_frame(self, with_subs: bool = True):
        """Geçerli kareyi numpy RGB olarak döndürür (mpv ile altyazı dahil)."""
        import numpy as np
        if self.mpv is not None:
            r = self.mpv.command("screenshot-raw", "subtitles" if with_subs else "video")
            w, h, stride = r["w"], r["h"], r["stride"]
            buf = np.frombuffer(r["data"], np.uint8).reshape(h, stride)[:, : w * 4].reshape(h, w, 4)
            return buf[:, :, [2, 1, 0]].copy()      # bgr0 -> rgb
        if self.media.grabber is not None:
            return self.media.grabber.get(self.media.frame)
        return None

    def keyPressEvent(self, e):
        k = e.key()
        if k == Qt.Key_Left:
            self.step_keyframe(-1) if e.modifiers() & Qt.ShiftModifier else self.step(-1)
        elif k == Qt.Key_Right:
            self.step_keyframe(1) if e.modifiers() & Qt.ShiftModifier else self.step(1)
        elif k == Qt.Key_Space:
            self.toggle_play()
        else:
            super().keyPressEvent(e)

    def sizeHint(self):
        return QSize(640, 400)
