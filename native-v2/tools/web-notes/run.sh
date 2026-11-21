#!/bin/bash
# The web's Notes nesting and block keys (Tab / Shift+Tab on paragraphs and list items, children moving with their parent, folding, Enter in nested
# lists, Alt+Up/Down, Ctrl+D, Ctrl+Shift+K, "level" kept on save, Markdown),
# checked in the real web UI in Electron under xvfb, against a fresh stub
# server on <port> (stopped again at the end). Needs Electron: npm ci --include=dev.
#
#   native-v2/tools/web-notes/run.sh 4820        OUT=<dir> also keeps screenshots
set -euo pipefail
PORT=${1:?usage: run.sh <port>}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
cd "$ROOT"
if [ ! -x node_modules/.bin/electron ]; then
  echo "run.sh needs Electron: run 'npm ci --include=dev' in $ROOT first (npm leaves dev dependencies out when NODE_ENV=production)." >&2
  exit 1
fi
native-v2/tools/stub-server.sh "$PORT" >/dev/null
SESS=$(mktemp); export SESS PORT OUT=${OUT:-}
XR=""
# However it ends (done, a failure, a timeout's TERM, Ctrl+C): the Xvfb and
# Electron xvfb-run started are stopped by their PIDs, its auth file goes, and
# the stub server is stopped (F208: a killed xvfb-run left its Xvfb behind).
cleanup() {
  if [ -n "$XR" ]; then
    for p in $(pgrep -P "$XR" 2>/dev/null); do kill "$p" 2>/dev/null || true; done
    kill "$XR" 2>/dev/null || true
  fi
  kill "$(cat /tmp/crundi-devtest-$PORT/server.pid 2>/dev/null)" 2>/dev/null || true
  rm -f "$SESS" "$SESS.auth"
}
trap cleanup EXIT
trap 'exit 143' TERM INT HUP
PORT=$PORT node "$HERE/setup.mjs" > "$SESS"
# (-f: our own auth file, so xvfb-run makes no /tmp/xvfb-run.* folder to leave.)
xvfb-run -a -f "$SESS.auth" -s "-screen 0 1280x900x24" node_modules/.bin/electron --no-sandbox "$HERE/drive.cjs" > "$SESS.log" 2>&1 &
XR=$!
wait "$XR" || RC=$?
XR=""
grep -E '^(ok|FAIL|all passed|[0-9]+ failed)' "$SESS.log" || { echo "the Electron run printed no results; its output:" >&2; tail -n 40 "$SESS.log" >&2; }
rm -f "$SESS.log"
exit ${RC:-0}
