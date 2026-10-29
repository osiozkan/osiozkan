#!/bin/bash
# kullanım: ship.sh "commit mesajı"  — derle + test + commit + push (hata varsa durur)
set -e
cd "$(dirname "$0")/.."
cargo build --workspace 2>&1 | grep -E "^(error|warning)" -A8 && { echo "DERLEME HATASI/UYARISI"; exit 1; } || true
if cargo test --workspace --exclude osisub-gui 2>&1 | grep -E "test result: F|FAILED|^error" ; then echo "TEST HATASI"; exit 1; fi
git add -A
git commit -q -m "$1

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KSVsijy9qQf7CGoSctZwzM"
git push -q origin claude/vibrant-clarke-2e8jtn 2>&1 | tail -1
echo "gönderildi: $1"
