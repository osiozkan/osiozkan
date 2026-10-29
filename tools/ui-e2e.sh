#!/bin/bash
# Uçtan uca GUI testi: düzenle → kaydet → dosyayı doğrula; geri al → kaydet → doğrula.
# Kullanım: tools/ui-e2e.sh <osisub-gui ikilisi> <örnek.ass>
set -u
BIN=$1; SRC=$2
D=:83
TMP=$(mktemp -d); cp "$SRC" "$TMP/t.ass"
Xvfb $D -screen 0 1400x900x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
export DISPLAY=$D LIBGL_ALWAYS_SOFTWARE=1
"$BIN" "$TMP/t.ass" --lua "$(dirname "$0")/../crates/osisub-lua/tests/fixtures/sample.lua" --shot-delay 60 --screenshot "$TMP/s.png" >"$TMP/log" 2>&1 &
APID=$!
for i in $(seq 1 60); do xdotool search --onlyvisible --name osisub >/dev/null 2>&1 && break; sleep 0.3; done
WID=$(xdotool search --onlyvisible --name osisub | head -1)
xdotool windowfocus "$WID" >/dev/null 2>&1
sleep 5
click() { xdotool mousemove "$1" "$2"; sleep 0.4; xdotool click 1; sleep 0.4; }
k() { xdotool key "$@"; sleep 0.5; }
fail=0
check() { if grep -q "$2" "$TMP/t.ass"; then echo "TAMAM: $1"; else echo "HATA: $1"; fail=1; fi; }
checkno() { if grep -q "$2" "$TMP/t.ass"; then echo "HATA: $1"; fail=1; else echo "TAMAM: $1"; fi; }

click 200 100                  # satır 1
click 400 690                  # düzenleme kutusu
k ctrl+End
xdotool type --delay 40 "XYZQ"; sleep 0.6
k ctrl+s; sleep 1
check "düzenleme kaydedildi" "XYZQ"
# kutudan çık, geri al
click 200 129                  # satır 2 (odağı kutudan al)
k ctrl+z; k ctrl+s; sleep 1
checkno "geri alma kaydedildi" "XYZQ"
k ctrl+y; k ctrl+s; sleep 1
check "yineleme kaydedildi" "XYZQ"
# satır sil / geri al
click 200 158; k Delete; k ctrl+s; sleep 1
checkno "satır silindi" "Bu satır çok kısa"
k ctrl+z; k ctrl+s; sleep 1
check "silme geri alındı" "Bu satır çok kısa"
# Lua makrosu: Otomasyon menüsü → ilk makro (Kaydir: +1 sn ve '!' ekler), tek geri-al adımı
click 200 100
click 330 12; sleep 0.5
click 345 40; sleep 1.5
k ctrl+s; sleep 1
check "Lua makrosu uygulandı (+1 sn, '!')" "Dialogue: 0,0:00:02.00,0:00:04.20,Default,Sakura,0,0,0,,.*!"
k ctrl+z; k ctrl+s; sleep 1
checkno "makro tek adımda geri alındı" "0:00:02.00,0:00:04.20,Default,Sakura"
# .bak yedeği
[ -f "$TMP/t.ass.bak" ] && echo "TAMAM: .bak yedeği var" || { echo "HATA: .bak yok"; fail=1; }

kill $APID 2>/dev/null; kill $XPID 2>/dev/null
grep -q panicked "$TMP/log" && { echo "ÇÖKME"; grep -A6 panicked "$TMP/log"; fail=1; }
rm -rf "$TMP"
exit $fail
