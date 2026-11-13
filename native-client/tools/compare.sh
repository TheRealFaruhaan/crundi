#!/bin/bash
# Side by side, region by region: the web UI's screenshot over the native
# client's (same window size), so differences show. One image per region.
#
#   native-client/tools/compare.sh web.png native.png out-prefix  WxH+X+Y [WxH+X+Y …]
#   e.g. … 1400x90+0+0 (top bar and tabs)  240x100+0+760 (sidebar footer)
# Add ZOOM=2 to enlarge small details.
set -e
web=$1; nat=$2; out=$3; shift 3
for r in "$@"; do
  convert "$web" -crop "$r" +repage -scale "$((${ZOOM:-1}*100))%" /tmp/cmp-w.png
  convert "$nat" -crop "$r" +repage -scale "$((${ZOOM:-1}*100))%" /tmp/cmp-n.png
  convert /tmp/cmp-w.png /tmp/cmp-n.png -append "$out-$r.png"
  echo "$out-$r.png"
done
