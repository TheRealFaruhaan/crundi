#!/bin/bash
# The architecture's rule 2 (ARCHITECTURE.md): no cx.refresh_windows() and
# no window.refresh() anywhere in native-v2 (both drop every view cache in
# the window). CI runs this; tools/build.sh runs it before every build.
# Comments don't count; code does, in any crate.
set -euo pipefail
cd "$(dirname "$0")/.."
HITS=$(grep -rnE '(\.refresh_windows\s*\(|\bwindow\s*\.\s*refresh\s*\(|\bcx\s*\.\s*refresh\s*\()' crates --include='*.rs' | grep -vE '^[^:]+:[0-9]+:\s*//' || true)
if [ -n "$HITS" ]; then
  echo "native-v2 rule 2: no refresh_windows() / window.refresh() (notify the view that changed instead):" >&2
  echo "$HITS" >&2
  exit 1
fi
echo "check: no window refresh in native-v2"
