#!/bin/bash
# Run the GPUI test client on a machine with no display (this server): a
# virtual X display, drawn with Mesa's software Vulkan (lavapipe). Takes a
# screenshot after SNAP seconds (default 12) into OUT (default /tmp/gpui.png).
#   CRUNDI_SERVER=… CRUNDI_TOKEN=… CRUNDI_CHAT=… tools/run-xvfb.sh
set -uo pipefail
BIN="${BIN:-$HOME/.cache/crundi-native-target/debug/crundi-gpui}"
OUT="${OUT:-/tmp/gpui.png}"; SNAP="${SNAP:-12}"; LOG="${LOG:-/tmp/gpui-run.log}"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}"
exec xvfb-run -a -s "-screen 0 1440x900x24" bash -c '
  "$0" > "$3" 2>&1 & P=$!
  sleep "$2"
  xwd -root -silent | convert xwd:- "$1"
  [ -n "${HOLD:-}" ] && sleep "$HOLD"
  kill $P; wait $P 2>/dev/null' "$BIN" "$OUT" "$SNAP" "$LOG"
