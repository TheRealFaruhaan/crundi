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
#
# Never set CARGO_INCREMENTAL=0 (it drops dev codegen-units to 16: far more memory).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/.cache/crundi-native-target"
export CARGO_BUILD_JOBS=4
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef/cef_linux_x86_64}"
unset CARGO_INCREMENTAL
echo 1000 > /proc/self/oom_score_adj
mkdir -p "$CARGO_TARGET_DIR"
exec 9>"$CARGO_TARGET_DIR/.area-build.lock"
flock 9
PEAKS=/tmp/crundi-gpui-peaks-$$.log
tools/rustc-peak.sh "$PEAKS" & PK=$!
trap 'kill $PK 2>/dev/null; rm -f "$PEAKS" "$PEAKS.tmp"' EXIT
S=$(date +%s)
cargo build -j4 "$@"
PROFILE=debug; for a in "$@"; do [ "$a" = "--release" ] && PROFILE=release; done
[ -f "$CARGO_TARGET_DIR/$PROFILE/crundi-gpui" ] && STAGE_NAME=crundi-gpui node ../native-client/scripts/stage-cef.mjs "$CARGO_TARGET_DIR/$PROFILE" >/dev/null 2>&1 || true
echo "build took $(( $(date +%s) - S )) s; top rustc peaks:"; sleep 1.2; head -5 "$PEAKS" 2>/dev/null || true
