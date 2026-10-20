"""Yuzu'nun TÜM kaynak ağacını tek bir .txt dosyasına paketler (ve geri açar).

  python tools/bundle.py                 -> dist/Yuzu_<sürüm>_tum_kod.txt
  python tools/bundle.py cikti.txt       -> cikti.txt
  python tools/bundle.py --unpack X.txt  -> X.txt'deki ağacı ./Yuzu_geri/ altına kurar
  python tools/bundle.py --unpack X.txt hedef/

Git deposu içindeyken yalnız git'in izlediği dosyalar alınır; değilse (paketten geri açılmış
ağaçta) target/, dist/, .git/ dışındaki her şey alınır.

Her dosya şu biçimde yazılır:
  ===== DOSYA: crates/asscore/src/ass.rs [bom,crlf] =====
  ...içerik (LF)...
  ===== SON: crates/asscore/src/ass.rs =====
BOM, CRLF ve "sonda satır sonu yok" bayrakları geri açarken aynen geri konur; UTF-8 olmayan
dosyalar base64 olarak saklanır. Geri açılan ağaç bayt bayt özgünüyle aynıdır.
"""

from __future__ import annotations

import base64
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP_DIRS = {".git", "target", "dist", "__pycache__", ".claude"}

UNPACK_SNIPPET = r'''
# ---- GERİ AÇMA (bu bloğu unbundle.py olarak kaydet, sonra: python unbundle.py Yuzu_tum_kod.txt)
import base64, os, re, sys
src = open(sys.argv[1], encoding="utf-8").read().split("\n")
out_root = sys.argv[2] if len(sys.argv) > 2 else "Yuzu_geri"
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
    if "exec" in flags:
        os.chmod(dest, 0o755)
    print("yazıldı:", dest)
    i = j + 1
# ---- GERİ AÇMA SONU
'''


def version() -> str:
    try:
        with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as f:
            m = re.search(r'^\s*version\s*=\s*"([^"]+)"', f.read(), re.M)
        return m.group(1) if m else "?"
    except OSError:
        return "?"


def collect() -> list[str]:
    try:
        out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
        files = sorted(p for p in out.decode("utf-8").split("\0") if p and os.path.isfile(os.path.join(ROOT, p)))
        if files:
            return files
    except (OSError, subprocess.CalledProcessError):
        pass
    files = []
    for dp, dns, fns in os.walk(ROOT):
        dns[:] = sorted(d for d in dns if d not in SKIP_DIRS)
        for fn in sorted(fns):
            files.append(os.path.relpath(os.path.join(dp, fn), ROOT).replace(os.sep, "/"))
    return files


def pack(out_path: str) -> tuple[int, int]:
    from datetime import datetime
    files = collect()
    name = os.path.basename(out_path)
    parts = [
        f"Yuzu {version()} — TÜM KAYNAK KOD",
        f"Paketlenme: {datetime.now():%Y-%m-%d %H:%M}",
        f"Dosya sayısı: {len(files)}",
        "",
        "Bu metin dosyası projenin eksiksiz kopyasıdır. Ağacı geri kurmak için aşağıdaki",
        "GERİ AÇMA bloğunu 'unbundle.py' adıyla kaydedip çalıştırın:",
        f"    python unbundle.py {name}",
        f"(ya da proje içindeki: python tools/bundle.py --unpack {name})",
        "Sonra: cd Yuzu_geri && cargo build --release -p yuzu-gui -p yuzu-cli",
        UNPACK_SNIPPET,
        "DOSYA LİSTESİ:",
    ]
    parts += ["  " + f for f in files]
    parts.append("")
    total_lines = 0
    for rel in files:
        full = os.path.join(ROOT, rel)
        with open(full, "rb") as f:
            data = f.read()
        flags = []
        try:
            if data.startswith(b"\xef\xbb\xbf"):
                flags.append("bom")
                data = data[3:]
            text = data.decode("utf-8")
            if "\r" in text.replace("\r\n", ""):
                raise UnicodeDecodeError("utf-8", b"", 0, 1, "yalnız CR içeren satır sonu")
            if "\r\n" in text:
                if text.count("\r\n") != text.count("\n"):
                    raise UnicodeDecodeError("utf-8", b"", 0, 1, "karışık satır sonu")
                flags.append("crlf")
                text = text.replace("\r\n", "\n")
            if text.endswith("\n"):
                text = text[:-1]
            else:
                flags.append("nonl")
            if os.access(full, os.X_OK) and os.name != "nt":
                flags.append("exec")
            if any(line.startswith("===== SON: ") for line in text.split("\n")):
                raise UnicodeDecodeError("utf-8", b"", 0, 1, "ayraç satırı içeriyor")
            head = f"===== DOSYA: {rel}" + (f" [{','.join(flags)}]" if flags else "") + " ====="
            parts += [head, text, f"===== SON: {rel} ====="]
            total_lines += text.count("\n") + 1
        except UnicodeDecodeError:
            with open(full, "rb") as f:
                raw = f.read()
            parts += [f"===== DOSYA(base64): {rel} =====", base64.b64encode(raw).decode("ascii"),
                      f"===== SON: {rel} ====="]
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(parts) + "\n")
    return len(files), total_lines


def unpack(txt_path: str, out_root: str):
    ns: dict = {}
    sys.argv = ["unbundle", txt_path, out_root]
    exec(compile(UNPACK_SNIPPET, "unbundle", "exec"), ns)  # noqa: S102


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "--unpack":
        unpack(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "Yuzu_geri")
    else:
        out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "dist", f"Yuzu_{version()}_tum_kod.txt")
        n, lines = pack(out)
        print(f"{n} dosya, {lines} satır -> {out}")
