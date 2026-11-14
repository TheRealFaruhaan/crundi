#!/bin/bash
# The 2.20 cards' review pairs: for every web state in specs/cards/web-*.png
# (not the -2x crops), the web's Manager pane (the right half of the
# workbench) over the native one (test/phase2.mjs's gallery writes
# cards-native-<state>.png and cards-rects.json to DRIVE_OUT), saved as
# specs/cards/review/<state>.png. The native full windows are copied to
# specs/cards/native-<state>.png.
#
#   native-client/tools/cards-review.sh <DRIVE_OUT of a phase2 run>
set -e
OUT=${1:?usage: $0 <phase2 DRIVE_OUT>}
cd "$(dirname "$0")/../specs/cards"
mkdir -p review
for web in web-*.png; do
  case "$web" in *-2x.png) continue ;; esac
  st=${web#web-}; st=${st%.png}
  nat="$OUT/cards-native-$st.png"
  [ -f "$nat" ] || { echo "no native shot for $st"; continue; }
  cp "$nat" "native-$st.png"
  W=$(identify -format %w "$web"); H=$(identify -format %h "$web")
  # The web's Manager pane: the right half of the workbench (it starts at 240).
  X=$(( 240 + (W - 240) / 2 )); PW=$(( W - X )); PH=$(( H - 88 ))
  R=$(node -e "const r=require('$OUT/cards-rects.json')['$st']; console.log(r ? [Math.round(r.w)+4, Math.round(r.h)+4, Math.round(r.x)-2, Math.round(r.y)-2].join(' ') : '')")
  [ -n "$R" ] || { echo "no rectangle for $st"; continue; }
  set -- $R
  convert -background '#000000' \( "$web" -crop ${PW}x${PH}+${X}+88 +repage \) \( "$nat" -crop ${1}x${2}+${3}+${4} +repage \) -append "review/$st.png"
  echo "review/$st.png"
done
