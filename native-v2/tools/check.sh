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

# No merge-conflict markers in native-v2/ or .github/ (F33: a sync once
# committed them into 5 files unnoticed). A file is flagged when a line starts
# with "<<<<<<< " or ">>>>>>> " (or is exactly the marker); a lone "======="
# counts only in a file that also has one of those, since markdown uses a row
# of = to underline a heading. Binary files are skipped.
ROOTS=(.)
[ -d ../.github ] && ROOTS+=(../.github)
CONFLICT=$(grep -rlIE '^(<<<<<<<|>>>>>>>)( |$)' "${ROOTS[@]}" --exclude-dir=.git || true)
if [ -n "$CONFLICT" ]; then
  echo "merge-conflict markers (resolve them before building):" >&2
  while IFS= read -r f; do grep -nE '^(<<<<<<<|=======|>>>>>>>)( |$)' "$f" | sed "s|^|$f:|" >&2; done <<< "$CONFLICT"
  exit 1
fi
echo "check: no merge-conflict markers in native-v2 or .github"
