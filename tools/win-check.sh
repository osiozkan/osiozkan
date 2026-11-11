#!/bin/bash
# Windows derlemesini wine ile doğrular: CLI (bilgi, Lua makrosu, kara-templater — Linux çıktısıyla
# bayt bayt karşılaştırma) ve GUI ekran görüntüsü.
# Kurulum: apt-get install gcc-mingw-w64-x86-64 wine64 xvfb; rustup target add x86_64-pc-windows-gnu
set -eu
cd "$(dirname "$0")/.."
T=$(mktemp -d)
export WINEDEBUG=-all WINEPREFIX=$T/wine
cargo build -q --release --target x86_64-pc-windows-gnu -p osisub-cli -p osisub-gui
cargo build -q -p osisub-cli
W=target/x86_64-pc-windows-gnu/release/osisub.exe
fail=0
cmp_run() { # $1 ad, sonrası argümanlar (çıktı dosyası sonda eklenir)
  local name=$1; shift
  wine $W "$@" "$T/w_$name.ass" >/dev/null 2>&1 || true
  ./target/debug/osisub "$@" "$T/l_$name.ass" >/dev/null 2>&1 || true
  if [ -s "$T/w_$name.ass" ] && cmp -s "$T/w_$name.ass" "$T/l_$name.ass"; then echo "TAMAM: $name (Windows = Linux)"; else echo "HATA: $name"; fail=1; fi
}
out=$(wine $W info examples/demo.ass 2>/dev/null || true)
wineserver -w  # ilk çalıştırmada wine önyüklemesi bitmeden GUI açılırsa takılır
echo "$out" | grep -q "diyalog: 7" && echo "TAMAM: info" || { echo "HATA: info"; fail=1; }
cmp_run lua run examples/demo.ass crates/osisub-lua/tests/fixtures/sample.lua 0
cmp_run templater run crates/osisub-lua/tests/fixtures/templater.ass builtin:kara-templater.lua "Apply karaoke template"
cmp_run merge merge examples/demo.ass crates/osisub-lua/tests/fixtures/templater.ass --sort -o
wineserver -w  # ekransız başlamış wineserver kapanmadan GUI pencere açamaz
LIBGL_ALWAYS_SOFTWARE=1 timeout 120 xvfb-run -a -s "-screen 0 1400x900x24" wine target/x86_64-pc-windows-gnu/release/osisub-gui.exe examples/demo.ass --screenshot "Z:$T/gui.png" >/dev/null 2>&1 || true
[ -s "$T/gui.png" ] && echo "TAMAM: GUI açıldı ($T/gui.png)" || { echo "HATA: GUI"; fail=1; }
exit $fail
