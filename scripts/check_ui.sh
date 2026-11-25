#!/usr/bin/env bash
# Load the built interface in a headless browser and assert it actually came up.
#
# Every other guard checks files, builds or the CLI. None of them would notice
# if the bundle threw on load and the window rendered blank.
set -euo pipefail

browser=""
for candidate in google-chrome chromium chromium-browser; do
  if command -v "$candidate" >/dev/null 2>&1; then browser="$candidate"; break; fi
done
if [ -z "$browser" ]; then
  echo "::warning::no headless browser available; skipping the interface check"
  exit 0
fi

dom="$("$browser" --headless --no-sandbox --disable-gpu --virtual-time-budget=6000 \
  --allow-file-access-from-files --dump-dom "file://$PWD/demo/index.html" 2>/dev/null || true)"

fail() { echo "::error::$1"; exit 1; }

[ -n "$dom" ] || fail "the interface rendered nothing at all"

modules=$(printf '%s' "$dom" | grep -o 'data-module="[a-z]*"' | sort -u | wc -l)
[ "$modules" -eq 13 ] || fail "expected 13 modules in the rail, found $modules"

ops=$(printf '%s' "$dom" | grep -o 'class="op-row' | wc -l)
[ "$ops" -ge 5 ] || fail "expected the operation list to be populated, found $ops rows"

printf '%s' "$dom" | grep -q 'class="run' || fail "no run button"
printf '%s' "$dom" | grep -q 'cli-echo' || fail "the command echo is missing"
printf '%s' "$dom" | grep -q 'class="form' || fail "the parameter form did not render"

echo "interface came up: $modules modules, $ops operations, form, run button and command echo present"
