"""Medya denetleyicisi: video/ses açma, arka planda dizinleme ve çözme.

Sıra (bplayer ile aynı mantık): video açılır açılmaz oynatıcıya verilir;
arka planda önce kare dizini (kareye tam arama + Lua kare işlevleri), sonra
ses (dalga formu + spektrogram) hazırlanır. Her biri bitince sinyal gelir.
"""

from __future__ import annotations

import os
import threading
import traceback

from PySide6.QtCore import QObject, Signal

from ..media.probe import probe


class MediaController(QObject):
    video_opened = Signal(str)
    video_closed = Signal()
    index_ready = Signal()
    audio_ready = Signal()
    spectrum_ready = Signal()
    scenes_ready = Signal()
    speech_ready = Signal()
    audio_closed = Signal()
    position_changed = Signal(int)          # kare
    progress = Signal(str)
    error = Signal(str)

    _index_done = Signal(object, object)    # (yol, FrameIndex | Exception)
    _audio_done = Signal(object, object)
    _spec_done = Signal(int)
    _scenes_done = Signal(int, object)
    _speech_done = Signal(int, object)

    def __init__(self, ctx):
        super().__init__()
        self.ctx = ctx
        self.video_path: str | None = None
        self.audio_path: str | None = None
        self.info = None                    # MediaInfo
        self.index = None                   # FrameIndex
        self.grabber = None                 # FrameGrabber
        self.audio = None                   # AudioData
        self.frame = 0
        self._gen_v = 0
        self._gen_a = 0
        self._index_done.connect(self._on_index)
        self._audio_done.connect(self._on_audio)
        self._spec_done.connect(self._on_spec)
        self._scenes_done.connect(self._on_scenes)
        self._speech_done.connect(self._on_speech)
        self.scenes: list[int] = []
        self.speech: list[tuple[int, int]] = []
        self.scenes_running = False

    # ---------------------------------------------------------------- video
    def open_video(self, path: str, with_audio: bool = True):
        path = os.path.abspath(path)
        self.close_video(emit=False)
        try:
            self.info = probe(path)
        except Exception as e:  # noqa: BLE001
            self.error.emit(f"Video açılamadı: {e}")
            return False
        if self.info.video is None:
            self.error.emit("Dosyada video akışı yok")
            return False
        self.video_path = path
        self.ctx.video_path = path
        v = self.info.video
        self.ctx.video_dims = (v.width, v.height, v.dar or (v.width / max(1, v.height)), 0)
        self.video_opened.emit(path)
        self._gen_v += 1
        gen = self._gen_v
        self.progress.emit("Kare dizini hazırlanıyor…")

        def work():
            try:
                from ..media.frameindex import build_index
                idx = build_index(path, progress=lambda p: self.progress.emit(f"Kare dizini %{int(p * 100)}"))
                self._index_done.emit((gen, path), idx)
            except Exception as e:  # noqa: BLE001
                self._index_done.emit((gen, path), RuntimeError(f"{e}\n{traceback.format_exc()}"))

        threading.Thread(target=work, name="osisub-index", daemon=True).start()
        if with_audio and self.info.audio:
            self.open_audio(path)
        return True

    def _on_index(self, key, result):
        gen, path = key
        if gen != self._gen_v:
            return
        if isinstance(result, Exception):
            self.error.emit(f"Kare dizini oluşturulamadı: {result}")
            return
        from ..media.frames import FrameGrabber
        self.index = result
        self.grabber = FrameGrabber(path, result)
        self.ctx.frame_map = result.frame_map()
        n = len(result)
        kind = "VFR" if result.is_vfr else "CFR"
        self.progress.emit(f"Video hazır: {n} kare · {result.avg_fps:.3f} fps ({kind}) · "
                           f"{len(result.keyframes)} anahtar kare")
        self.index_ready.emit()

    # ---------------------------------------------------------------- sahneler
    def detect_scenes(self, threshold: float = 10.0):
        if self.index is None or self.scenes_running:
            return False
        self.scenes_running = True
        gen, path, idx = self._gen_v, self.video_path, self.index
        self.progress.emit("Sahne değişimleri aranıyor…")

        def work():
            try:
                from ..media.scenes import detect_scenes
                fr = detect_scenes(path, idx, threshold,
                                   progress=lambda p: self.progress.emit(f"Sahne değişimleri %{int(p * 100)}"))
                self._scenes_done.emit(gen, fr)
            except Exception as e:  # noqa: BLE001
                self._scenes_done.emit(gen, RuntimeError(str(e)))

        threading.Thread(target=work, name="osisub-scenes", daemon=True).start()
        return True

    def _on_scenes(self, gen, result):
        self.scenes_running = False
        if gen != self._gen_v:
            return
        if isinstance(result, Exception):
            self.error.emit(f"Sahne algılama başarısız: {result}")
            return
        self.scenes = list(result)
        self.progress.emit(f"{len(self.scenes)} sahne değişimi bulundu")
        self.scenes_ready.emit()

    def close_video(self, emit: bool = True):
        self._gen_v += 1
        self.scenes = []
        if self.grabber is not None:
            self.grabber.close()
        self.video_path = None
        self.info = None
        self.index = None
        self.grabber = None
        self.ctx.video_path = None
        self.ctx.video_dims = None
        self.ctx.frame_map = None
        self.frame = 0
        if emit:
            self.video_closed.emit()

    # ---------------------------------------------------------------- ses
    def open_audio(self, path: str):
        path = os.path.abspath(path)
        self._gen_a += 1
        gen = self._gen_a
        self.audio_path = path
        self.ctx.audio_path = path
        self.progress.emit("Ses çözülüyor…")

        def work():
            try:
                from ..media.audio import decode_audio
                rate = int(self.ctx.config["audio_rate"] or 22050)
                ad = decode_audio(path, rate=rate, with_spectrogram=False,
                                  progress=lambda p: self.progress.emit(f"Ses %{int(p * 100)}"))
                self._audio_done.emit((gen, path), ad)
            except Exception as e:  # noqa: BLE001
                self._audio_done.emit((gen, path), RuntimeError(str(e)))

        threading.Thread(target=work, name="osisub-audio", daemon=True).start()

    def _on_audio(self, key, result):
        gen, path = key
        if gen != self._gen_a:
            return
        if isinstance(result, Exception):
            self.error.emit(f"Ses yüklenemedi: {result}")
            self.audio_path = None
            self.ctx.audio_path = None
            return
        self.audio = result
        self.progress.emit(f"Ses hazır: {result.duration_ms / 1000:.1f} sn")
        self.audio_ready.emit()
        if result.spectrogram is None:
            # Dalga formu hemen görünsün; spektrogram arkadan gelsin
            def spec():
                try:
                    from ..media.audio import ensure_spectrogram
                    ensure_spectrogram(result)
                    self._spec_done.emit(gen)
                except Exception:  # noqa: BLE001
                    pass
                self._compute_speech(result, gen)
            threading.Thread(target=spec, name="osisub-spectrum", daemon=True).start()
        else:
            self.spectrum_ready.emit()
            threading.Thread(target=self._compute_speech, args=(result, gen), daemon=True).start()

    def _compute_speech(self, ad, gen):
        try:
            from ..media.vad import speech_segments
            segs = speech_segments(ad.pcm, ad.rate)
        except Exception:  # noqa: BLE001
            segs = []
        self._speech_done.emit(gen, segs)

    def _on_speech(self, gen, segs):
        if gen == self._gen_a:
            self.speech = list(segs)
            self.speech_ready.emit()

    def _on_spec(self, gen):
        if gen == self._gen_a:
            self.spectrum_ready.emit()

    def close_audio(self):
        self._gen_a += 1
        self.audio = None
        self.speech = []
        self.audio_path = None
        self.ctx.audio_path = None
        self.audio_closed.emit()

    # ---------------------------------------------------------------- konum
    def set_frame(self, frame: int):
        if frame != self.frame:
            self.frame = frame
            self.position_changed.emit(frame)

    @property
    def frame_count(self) -> int:
        return len(self.index) if self.index is not None else 0

    def ms_of_frame(self, frame: int) -> int:
        """Karenin Aegisub zamanı (normalize ms, EXACT)."""
        if self.index is None:
            return 0
        frame = max(0, min(frame, len(self.index) - 1))
        return self.index.ms[frame]

    @property
    def position_ms(self) -> int:
        return self.ms_of_frame(self.frame)

    def frame_for_ms(self, ms: int) -> int:
        fm = self.ctx.frame_map
        if fm is None:
            return 0
        return max(0, min(fm.frame_from_ms(ms), self.frame_count - 1))
