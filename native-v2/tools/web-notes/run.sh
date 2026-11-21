#!/bin/bash
# The web's Notes nesting and block keys (Tab / Shift+Tab, Enter in nested
# lists, Alt+Up/Down, Ctrl+D, Ctrl+Shift+K, "level" kept on save, Markdown),
# checked in the real web UI in Electron under xvfb, against a fresh stub
# server on <port> (stopped again at the end). Needs npm ci (Electron).
#
#   native-v2/tools/web-notes/run.sh 4820        OUT=<dir> also keeps screenshots
set -euo pipefail
PORT=${1:?usage: run.sh <port>}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
cd "$ROOT"
native-v2/tools/stub-server.sh "$PORT" >/dev/null
trap 'kill $(cat /tmp/crundi-devtest-'"$PORT"'/server.pid) 2>/dev/null || true' EXIT
SESS=$(mktemp); export SESS PORT OUT=${OUT:-}
PORT=$PORT node "$HERE/setup.mjs" > "$SESS"
xvfb-run -a -s "-screen 0 1280x900x24" node_modules/.bin/electron --no-sandbox "$HERE/drive.cjs" 2>&1 | grep -E '^(ok|FAIL|all passed|[0-9]+ failed)'
