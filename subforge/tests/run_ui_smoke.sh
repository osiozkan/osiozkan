#!/bin/sh
# Starts a fresh `subforge serve` on a private port and runs the Playwright UI smoke test against it.
# usage: PW_PATH=/path/to/node_modules/playwright tests/run_ui_smoke.sh [path-to-subforge-binary]
set -e
cd "$(dirname "$0")/.."
BIN="${1:-target/debug/subforge}"
export SF_PORT="${SF_PORT:-18765}"
CFG=$(mktemp -d)
export SUBFORGE_CONFIG_DIR="$CFG/cfg" SUBFORGE_AUTOSAVE_DIR="$CFG/autosave"
"$BIN" serve tests/fixtures/sample.ass --addr "127.0.0.1:$SF_PORT" --no-open >/tmp/subforge-ui-test.log 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true; rm -rf "$CFG"' EXIT
sleep 1
node tests/ui_smoke.cjs
