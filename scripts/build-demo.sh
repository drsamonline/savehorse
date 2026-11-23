#!/usr/bin/env bash
# Rebuild the static interface demo in demo/ from the app source.
#
# The demo is the real interface with a snapshot of the manifest and a few
# captured outputs, so it has to be regenerated whenever the app or the
# registry changes. CI runs this and fails if the result differs from what is
# committed, which is what stops the demo going quietly stale.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

app="apps/savehorse-desktop"

npm --prefix "$app" ci --silent
npm --prefix "$app" run build
npx --prefix "$app" vite build --prefix "$app" --base=./ 2>/dev/null || (cd "$app" && npx vite build --base=./)

python3 scripts/export_manifest.py /tmp/savehorse-manifest.json

rm -rf demo
mkdir -p demo
cp -r "$app/dist/." demo/

python3 - <<'PY'
import json
manifest = json.load(open('/tmp/savehorse-manifest.json'))
captured = json.load(open('scripts/demo-captured.json'))
data = {"modules": manifest, "captured": captured}
html = open('demo/index.html').read()
script = ('<script>window.__SAVEHORSE_DATA__ = '
          + json.dumps(data, separators=(',', ':'), ensure_ascii=False) + ';</script>')
html = html.replace('<div id="root"></div>', '<div id="root"></div>\n    ' + script)
html = html.replace('<title>Savehorse</title>', '<title>Savehorse — interface preview</title>')
open('demo/index.html', 'w').write(html)
print(f"demo: {len(manifest)} modules, {sum(len(m['ops']) for m in manifest)} operations, {len(captured)} captured outputs")
PY
