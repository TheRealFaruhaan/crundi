#!/bin/bash
# The browser host beside the app (the Desktop area): builds
# crates/browser-host (the only crate linking CEF; not a default member)
# into the same build folder, then puts CEF's files next to both binaries
# (native-gpui/scripts/stage-cef.mjs). CEF from native-gpui/scripts/fetch-cef.mjs
# (CEF_PATH, default ~/.local/share/cef/cef_<os>_<arch>).
#
#   native-v2/tools/stage-browser.sh [--release]
set -euo pipefail
cd "$(dirname "$0")/.."
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef/cef_linux_x86_64}"
PROFILE=debug; [ "${1:-}" = --release ] && PROFILE=release
tools/build.sh -p crundi-browser-host "$@"
# (build.sh builds into the shared folder; STAGE_DIR for a build made elsewhere.)
STAGE_NAME=crundi-v2 STAGE_HOST=crundi-browser-host node ../native-gpui/scripts/stage-cef.mjs "${STAGE_DIR:-$HOME/.cache/crundi-native-target/$PROFILE}"
# This worktree's .bin gets the host just built and CEF's files beside it
# (build.sh links .bin only on a build of the client; F138).
T="${STAGE_DIR:-$HOME/.cache/crundi-native-target/$PROFILE}"
mkdir -p ".bin/$PROFILE"
[ -f "$T/crundi-browser-host" ] && { ln -f "$T/crundi-browser-host" ".bin/$PROFILE/" 2>/dev/null || cp -f "$T/crundi-browser-host" ".bin/$PROFILE/"; }
find "$T" -maxdepth 1 -type l -exec cp -P -f {} ".bin/$PROFILE/" \;
echo "this worktree's browser host: $(pwd)/.bin/$PROFILE/crundi-browser-host"
