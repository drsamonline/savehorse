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

# ---------------------------------------------------------------- behaviour
#
# The page loading is the floor, not the ceiling. These drive the real
# interface and check that the things a person relies on still work: the form
# remembers what was typed, the shortcut runs, and the palette behaves like a
# combobox rather than a list of 87 tab stops.

probe="demo/behaviour-probe.html"
cp demo/index.html "$probe"
cat >> "$probe" <<'PROBEJS'
<script>
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const setInput = (el, v) => {
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  setter.call(el, v);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};
const results = {};
const rows = () => [...document.querySelectorAll(".op-row")];
const formInputs = () =>
  document.querySelectorAll('.form input[type="text"], .form input:not([type])');

(async () => {
  await wait(700);

  // 1. The form remembers what was typed, per operation.
  const first = formInputs()[0];
  if (first) {
    setInput(first, "REMEMBER-ME");
    await wait(300);
    const stats = rows().find((r) => r.textContent.trim().toLowerCase().startsWith("stats"));
    const search = rows().find((r) => r.textContent.trim().toLowerCase().startsWith("search"));
    stats?.click();
    await wait(350);
    search?.click();
    await wait(450);
    results.form_memory = formInputs()[0]?.value === "REMEMBER-ME";
  } else {
    results.form_memory = false;
  }

  // 2. Ctrl+Enter runs the operation on screen.
  const jobsBefore = document.querySelectorAll(".job, .dock-row").length;
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }));
  await wait(600);
  results.shortcut_runs = document.querySelectorAll(".job, .dock-row").length > jobsBefore;

  // 3. The palette opens, arrows move the active option, Tab cannot escape.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true }));
  await wait(450);
  const input = document.querySelector('input[aria-label="Search operations"]');
  if (input) {
    input.focus();
    const before = input.getAttribute("aria-activedescendant");
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await wait(250);
    const after = input.getAttribute("aria-activedescendant");
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    await wait(150);
    results.palette_opens = true;
    results.palette_moves = Boolean(before && after && before !== after);
    results.palette_traps = document.activeElement === input;
    results.palette_untabbable =
      [...document.querySelectorAll('.palette-row[role="option"]')].every((r) => r.tabIndex < 0);
  } else {
    results.palette_opens = false;
  }

  const out = document.createElement("div");
  out.id = "savehorse-probe";
  out.setAttribute("data-results", JSON.stringify(results));
  document.body.appendChild(out);
})();
</script>
PROBEJS

probe_dom="$("$browser" --headless --no-sandbox --disable-gpu \
  --allow-file-access-from-files --virtual-time-budget=15000 \
  --dump-dom "file://$PWD/$probe" 2>/dev/null || true)"
rm -f "$probe"

results="$(printf '%s' "$probe_dom" | grep -o 'data-results="[^"]*"' | head -1 \
  | sed 's/^data-results="//; s/"$//; s/&quot;/"/g')"
[ -n "$results" ] || fail "the behaviour probe never reported; the interface may have stalled"

for check in form_memory shortcut_runs palette_opens palette_moves palette_traps palette_untabbable; do
  case "$results" in
    *"\"$check\":true"*) ;;
    *) fail "$check failed: $results" ;;
  esac
done

echo "behaviour holds: $results"
