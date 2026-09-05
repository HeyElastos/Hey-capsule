#!/usr/bin/env bash
# Home's opaque sandbox drops / fails Trunk integrity= on modulepreload and
# wasm preload. Strip them from dist/index.html after `trunk build`.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
html="$root/dist/index.html"
[ -f "$html" ] || { echo "strip-sri: missing $html" >&2; exit 1; }
python3 - "$html" <<'PY'
import re, sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
# Drop integrity="…" and the optional crossorigin that only existed for SRI.
out = re.sub(r'\s+integrity="[^"]*"', "", text)
open(path, "w", encoding="utf-8").write(out)
print(f"strip-sri: cleaned {path}")
PY
