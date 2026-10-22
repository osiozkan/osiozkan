#!/usr/bin/env python3
"""Write daily/subforge-code-YYYY-MM-DD.zip containing one .txt with ALL SubForge source written so far.
Run at the end of every nightly session:  python3 subforge/scripts/make_daily_dump.py"""
import datetime, os, subprocess, sys, zipfile

root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
files = subprocess.check_output(["git", "ls-files", "-co", "--exclude-standard"], cwd=root, text=True).splitlines()
TEXT = {".rs", ".toml", ".lua", ".js", ".html", ".md", ".sh", ".py", ".cjs", ".yml", ".yaml", ".ass", ".srt", ".txt", ".lock", ".gitignore"}
today = datetime.date.today().isoformat()
out_dir = os.path.join(root, "daily")
os.makedirs(out_dir, exist_ok=True)
name = f"subforge-code-{today}"
parts, n = [], 0
for f in sorted(files):
    if f.startswith("daily/") or f.endswith("Cargo.lock"):
        continue
    ext = os.path.splitext(f)[1].lower() or os.path.basename(f)
    if ext not in TEXT:
        continue
    try:
        body = open(os.path.join(root, f), encoding="utf-8").read()
    except (UnicodeDecodeError, OSError):
        continue
    parts.append(f"\n{'=' * 78}\nFILE: {f}\n{'=' * 78}\n{body}")
    n += 1
head = f"SubForge - all source code as of {today} ({n} files)\nRepo: osiozkan/osiozkan  (folder subforge/)\n"
txt = head + "".join(parts) + "\n"
zp = os.path.join(out_dir, name + ".zip")
with zipfile.ZipFile(zp, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr(name + ".txt", txt)
print(zp, n, "files", len(txt) // 1024, "KB txt")
