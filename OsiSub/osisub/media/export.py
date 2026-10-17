"""Videoyla dışa aktarma (ffmpeg): gömülü altyazıları çıkarma, videoya yakma (hardsub), MKV'ye gömme.

  subtitle_tracks(path)      ffprobe ile altyazı izleri (kodek, dil, başlık, varsayılan/zorunlu, metin mi)
  extract_cmd(...)           izi .ass/.srt olarak çıkarma komutu (ASS kopyalanır: stiller korunur)
  attachments_cmd(...)       MKV eklerini (yazı tipleri) klasöre dökme komutu
  burn_cmd(...)              libass ile yakma (x264/x265, CRF, hazır ayar, yazı tipi klasörü, ses kopya/aac)
  mux_cmd(...)               MKV: video + mevcut izler (isteğe bağlı) + yeni altyazı (dil/başlık/varsayılan)
                             + yazı tipi ekleri (mimetype ile)
  Job                        komutu arka planda çalıştırır; -progress çıktısından yüzde, iptal

Komut kurucular saf işlevlerdir (sınanabilir); çalıştırma Job ile.
"""

from __future__ import annotations

import json
import os
import subprocess
import threading
from dataclasses import dataclass

from .probe import _NO_WINDOW, tool

TEXT_CODECS = {"ass": ".ass", "ssa": ".ass", "subrip": ".srt", "srt": ".srt", "webvtt": ".vtt", "mov_text": ".srt",
               "text": ".srt"}
IMAGE_CODECS = {"hdmv_pgs_subtitle": "PGS (Blu-ray, resim)", "dvd_subtitle": "VobSub (DVD, resim)",
                "dvb_subtitle": "DVB (resim)", "xsub": "XSUB (resim)"}
FONT_MIME = {".ttf": "application/x-truetype-font", ".ttc": "application/x-truetype-font",
             ".otf": "application/vnd.ms-opentype", ".otc": "application/vnd.ms-opentype"}


@dataclass
class SubTrack:
    index: int               # dosyadaki akış numarası
    sub_index: int           # altyazı izleri arasındaki sıra (0:s:N)
    codec: str
    language: str = ""
    title: str = ""
    default: bool = False
    forced: bool = False

    @property
    def is_text(self) -> bool:
        return self.codec in TEXT_CODECS

    @property
    def ext(self) -> str:
        return TEXT_CODECS.get(self.codec, "")

    def label(self) -> str:
        kind = IMAGE_CODECS.get(self.codec, self.codec.upper())
        bits = [f"#{self.sub_index + 1}", kind]
        if self.language:
            bits.append(self.language)
        if self.title:
            bits.append(f"“{self.title}”")
        if self.default:
            bits.append("varsayılan")
        if self.forced:
            bits.append("zorunlu")
        return " · ".join(bits)


def _ffprobe_json(path: str) -> dict:
    exe = tool("ffprobe")
    if not exe:
        raise RuntimeError("ffprobe bulunamadı")
    r = subprocess.run([exe, "-v", "error", "-print_format", "json", "-show_streams", "-show_format", path],
                       capture_output=True, creationflags=_NO_WINDOW)
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode("utf-8", "replace").strip() or "ffprobe hata verdi")
    return json.loads(r.stdout.decode("utf-8", "replace"))


def subtitle_tracks(path: str) -> list[SubTrack]:
    data = _ffprobe_json(path)
    out = []
    n = 0
    for s in data.get("streams", []):
        if s.get("codec_type") != "subtitle":
            continue
        tags = {k.lower(): v for k, v in (s.get("tags") or {}).items()}
        disp = s.get("disposition") or {}
        out.append(SubTrack(int(s["index"]), n, s.get("codec_name", "?"), tags.get("language", ""),
                            tags.get("title", ""), bool(disp.get("default")), bool(disp.get("forced"))))
        n += 1
    return out


def attachment_count(path: str) -> int:
    return sum(1 for s in _ffprobe_json(path).get("streams", []) if s.get("codec_type") == "attachment")


def duration_s(path: str) -> float:
    try:
        return float(_ffprobe_json(path).get("format", {}).get("duration") or 0)
    except (RuntimeError, ValueError):
        return 0.0


def extract_cmd(video: str, track: SubTrack, out_path: str) -> list[str]:
    ff = tool("ffmpeg") or "ffmpeg"
    codec = "copy" if track.codec in ("ass", "ssa", "subrip", "srt", "webvtt") else (
        "srt" if out_path.lower().endswith(".srt") else "ass")
    return [ff, "-v", "error", "-y", "-i", video, "-map", f"0:{track.index}", "-c:s", codec, out_path]


def attachments_cmd(video: str) -> list[str]:
    """Çalışma klasöründe çalıştırılmalı (ekler kendi adlarıyla oraya yazılır)."""
    ff = tool("ffmpeg") or "ffmpeg"
    return [ff, "-v", "error", "-y", "-dump_attachment:t", "", "-i", video]


def ff_escape(p: str) -> str:
    """ffmpeg süzgeç argümanı için yol kaçışı (Windows sürücü harfi dahil)."""
    p = p.replace("\\", "/")
    for ch in (":", "'", ",", "[", "]"):
        p = p.replace(ch, "\\" + ch)
    return p


@dataclass
class BurnOptions:
    codec: str = "libx264"        # libx264 | libx265
    crf: int = 18
    preset: str = "medium"
    audio: str = "copy"           # copy | aac | none
    start_ms: int | None = None   # yalnız bir aralık (önizleme)
    end_ms: int | None = None
    tune: str = ""                # animation vb.


def burn_cmd(video: str, ass_path: str, out_path: str, o: BurnOptions, fontsdir: str | None = None) -> list[str]:
    """Aralık verilirse (önizleme) giriş o ana aranır; ass süzgecine özgün zaman verilir
    (setpts=PTS+başlangıç), çıktı yine 0'dan başlar."""
    ff = tool("ffmpeg") or "ffmpeg"
    flt = f"ass='{ff_escape(ass_path)}'"
    if fontsdir:
        flt += f":fontsdir='{ff_escape(fontsdir)}'"
    s = (o.start_ms or 0) / 1000.0
    cmd = [ff, "-hide_banner", "-v", "error", "-y", "-nostats", "-progress", "pipe:1"]
    if s > 0:
        cmd += ["-ss", f"{s:.3f}"]
        flt = f"setpts=PTS+{s:.3f}/TB,{flt},setpts=PTS-STARTPTS"
    cmd += ["-i", video]
    if o.end_ms is not None:
        cmd += ["-t", f"{max(0.04, o.end_ms / 1000.0 - s):.3f}"]
    cmd += ["-map", "0:v:0"]
    if o.audio != "none":
        cmd += ["-map", "0:a:0?"]
    cmd += ["-vf", flt, "-c:v", o.codec, "-crf", str(o.crf), "-preset", o.preset, "-pix_fmt", "yuv420p"]
    if o.tune:
        cmd += ["-tune", o.tune]
    if o.audio == "copy":
        cmd += ["-c:a", "copy"]
    elif o.audio == "aac":
        cmd += ["-c:a", "aac", "-b:a", "192k"]
    if out_path.lower().endswith(".mp4"):
        cmd += ["-movflags", "+faststart"]
    cmd.append(out_path)
    return cmd


@dataclass
class MuxOptions:
    language: str = "tur"
    title: str = "Türkçe"
    default: bool = True
    keep_subs: bool = False       # videodaki mevcut altyazı izleri kalsın mı
    keep_attachments: bool = True


def mux_cmd(video: str, ass_path: str, out_path: str, o: MuxOptions, fonts: list[str] | None = None,
            existing_subs: int = 0, existing_atts: int = 0) -> list[str]:
    """Video + ses (+ mevcut altyazı/ekler) + yeni ASS + yazı tipi ekleri → MKV (yeniden kodlama yok)."""
    ff = tool("ffmpeg") or "ffmpeg"
    cmd = [ff, "-hide_banner", "-v", "error", "-y", "-nostats", "-progress", "pipe:1", "-i", video, "-i", ass_path,
           "-map", "0:v", "-map", "0:a?"]
    n_sub = n_att = 0
    if o.keep_subs and existing_subs:
        cmd += ["-map", "0:s"]
        n_sub = existing_subs
    if o.keep_attachments and existing_atts:
        cmd += ["-map", "0:t"]
        n_att = existing_atts
    cmd += ["-map", "1:0", "-c", "copy"]
    cmd += [f"-metadata:s:s:{n_sub}", f"language={o.language}"]
    if o.title:
        cmd += [f"-metadata:s:s:{n_sub}", f"title={o.title}"]
    if o.default:
        for k in range(n_sub):
            cmd += [f"-disposition:s:{k}", "0"]
        cmd += [f"-disposition:s:{n_sub}", "default"]
    for f in fonts or []:
        cmd += ["-attach", f]
    for k, f in enumerate(fonts or []):
        mime = FONT_MIME.get(os.path.splitext(f)[1].lower(), "application/octet-stream")
        cmd += [f"-metadata:s:t:{n_att + k}", f"mimetype={mime}"]
    cmd.append(out_path)
    return cmd


def mux_cmd_full(video: str, ass_path: str, out_path: str, o: MuxOptions, fonts: list[str] | None = None) -> list[str]:
    """Videodaki mevcut altyazı ve ek sayılarını ffprobe ile okuyup mux_cmd'u kurar."""
    data = _ffprobe_json(video)
    subs = sum(1 for st in data.get("streams", []) if st.get("codec_type") == "subtitle")
    atts = sum(1 for st in data.get("streams", []) if st.get("codec_type") == "attachment")
    return mux_cmd(video, ass_path, out_path, o, fonts, subs, atts)


class Job:
    """ffmpeg'i arka planda çalıştırır. progress: 0..1 (-progress pipe:1'den), done: (kod, hata metni)."""

    def __init__(self, cmd: list[str], duration_s: float = 0.0, cwd: str | None = None):
        self.cmd = cmd
        self.duration = duration_s
        self.cwd = cwd
        self.progress = 0.0
        self.returncode = None
        self.error = ""
        self.proc = None
        self._th = None

    def start(self):
        def work():
            try:
                self.proc = subprocess.Popen(self.cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                             cwd=self.cwd, creationflags=_NO_WINDOW)
                err_chunks = []

                def read_err():
                    err_chunks.append(self.proc.stderr.read())
                te = threading.Thread(target=read_err, daemon=True)
                te.start()
                for raw in self.proc.stdout:
                    line = raw.decode("utf-8", "replace").strip()
                    if line.startswith("out_time_us=") or line.startswith("out_time_ms="):
                        try:
                            us = int(line.split("=", 1)[1])
                        except ValueError:
                            continue
                        if self.duration > 0:
                            self.progress = max(0.0, min(1.0, us / 1e6 / self.duration))
                    elif line == "progress=end":
                        self.progress = 1.0
                self.proc.wait()
                te.join(timeout=5)
                self.proc.stdout.close()
                self.proc.stderr.close()
                self.returncode = self.proc.returncode
                self.error = b"".join(c for c in err_chunks if c).decode("utf-8", "replace").strip()
            except Exception as e:  # noqa: BLE001
                self.returncode = -1
                self.error = str(e)

        self._th = threading.Thread(target=work, name="osisub-ffmpeg", daemon=True)
        self._th.start()

    @property
    def running(self) -> bool:
        return self._th is not None and self._th.is_alive()

    def cancel(self):
        if self.proc is not None and self.proc.poll() is None:
            self.proc.kill()

    def wait(self, timeout: float | None = None) -> bool:
        if self._th is not None:
            self._th.join(timeout)
        return not self.running


def dump_attachments(video: str, dest: str) -> list[str]:
    """MKV eklerini (yazı tipleri vb.) dest klasörüne kendi adlarıyla yazar; yazılan dosyaları döndürür.
    ffmpeg çıktı dosyası verilmediği için 'output file' hatasıyla biter — bu beklenen durumdur."""
    os.makedirs(dest, exist_ok=True)
    before = set(os.listdir(dest))
    rc, err = run_sync(attachments_cmd(video), cwd=dest)
    new = sorted(set(os.listdir(dest)) - before)
    if rc != 0 and not new and "output file" not in err.lower():
        raise RuntimeError(err or "ekler çıkarılamadı")
    return [os.path.join(dest, n) for n in new]


def run_sync(cmd: list[str], cwd: str | None = None) -> tuple[int, str]:
    r = subprocess.run(cmd, capture_output=True, cwd=cwd, creationflags=_NO_WINDOW)
    return r.returncode, r.stderr.decode("utf-8", "replace").strip()


__all__ = ["SubTrack", "subtitle_tracks", "extract_cmd", "attachments_cmd", "dump_attachments", "BurnOptions", "burn_cmd", "MuxOptions",
           "mux_cmd", "mux_cmd_full", "Job", "run_sync", "duration_s", "attachment_count", "ff_escape"]
