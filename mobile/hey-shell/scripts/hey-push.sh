#!/usr/bin/env bash
# Simulate the runtime's job: notify a device of a new event by POSTing to its
# ntfy topic. In production this exact POST is fired by the runtime's peer
# receiver when a new gossip message lands for a principal (see
# docs/c-ntfy-push.md). Locally it lets you test the full loop without the
# runtime.
#
#   ./hey-push.sh <topic> "<title>" "<body>"
#   HEY_NTFY=http://192.168.1.10:2587 ./hey-push.sh hey-alice "New DM" "bob: hi"
set -euo pipefail

NTFY="${HEY_NTFY:-http://localhost:2587}"
TOPIC="${1:?usage: hey-push.sh <topic> <title> <body>}"
TITLE="${2:-Hey}"
BODY="${3:-You have a new message}"

curl -s \
  -H "Title: ${TITLE}" \
  -H "Tags: speech_balloon" \
  -H "Priority: high" \
  -d "${BODY}" \
  "${NTFY}/${TOPIC}"
echo
