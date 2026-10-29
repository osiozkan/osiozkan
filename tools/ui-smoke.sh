#!/bin/bash
# Etkileşimli arayüz duman testi: Xvfb + xdotool ile tıklama/tuş senaryosu, sonunda ekran görüntüsü.
# Kullanım: tools/ui-smoke.sh <osisub-gui ikilisi> <altyazı.ass> <çıktı.png> [ek bayraklar...]
# Gereken: Xvfb, xdotool, libxkbcommon-x11-0.  Çökme (panic) olursa 1 ile çıkar.
set -u
BIN=$1; SUB=$2; OUT=$3; shift 3
D=:77
Xvfb $D -screen 0 1400x900x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
export DISPLAY=$D LIBGL_ALWAYS_SOFTWARE=1
LOG=$(mktemp)
"$BIN" "$SUB" --shot-delay 22 --screenshot "$OUT" "$@" >"$LOG" 2>&1 &
APID=$!
for i in $(seq 1 50); do xdotool search --onlyvisible --name osisub >/dev/null 2>&1 && break; sleep 0.3; done
WID=$(xdotool search --onlyvisible --name osisub | head -1)
xdotool windowfocus "$WID" >/dev/null 2>&1
sleep 5
x() { xdotool "$@"; sleep 0.4; }
click() { xdotool mousemove "$1" "$2"; sleep 0.4; xdotool click 1; sleep 0.4; }
# satır 3'e tıkla, gezin, sil, geri al
click 200 158
x key Down
x key Down
x key Delete
x key ctrl+z
# bul penceresi
x key ctrl+f
x type --delay 30 "Evet"
x key Return
x key Escape
# zamana git
x key ctrl+g
x type --delay 30 "7"
x key Return
# metin düzenle: düzenleme kutusuna tıkla ve yaz
click 400 690
x type --delay 30 " test"
x key ctrl+z
# kopyala / yapıştır / çoğalt
click 200 130
x key ctrl+c
x key ctrl+v
x key ctrl+d
x key ctrl+Return
x key alt+Down
if [ -n "${MEDIA_TEST:-}" ]; then
  # dalga formu: tıkla, sürükle (tutamaç), yakınlaştır/kaydır, oynat/durdur
  click 760 150
  xdotool mousemove 555 150; sleep 0.3; xdotool mousedown 1; sleep 0.2; xdotool mousemove 575 150; sleep 0.2; xdotool mousemove 600 150; sleep 0.2; xdotool mouseup 1; sleep 0.4
  xdotool keydown ctrl; xdotool click 4; xdotool click 4; xdotool click 5; xdotool keyup ctrl; sleep 0.4
  xdotool mousemove 900 150; xdotool click 5; xdotool click 5; sleep 0.3
  x key F5
  sleep 1.5
  x key F5
  x key comma
  x key period
  x key ctrl+z
fi
wait $APID
CODE=$?
kill $XPID 2>/dev/null
if grep -q "panicked" "$LOG"; then echo "ÇÖKME:"; grep -A8 "panicked" "$LOG"; exit 1; fi
echo "çıkış kodu: $CODE"
tail -3 "$LOG"
exit $CODE
