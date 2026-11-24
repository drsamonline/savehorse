#!/usr/bin/env bash
# Regenerate scripts/demo-captured.json by running the operations the demo
# replays, against the same fixtures the smoke test uses.
#
# The demo is a committed build artefact whose text comes from this file. Until
# this script existed, nothing regenerated it: a change to any of these
# operations' output left the demo showing stale text with every test green.
#
# CI runs this and fails if the result differs from what is committed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out="${1:-/tmp/demo-captured.json}"
# The binary is savehorse.exe on Windows, and it may not have been built yet.
if [ -n "${SAVEHORSE_BIN:-}" ]; then
  bin="$SAVEHORSE_BIN"
else
  cargo build --quiet -p savehorse-cli
  bin="target/debug/savehorse"
  [ -x "$bin" ] || bin="target/debug/savehorse.exe"
fi
if [ ! -x "$bin" ]; then
  echo "could not find the savehorse binary (looked for target/debug/savehorse[.exe])" >&2
  exit 1
fi
# The script moves into a temporary directory to build fixtures, so the binary
# path has to be absolute before that happens.
case "$bin" in
  /*) ;;
  *) bin="$root/$bin" ;;
esac
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"

# Fixtures, matching the smoke test.
mkdir -p smoke/nested smoke/iso
printf 'alpha beta\nsecond line alpha\n' > smoke/a.txt
printf 'alpha beta\nsecond line alpha\n' > smoke/nested/copy.txt
printf 'nothing here\n' > smoke/b.txt
printf 'line one\nline two  \n\n\nline three\n' > smoke/messy.txt
printf 'hello iso\n' > smoke/iso/readme.txt
printf 'payload\n' > smoke/iso/notes.txt
printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > smoke/px.png
printf 'first line\nsecond line\n' > smoke/text.txt
"$bin" halo create smoke/iso --out smoke/test.iso --volume-id SMOKE >/dev/null
"$bin" glyph pdf smoke/text.txt --out smoke/text.pdf --title "Smoke Test" >/dev/null
# A PNG buried in a blob, so revive has something to find.
{ printf 'header junk before the image\n'; cat smoke/px.png; printf 'trailing junk\n'; } > smoke/blob.bin
# Two files for the rename preview.
mkdir -p smoke/ren
printf 'one\n' > smoke/ren/one.txt
printf 'two\n' > smoke/ren/two.txt

python3 - "$bin" "$out" <<'PY'
import json, subprocess, sys

binary, out = sys.argv[1], sys.argv[2]

# operation -> the command that produces the text the demo replays
COMMANDS = {
    "sift.search":       ["sift", "search", "alpha", "smoke"],
    "sift.strings":      ["sift", "strings", "smoke/a.txt", "--min-len", "4"],
    "sift.replace":      ["sift", "replace", "beta", "gamma", "smoke", "--dry-run"],
    "sift.view":         ["sift", "view", "smoke/messy.txt", "--numbers"],
    "sift.stats":        ["sift", "stats", "smoke/messy.txt"],
    "sift.eol":          ["sift", "eol", "smoke/messy.txt"],
    "scout.duplicates":  ["scout", "duplicates", "smoke"],
    "scout.clean":       ["scout", "clean", "smoke"],
    "glyph.extract":     ["glyph", "extract", "smoke/text.pdf"],
    "glyph.readability": ["glyph", "readability", "smoke/text.txt"],
    "halo.list":         ["halo", "list", "smoke/test.iso"],
    "halo.create":       ["halo", "create", "smoke/iso", "--out", "smoke/demo.iso", "--volume-id", "SMOKE"],
    "iris.info":         ["iris", "info", "smoke/px.png"],
    "iris.stats":        ["iris", "stats", "smoke/px.png"],
    "arc.list":          ["arc", "list", "smoke/tree.zip"],
    "forge.hexdump":     ["forge", "hexdump", "smoke/px.png", "--length", "32"],
    "forge.search":      ["forge", "search", "smoke/px.png", "--hex", "89504E47"],
    "revive.scan":       ["revive", "scan", "smoke/blob.bin"],
    "morph.rename":      ["morph", "rename", "smoke/ren", "--case", "upper", "--dry-run"],
}

subprocess.run([binary, "arc", "create", "smoke/iso", "--out", "smoke/tree.zip"],
               check=True, capture_output=True)

captured = {}
for key, args in COMMANDS.items():
    result = subprocess.run([binary] + args, capture_output=True, text=True, encoding="utf-8")
    if result.returncode != 0:
        sys.exit("{} failed: {}".format(key, result.stderr.strip()))
    captured[key] = result.stdout.rstrip("\n")

# newline="" so Windows writes the same bytes as Linux: the file is compared
# byte for byte on every platform.
with open(out, "w", newline="\n") as handle:
    json.dump(captured, handle, indent=2)
    handle.write("\n")
print("captured", len(captured), "outputs to", out)
PY
