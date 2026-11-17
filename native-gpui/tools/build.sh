#!/bin/bash
# The one build entry for the GPUI client (AREAS.md, shared/BUILD-RULES.md):
# the one shared build folder (CARGO_TARGET_DIR=$HOME/.cache/crundi-native-target),
# four jobs, one build at a time across everyone (the shared lock), and the
# kernel kills this build first if memory runs out, never Crundi. Records
# each rustc's peak memory and the build's wall time, and stages CEF next to
# the binary (native-client/scripts/stage-cef.mjs, shared with the Slint client).
#
#   tools/build.sh                     the client (crates/app: debug/crundi-gpui)
#   tools/build.sh -p crundi-planning  one crate
#   tools/build.sh --release           a release build (only when a test needs one)
#   tools/build.sh test --workspace    cargo test, the same way (the same lock and touch)
#
# Before cargo runs, sources are touched when the last build came from another
# worktree (F18, below). Never set CARGO_INCREMENTAL=0 (it drops dev codegen-units to 16: far more memory).
set -euo pipefail
cd "$(dirname "$0")/.."
CMD=build; if [ "${1:-}" = test ]; then CMD=test; shift; fi
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/.cache/crundi-native-target"
export CARGO_BUILD_JOBS=4
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef/cef_linux_x86_64}"
unset CARGO_INCREMENTAL
echo 1000 > /proc/self/oom_score_adj
mkdir -p "$CARGO_TARGET_DIR"
exec 9>"$CARGO_TARGET_DIR/.area-build.lock"
flock 9
# Stale builds (F18): every worktree builds into this one folder, and the
# workspace's crates have the same names and versions in each, so cargo can
# take another worktree's newer build of a crate as up to date for this one.
# When the last build here came from another worktree, make this worktree's
# sources newer than anything built (a touch: no extra disk, no per-worktree
# folders); the crates then rebuild from this worktree's files.
HERE=$(pwd -P)
LAST="$CARGO_TARGET_DIR/.gpui-last-worktree"
# Also when any crundi-* build output is newer than this marker: a build that
# didn't come through this script (a direct cargo test from another
# worktree) left it, and the marker alone can't see that (F82).
OTHER=""
[ -f "$LAST" ] && OTHER=$(find "$CARGO_TARGET_DIR/debug/build" "$CARGO_TARGET_DIR/release/build" -mindepth 2 -maxdepth 2 -path '*/crundi-*' -newer "$LAST" -print -quit 2>/dev/null || true)
if [ "$(cat "$LAST" 2>/dev/null)" != "$HERE" ] || [ -n "$OTHER" ]; then
  find crates -name '*.rs' -path '*/src/*' -exec touch {} +
  find crates -name 'Cargo.toml' -exec touch {} +
  echo "sources touched: the last build was from $(cat "$LAST" 2>/dev/null || echo nowhere)" >&2
fi
PEAKS=/tmp/crundi-gpui-peaks-$$.log
tools/rustc-peak.sh "$PEAKS" & PK=$!
trap 'kill $PK 2>/dev/null; rm -f "$PEAKS" "$PEAKS.tmp"' EXIT
S=$(date +%s)
cargo "$CMD" -j4 "$@"
# The marker after the build: this build's own outputs are older than it.
echo "$HERE" > "$LAST"
PROFILE=debug; for a in "$@"; do [ "$a" = "--release" ] && PROFILE=release; done
[ -f "$CARGO_TARGET_DIR/$PROFILE/crundi-gpui" ] && STAGE_NAME=crundi-gpui STAGE_HOST=crundi-browser-host node ../native-client/scripts/stage-cef.mjs "$CARGO_TARGET_DIR/$PROFILE" >/dev/null 2>&1 || true
echo "build took $(( $(date +%s) - S )) s; top rustc peaks:"; sleep 1.2; head -5 "$PEAKS" 2>/dev/null || true
