#!/bin/bash
# A fresh bare test server whose chats never run the real Claude: it starts
# native-gpui/tools/test-server.sh <port> --bare with HOME set to a folder
# of its own, where ~/.local/bin/claude is tools/claude-stub.mjs (the server
# looks there first: src/claude-ui.js resolveClaudeBin). Nothing of the
# caller's ~/.claude (sign-in, transcripts, settings) is reachable from it.
#
#   native-v2/tools/stub-server.sh 4884
#   … MEASURE_SERVER=http://localhost:4884 node native-v2/tools/measure-pc.mjs
#   kill $(cat /tmp/crundi-devtest-4884/server.pid)
set -euo pipefail
PORT=${1:?usage: stub-server.sh <port>}
HERE=$(cd "$(dirname "$0")" && pwd)
H=/tmp/crundi-devtest-$PORT-home
rm -rf "$H"; mkdir -p "$H/.local/bin"
printf '#!/bin/sh\nexec node %s "$@"\n' "$HERE/claude-stub.mjs" > "$H/.local/bin/claude"
chmod +x "$H/.local/bin/claude"
# The real CLI is not on PATH either (the stub is found first; this keeps a
# `claude --version` or a terminal pane from reaching the owner's).
P=$(printf '%s' "$PATH" | tr ':' '\n' | grep -v -e "$HOME/.local/bin" -e '/.claude' | paste -sd:)
HOME="$H" PATH="$H/.local/bin:$P" exec "$HERE/../../native-gpui/tools/test-server.sh" "$PORT" --bare
