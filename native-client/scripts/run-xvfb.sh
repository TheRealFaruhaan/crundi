#!/usr/bin/env bash
# Run the prototype on a machine with no display (this server): a virtual X
# display, and Slint's software renderer. Screenshots: xwd -root -display $DISPLAY.
set -euo pipefail
cd "$(dirname "$0")/.."
export SLINT_BACKEND="${SLINT_BACKEND:-winit-software}"
BIN="${BIN:-./target/debug/crundi-native}"
exec xvfb-run -a -s "-screen 0 1440x900x24" "$BIN" "$@"
