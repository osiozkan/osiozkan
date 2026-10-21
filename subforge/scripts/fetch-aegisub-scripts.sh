#!/bin/sh
# Downloads Aegisub's stock Automation scripts (BSD-licensed) into a directory so the
# compatibility test (tests/aegisub_stock.rs) can run them unmodified.
# usage: scripts/fetch-aegisub-scripts.sh DIR   then: AEGISUB_SCRIPTS=DIR cargo test --test aegisub_stock
set -e
DIR="${1:?usage: $0 DIR}"
mkdir -p "$DIR"
BASE=https://raw.githubusercontent.com/Aegisub/Aegisub/master/automation
curl -fsSL -o "$DIR/kara-templater.lua" "$BASE/autoload/kara-templater.lua"
for f in cleantags-autoload.lua karaoke-auto-leadin.lua macro-1-edgeblur.lua strip-tags.lua; do
  curl -fsSL -o "$DIR/$f" "$BASE/autoload/$f"
done
for f in karaskel.lua karaskel-auto4.lua utils.lua unicode.lua cleantags.lua clipboard.lua; do
  curl -fsSL -o "$DIR/$f" "$BASE/include/$f"
done
echo "fetched into $DIR"
