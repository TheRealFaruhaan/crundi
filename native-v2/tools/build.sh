#!/bin/bash
# The one build entry for native-v2 (ARCHITECTURE.md, "Building"): the one
# shared build folder (CARGO_TARGET_DIR=$HOME/.cache/crundi-native-target),
# two jobs, one build at a time across everyone (the shared lock), and the
# kernel kills this build first if memory runs out, never Crundi. The
# no-refresh check (tools/check.sh) runs first; it fails the build on a
# cx.refresh_windows() or window.refresh().
#
#   tools/build.sh                     the client (crates/app: debug/crundi-v2)
#   tools/build.sh -p crundi-chat      one crate
#   tools/build.sh --release           a release build (the frame budget's numbers)
#   tools/build.sh test --workspace    cargo test, the same way (the same lock and touch)
#
#   CRUNDI_BUILD_JOBS=<n>              more jobs where the machine is yours (default 2)
#
# Before cargo runs, sources are touched when the last build came from another
# worktree (native-gpui's F18: crates of the same name and version in two
# worktrees share the folder). Never set CARGO_INCREMENTAL=0.
set -euo pipefail
cd "$(dirname "$0")/.."
tools/check.sh
CMD=build; if [ "${1:-}" = test ]; then CMD=test; shift; fi
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/.cache/crundi-native-target"
JOBS="${CRUNDI_BUILD_JOBS:-2}"
export CARGO_BUILD_JOBS="$JOBS"
unset CARGO_INCREMENTAL
echo 1000 > /proc/self/oom_score_adj 2>/dev/null || true
mkdir -p "$CARGO_TARGET_DIR"
# Keep 20 GB free on / (the machine is shared).
FREE=$(df --output=avail -BG / | tail -1 | tr -dc 0-9)
if [ "${FREE:-0}" -lt 20 ]; then echo "only ${FREE} GB free on /: below the 20 GB floor; clean first (tools/clean-build.sh)" >&2; exit 1; fi
exec 9>"$CARGO_TARGET_DIR/.area-build.lock"
flock 9
HERE=$(pwd -P)
LAST="$CARGO_TARGET_DIR/.v2-last-worktree"
OTHER=""
[ -f "$LAST" ] && OTHER=$(find "$CARGO_TARGET_DIR/debug/build" "$CARGO_TARGET_DIR/release/build" -mindepth 2 -maxdepth 2 -path '*/crundi-*' -newer "$LAST" -print -quit 2>/dev/null || true)
if [ "$(cat "$LAST" 2>/dev/null)" != "$HERE" ] || [ -n "$OTHER" ]; then
  find crates -name '*.rs' -path '*/src/*' -exec touch {} +
  find crates -name 'Cargo.toml' -exec touch {} +
  echo "sources touched: the last build was from $(cat "$LAST" 2>/dev/null || echo nowhere)" >&2
fi
PEAKS=/tmp/crundi-v2-peaks-$$.log
tools/rustc-peak.sh "$PEAKS" & PK=$!
trap 'kill $PK 2>/dev/null; rm -f "$PEAKS" "$PEAKS.tmp"' EXIT
S=$(date +%s)
cargo "$CMD" -j"$JOBS" "$@"
echo "$HERE" > "$LAST"
echo "build took $(( $(date +%s) - S )) s; top rustc peaks:"; sleep 1.2; head -5 "$PEAKS" 2>/dev/null || true
