#!/bin/bash
# Build the client for one area's tests, in the ONE shared target dir, and
# print the path of a copy of the binary that is that area's own
# ($CARGO_TARGET_DIR/debug/crundi-native-<area>, next to CEF's files), so
# another area's build can't swap the binary under a running test (AREAS.md).
#
#   BIN=$(native-client/tools/area-build.sh planning)
#
# Builds are taken one at a time (a lock in the target dir). The build log
# goes to stderr. No other build folders: never set another CARGO_TARGET_DIR,
# never `cargo build --release` unless a test needs it.
set -euo pipefail
AREA=${1:?usage: area-build.sh <workspace|planning|ops|desktop|lead>}
case "$AREA" in workspace|planning|ops|desktop|lead) ;; *) echo "unknown area $AREA" >&2; exit 2 ;; esac
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/crundi-native-target}"
case "$CARGO_TARGET_DIR" in "$HOME/.cache/crundi-native-target") ;; *) echo "CARGO_TARGET_DIR must be \$HOME/.cache/crundi-native-target (one shared build folder)" >&2; exit 2 ;; esac
export CEF_PATH="${CEF_PATH:-$(node scripts/fetch-cef.mjs)}"
mkdir -p "$CARGO_TARGET_DIR"
exec 9>"$CARGO_TARGET_DIR/.area-build.lock"
flock 9
cargo build >&2
node scripts/stage-cef.mjs "$CARGO_TARGET_DIR/debug" >/dev/null
OUT="$CARGO_TARGET_DIR/debug/crundi-native-$AREA"
cp "$CARGO_TARGET_DIR/debug/crundi-native" "$OUT.tmp"
mv -f "$OUT.tmp" "$OUT"
echo "$OUT"
