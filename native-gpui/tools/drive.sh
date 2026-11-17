#!/bin/bash
# One scripted session of the GPUI test client under Xvfb: screenshots of
# each step into $OUT_DIR, and the client's memory and CPU along the way.
# Run with the test server's env (tools/env.sh). Steps: the window as
# loaded; a message typed and sent (Claude's reply streams in); the drawer's
# Device menu, and the page at a phone's size; a stroke annotated over the
# page; the divider dragged; the drawer hidden (the project bar's Browser
# button, with its dot) and shown again.
set -uo pipefail
BIN="${BIN:-$HOME/.cache/crundi-native-target/debug/crundi-gpui}"
OUT_DIR="${OUT_DIR:-/tmp/gpui-drive}"; mkdir -p "$OUT_DIR"; rm -f "$OUT_DIR"/*.png
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}"
export CRUNDI_URL="${CRUNDI_URL:-https://en.wikipedia.org/wiki/Rust_(programming_language)}"
xvfb-run -a -s "-screen 0 1440x900x24" bash -c '
  BIN=$0; OUT=$1
  snap() { xwd -root -silent | convert xwd:- "$OUT/$1.png"; }
  "$BIN" > "$OUT/run.log" 2>&1 & P=$!
  sleep 16; snap 1-loaded
  xdotool mousemove 530 816 click 1; sleep 0.5
  xdotool type --delay 25 "Reply in one short line: say hello from the GPUI client."
  xdotool key Return; sleep 12; snap 2-replied
  # The drawer: its Device menu, then a phone size.
  xdotool mousemove 1352 117 click 1; sleep 1; snap 3-device-menu
  xdotool key Down Down Return; sleep 3; snap 4-phone
  xdotool mousemove 1352 117 click 1; sleep 0.8; xdotool key Up Up Up Up Up Return; sleep 2
  # Annotate: on, a stroke over the page.
  xdotool mousemove 1378 117 click 1; sleep 0.6
  xdotool mousemove 950 420 mousedown 1; for x in 970 1010 1050 1090 1130 1170 1210; do xdotool mousemove $x $((420 + (x-950)/3)); sleep 0.05; done; xdotool mouseup 1
  sleep 1; snap 5-annotated
  # The divider: drag it left.
  xdotool mousemove 859 500 mousedown 1; for x in 840 800 760 720 690; do xdotool mousemove $x 500; sleep 0.05; done; xdotool mouseup 1
  sleep 2; snap 6-wider-drawer
  # Hide the drawer: the Browser button (with its dot) comes back; then open it again.
  xdotool mousemove 1383 89 click 1; sleep 1.5; snap 7-hidden
  echo "browser button: look in 7-hidden.png"
  kill $P; wait $P 2>/dev/null' "$BIN" "$OUT_DIR"
