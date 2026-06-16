#!/usr/bin/env bash
# Run a local, self-hosted ntfy server (no Google, no cloud) for Hey push.
# Default: http://localhost:2587  (override with HEY_NTFY_PORT).
set -euo pipefail

PORT="${HEY_NTFY_PORT:-2587}"
NAME="ntfy-hey"
ENGINE="$(command -v podman || command -v docker)"

if [ -z "$ENGINE" ]; then
  echo "need podman or docker" >&2; exit 1
fi

"$ENGINE" rm -f "$NAME" >/dev/null 2>&1 || true
"$ENGINE" run -d --name "$NAME" -p "${PORT}:80" \
  docker.io/binwiederhier/ntfy serve --base-url "http://localhost:${PORT}"

sleep 2
echo "ntfy up: http://localhost:${PORT}  (health: $(curl -s http://localhost:${PORT}/v1/health))"
echo "subscribe test: curl -s http://localhost:${PORT}/hey-test/json"
echo "stop: $ENGINE rm -f $NAME"
