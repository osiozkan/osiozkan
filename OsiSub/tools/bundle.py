"""Proje ağacının TAMAMINI tek bir .txt dosyasına paketler (ve geri açar).

  python tools/bundle.py                 -> dist/OsiSub_tum_kod.txt
  python tools/bundle.py --unpack X.txt  -> X.txt'deki ağacı ./OsiSub_geri/ altına kurar

Her dosya şu biçimde yazılır:
  ===== DOSYA: osisub/core/assfile.py [bom,crlf] =====
  ...içerik (LF)...
  ===== SON: osisub/core/assfile.py =====
BOM ve CRLF bayrakları geri açarken aynen geri konur; UTF-8 olmayan dosyalar
base64 olarak saklanır.
"""

from __future__ import annotations

import base64
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INCLUDE_EXT = {".py", ".lua", ".moon", ".md", ".txt", ".bat", ".cmd", ".json", ".ass", ".toml",
               ".cfg", ".ini", ".gitignore", ".spec", ".iss"}
SKIP_DIRS = {"__pycache__", ".git", "dist", ".pytest_cache", ".mypy_cache", "build"}

UNPACK_SNIPPET = r'''
# ---- GERİ AÇMA (bu bloğu unbundle.py olarak kaydet, sonra: python unbundle.py OsiSub_tum_kod.txt)
import base64, os, re, sys
src = open(sys.argv[1], encoding="utf-8").read().split("\n")
out_root = sys.argv[2] if len(sys.argv) > 2 else "OsiSub_geri"
i = 0
head = re.compile(r"^===== DOSYA(\(base64\))?: (.+?)(?: \[([a-z,]*)\])? =====$")
while i < len(src):
    m = head.match(src[i])
    if not m:
        i += 1
        continue
    b64, path, flags = m.group(1), m.group(2), (m.group(3) or "").split(",")
    end = "===== SON: " + path + " ====="
    j = src.index(end, i + 1)
    body = "\n".join(src[i + 1:j])
    dest = os.path.join(out_root, *path.split("/"))
    os.makedirs(os.path.dirname(dest) or ".", exist_ok=True)
    if b64:
        data = base64.b64decode(body)
    else:
        if "nonl" not in flags:
            body += "\n"
        if "crlf" in flags:
            body = body.replace("\n", "\r\n")
        data = body.encode("utf-8")
        if "bom" in flags:
            data = b"\xef\xbb\xbf" + data
    open(dest, "wb").write(data)
    print("yazıldı:", dest)
    i = j + 1
# ---- GERİ AÇMA SONU
'''


def collect() -> list[str]:
    files = []
    for dp, dns, fns in os.walk(ROOT):
        dns[:] = sorted(d for d in dns if d not in SKIP_DIRS)
        for fn in sorted(fns):
            ext = os.path.splitext(fn)[1].lower() if not fn.startswith(".") else fn
            if ext in INCLUDE_EXT or fn in ("LICENSE", "requirements.txt"):
                files.append(os.path.relpath(os.path.join(dp, fn), ROOT).replace(os.sep, "/"))
    return files


def pack(out_path: str) -> tuple[int, int]:
    from datetime import datetime
    sys.path.insert(0, ROOT)
    try:
        from osisub import __version__
    except Exception:  # noqa: BLE001
        __version__ = "?"
    files = collect()
    parts = [
        f"OsiSub {__version__} — TÜM KAYNAK KOD",
        f"Paketlenme: {datetime.now():%Y-%m-%d %H:%M}",
        f"Dosya sayısı: {len(files)}",
        "",
        "Bu metin dosyası projenin eksiksiz kopyasıdır. Ağacı geri kurmak için aşağıdaki",
        "GERİ AÇMA bloğunu 'unbundle.py' adıyla kaydedip çalıştırın:",
        "    python unbundle.py OsiSub_tum_kod.txt",
        "(ya da proje içindeki tools/bundle.py --unpack OsiSub_tum_kod.txt)",
        UNPACK_SNIPPET,
        "DOSYA LİSTESİ:",
    ]
    parts += ["  " + f for f in files]
    parts.append("")
    total_lines = 0
    for rel in files:
        with open(os.path.join(ROOT, rel), "rb") as f:
            data = f.read()
        flags = []
        try:
            if data.startswith(b"\xef\xbb\xbf"):
                flags.append("bom")
                data = data[3:]
            text = data.decode("utf-8")
            if "\r\n" in text:
                flags.append("crlf")
                text = text.replace("\r\n", "\n")
            if text.endswith("\n"):
                text = text[:-1]
            else:
                flags.append("nonl")
            head = f"===== DOSYA: {rel}" + (f" [{','.join(flags)}]" if flags else "") + " ====="
            parts += [head, text, f"===== SON: {rel} ====="]
            total_lines += text.count("\n") + 1
        except UnicodeDecodeError:
            parts += [f"===== DOSYA(base64): {rel} =====", base64.b64encode(data).decode("ascii"),
                      f"===== SON: {rel} ====="]
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(parts) + "\n")
    return len(files), total_lines


def unpack(txt_path: str, out_root: str):
    ns: dict = {}
    sys.argv = ["unbundle", txt_path, out_root]
    exec(compile(UNPACK_SNIPPET, "unbundle", "exec"), ns)  # noqa: S102


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "--unpack":
        unpack(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "OsiSub_geri")
    else:
        out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "dist", "OsiSub_tum_kod.txt")
        n, lines = pack(out)
        print(f"{n} dosya, {lines} satır -> {out}")
