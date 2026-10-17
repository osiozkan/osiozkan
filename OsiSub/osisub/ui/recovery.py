"""Otomatik yedek ve kurtarma.

Belge değiştiyse her `autosave_seconds` saniyede bir (varsayılan 60) kopyası
<kullanıcı klasörü>/autosave/ altına yazılır (atomik; yanına özgün yol ve zamanı tutan .json).
Belge kaydedilince ya da program düzgün kapanınca o yedek silinir. Program çökerse ya da
elektrik giderse yedek kalır; bir sonraki açılışta "Kurtarma" penceresi önerir.

Kurtarılan yedek, ÖZGÜN dosyanın yoluyla ama "değişmiş" olarak açılır: kullanıcı bilerek
Kaydet'e basmadıkça özgün dosyanın üstüne yazılmaz.
"""

from __future__ import annotations

import datetime
import hashlib
import json
import logging
import os

from PySide6.QtCore import QObject, QTimer
from PySide6.QtWidgets import (QDialog, QDialogButtonBox, QLabel, QListWidget, QListWidgetItem,
                               QPushButton, QVBoxLayout)

log = logging.getLogger("osisub.recovery")


def autosave_dir() -> str:
    from ..config import user_dir
    d = os.path.join(user_dir(), "autosave")
    os.makedirs(d, exist_ok=True)
    return d


def _key(path: str | None, token: str) -> str:
    src = os.path.abspath(path) if path else f"adsiz-{token}"
    h = hashlib.sha1(src.encode("utf-8")).hexdigest()[:10]
    stem = os.path.splitext(os.path.basename(path))[0] if path else "adsiz"
    return f"{stem}-{h}"


def write_backup(doc, token: str) -> str:
    """Belgenin yedeğini yaz (atomik). Döndürür: yedek .ass yolu."""
    base = os.path.join(autosave_dir(), _key(doc.path, token))
    data = doc.to_bytes()
    tmp = base + ".ass.tmp"
    with open(tmp, "wb") as fh:
        fh.write(data)
    os.replace(tmp, base + ".ass")
    meta = {"original": os.path.abspath(doc.path) if doc.path else None,
            "time": datetime.datetime.now().isoformat(timespec="seconds"), "bytes": len(data)}
    with open(base + ".json.tmp", "w", encoding="utf-8") as fh:
        json.dump(meta, fh, ensure_ascii=False)
    os.replace(base + ".json.tmp", base + ".json")
    return base + ".ass"


def remove_backup(path: str | None, token: str):
    base = os.path.join(autosave_dir(), _key(path, token))
    for ext in (".ass", ".json"):
        try:
            os.remove(base + ext)
        except OSError:
            pass


def find_recoverable() -> list[dict]:
    """Özgün dosyadan YENİ olan yedekler (ya da adsız belgelerin yedekleri)."""
    out = []
    d = autosave_dir()
    for name in sorted(os.listdir(d)):
        if not name.endswith(".json"):
            continue
        meta_p = os.path.join(d, name)
        ass_p = meta_p[:-5] + ".ass"
        if not os.path.exists(ass_p):
            continue
        try:
            with open(meta_p, encoding="utf-8") as fh:
                meta = json.load(fh)
        except (OSError, ValueError):
            continue
        orig = meta.get("original")
        mt = os.path.getmtime(ass_p)
        if orig and os.path.exists(orig) and os.path.getmtime(orig) >= mt:
            discard({"backup": ass_p, "meta": meta_p})   # özgün dosya yedekten sonra kaydedilmiş: eski yedek
            continue
        out.append({"backup": ass_p, "meta": meta_p, "original": orig, "time": meta.get("time", ""),
                    "bytes": meta.get("bytes", 0)})
    return out


def discard(item: dict):
    for p in (item["backup"], item["meta"]):
        try:
            os.remove(p)
        except OSError:
            pass


def restore_into(ctx, item: dict):
    """Yedeği aç: belge yolu ÖZGÜN dosya, durum 'değişmiş' (bilerek kaydedilene kadar üstüne yazılmaz)."""
    from ..core.assfile import AssDocument
    doc = AssDocument.load(item["backup"])
    doc.path = item["original"]
    ctx.doc = doc
    ctx.undo.reset(doc)
    ctx.undo.saved_index = -1
    ctx._reset_view()
    ctx.undo_changed.emit()
    discard(item)


class Autosave(QObject):
    def __init__(self, ctx, seconds: int = 60):
        super().__init__()
        self.ctx = ctx
        self.token = hashlib.sha1(os.urandom(8)).hexdigest()[:6]     # bu oturumun adsız belgesi
        self._last_state = None
        self._last_path = None
        self.written: set[str] = set()      # bu oturumun yazdığı yedekler (yalnız onlar silinir)
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.tick)
        self.set_interval(seconds)
        ctx.path_changed.connect(self._path_changed)
        ctx.undo_changed.connect(self._undo_changed)

    def set_interval(self, seconds: int):
        if seconds and seconds > 0:
            self.timer.start(int(seconds * 1000))
        else:
            self.timer.stop()

    def _state(self):
        u = self.ctx.undo
        return id(u.states[u.index]) if u.states else None

    def tick(self) -> str | None:
        u = self.ctx.undo
        if not u.modified:
            return None
        st = self._state()
        if st == self._last_state and self._last_path == self.ctx.doc.path:
            return None
        try:
            p = write_backup(self.ctx.doc, self.token)
        except OSError as e:
            log.warning("Otomatik yedek yazılamadı: %s", e)
            return None
        self._last_state = st
        self._last_path = self.ctx.doc.path
        self.written.add(p)
        log.info("Otomatik yedek: %s", p)
        return p

    def _undo_changed(self):
        if not self.ctx.undo.modified:
            self.clear()

    def _path_changed(self):
        # farklı kaydet / yeni belge: bu oturumun eski yedeği artık gereksiz
        if self._last_path != self.ctx.doc.path and self._last_state is not None:
            self.clear()

    def clear(self):
        """Yalnız BU oturumun yazdığı yedekleri sil (önceki çökmeden kalanlara dokunma)."""
        for p in list(self.written):
            for q in (p, p[:-4] + ".json"):
                try:
                    os.remove(q)
                except OSError:
                    pass
            self.written.discard(p)
        self._last_state = None
        self._last_path = None

    def emergency(self) -> str | None:
        """Çökme anında: değişiklik varsa hemen yaz."""
        if not self.ctx.undo.modified:
            return None
        try:
            p = write_backup(self.ctx.doc, self.token)
            self.written.discard(p)          # çökme yedeği düzgün kapanışta bile silinmesin
            return p
        except Exception:  # noqa: BLE001
            return None


class RecoveryDialog(QDialog):
    def __init__(self, items, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Kurtarma — kaydedilmemiş çalışma bulundu")
        self.items = list(items)
        self.choice = None
        self.list = QListWidget()
        for it in self.items:
            name = it["original"] or "(adsız belge)"
            li = QListWidgetItem(f"{name}\n    yedek: {it['time']} · {it['bytes'] // 1024} KB")
            self.list.addItem(li)
        if self.items:
            self.list.setCurrentRow(0)
        info = QLabel("Program beklenmedik biçimde kapanmış. Aşağıdaki yedekler özgün dosyalardan daha yeni.\n"
                      "Açtığın yedek özgün dosyanın yerine KENDİLİĞİNDEN yazılmaz; kontrol edip Kaydet'e bas.")
        info.setWordWrap(True)
        b_open = QPushButton("Yedeği aç")
        b_del = QPushButton("Yedeği sil")
        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.button(QDialogButtonBox.Close).setText("Daha sonra")
        bb.addButton(b_open, QDialogButtonBox.AcceptRole)
        bb.addButton(b_del, QDialogButtonBox.DestructiveRole)
        b_open.clicked.connect(self._open)
        b_del.clicked.connect(self._delete)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addWidget(info)
        lay.addWidget(self.list, 1)
        lay.addWidget(bb)
        self.resize(640, 320)

    def _current(self):
        r = self.list.currentRow()
        return self.items[r] if 0 <= r < len(self.items) else None

    def _open(self):
        it = self._current()
        if it is not None:
            self.choice = it
            self.accept()

    def _delete(self):
        it = self._current()
        if it is None:
            return
        discard(it)
        r = self.list.currentRow()
        self.items.pop(r)
        self.list.takeItem(r)
        if not self.items:
            self.reject()
