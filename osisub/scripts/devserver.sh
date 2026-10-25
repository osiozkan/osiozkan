#!/bin/sh
# restart the dev server in the background (pid in /tmp/osisub.pid)
[ -f /tmp/osisub.pid ] && kill "$(cat /tmp/osisub.pid)" 2>/dev/null
cd "$(dirname "$0")/.." || exit 1
setsid nohup node server.js >/tmp/server.log 2>&1 < /dev/null &
echo $! > /tmp/osisub.pid
sleep 1.5
