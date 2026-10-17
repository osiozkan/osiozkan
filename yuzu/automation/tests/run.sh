#!/usr/bin/env bash
# Build the yuzu CLI and run every case in cases/*.ass against autoload/kara-templater.lua.
#
# Each case NAME.ass has a NAME.expect with one directive per line ('#' = comment):
#   total: N          exactly N generated lines (Dialogue lines with Effect "fx")
#   has: LINE         the output contains this exact line (fixed string)
#   re: ERE           at least one output line matches the extended regex
#   count: N ERE      exactly N output lines match the regex
#   none: ERE         no output line matches the regex
#   idempotent        running the templater again on the output gives the same output
#   fails: TEXT       the run must fail (non-zero exit) and its stderr must contain TEXT
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"           # yuzu/
SCRIPT="$ROOT/automation/autoload/kara-templater.lua"
cd "$ROOT"
cargo build -q -p yuzu-cli 2>/dev/null || { echo "FATAL: cargo build failed" >&2; cargo build -p yuzu-cli; exit 2; }
BIN="$ROOT/target/debug/yuzu"
[ -x "$BIN" ] || { echo "FATAL: $BIN was not built" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0; failed=()

run_case() {
  local name="$1" src="$HERE/cases/$1.ass" exp="$HERE/cases/$1.expect"
  local out="$TMP/$name.out.ass" err="$TMP/$name.err" txt="$TMP/$name.txt" ok=1
  local fails_text=""
  fails_text="$(grep -m1 '^fails:' "$exp" | sed 's/^fails: *//')" || true
  "$BIN" run "$SCRIPT" 0 "$src" "$out" >"$TMP/$name.stdout" 2>"$err"
  local rc=$?
  if [ -n "$fails_text" ]; then
    if [ $rc -eq 0 ]; then echo "  [$name] expected the run to fail but it succeeded"; return 1; fi
    if ! grep -qF -- "$fails_text" "$err"; then echo "  [$name] failure message lacks '$fails_text':"; sed 's/^/    /' "$err"; return 1; fi
    return 0
  fi
  if [ $rc -ne 0 ]; then echo "  [$name] run failed (exit $rc):"; sed 's/^/    /' "$err"; return 1; fi
  sed '1s/^\xEF\xBB\xBF//' "$out" | tr -d '\r' >"$txt"

  while IFS= read -r l || [ -n "$l" ]; do
    case "$l" in
      ''|'#'*) continue ;;
      total:*)
        local want="${l#total: }" got
        got=$(grep -cE '^Dialogue: .*,fx,' "$txt" || true)
        [ "$got" = "$want" ] || { echo "  [$name] total: expected $want generated lines, got $got"; ok=0; } ;;
      has:*)
        local want="${l#has: }"
        grep -qxF -- "$want" "$txt" || { echo "  [$name] missing line: $want"; ok=0; } ;;
      re:*)
        local re="${l#re: }"
        grep -qE -- "$re" "$txt" || { echo "  [$name] no line matches: $re"; ok=0; } ;;
      count:*)
        local rest="${l#count: }" n re got
        n="${rest%% *}"; re="${rest#* }"
        got=$(grep -cE -- "$re" "$txt" || true)
        [ "$got" = "$n" ] || { echo "  [$name] count of /$re/: expected $n, got $got"; ok=0; } ;;
      none:*)
        local re="${l#none: }"
        if grep -qE -- "$re" "$txt"; then echo "  [$name] unexpected line matching: $re"; grep -E -- "$re" "$txt" | head -3 | sed 's/^/    /'; ok=0; fi ;;
      idempotent)
        local out2="$TMP/$name.out2.ass"
        if ! "$BIN" run "$SCRIPT" 0 "$out" "$out2" >/dev/null 2>"$TMP/$name.err2"; then
          echo "  [$name] second run failed:"; sed 's/^/    /' "$TMP/$name.err2"; ok=0
        elif ! diff -q "$out" "$out2" >/dev/null; then
          echo "  [$name] second run differs from the first:"; diff "$out" "$out2" | head -10 | sed 's/^/    /'; ok=0
        fi ;;
      *) echo "  [$name] unknown directive: $l"; ok=0 ;;
    esac
  done <"$exp"
  [ $ok -eq 1 ] || { echo "  --- output of $name ---"; grep -E '^(Dialogue|Comment)' "$txt" | sed 's/^/    /'; return 1; }
  return 0
}

for src in "$HERE"/cases/*.ass; do
  name="$(basename "$src" .ass)"
  if run_case "$name"; then echo "ok    $name"; pass=$((pass+1)); else echo "FAIL  $name"; fail=$((fail+1)); failed+=("$name"); fi
done

# the macro list must expose both entry points
if "$BIN" macros "$SCRIPT" 2>&1 | grep -q 'Apply karaoke template' && "$BIN" macros "$SCRIPT" 2>&1 | grep -q 'filter \[0\] Karaoke template'; then
  echo "ok    registration"; pass=$((pass+1))
else
  echo "FAIL  registration (macro / filter not listed)"; fail=$((fail+1)); failed+=(registration)
fi

echo
echo "passed: $pass  failed: $fail"
if [ $fail -ne 0 ]; then echo "FAILED: ${failed[*]}"; exit 1; fi
