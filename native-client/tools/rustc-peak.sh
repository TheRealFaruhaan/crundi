#!/bin/bash
# Samples every rustc of this user once a second and records each crate's
# peak resident memory (RSS + swap), until killed. Use it around a build:
#   tools/rustc-peak.sh /tmp/peaks.log & P=$!; <build>; kill $P; cat /tmp/peaks.log
OUT=${1:-/tmp/rustc-peaks.log}
declare -A PEAK
while true; do
  for pid in $(pgrep -u "$(id -u)" -x rustc); do
    crate=$(tr '\0' ' ' < /proc/$pid/cmdline 2>/dev/null | grep -o -- '--crate-name [^ ]*' | cut -d' ' -f2)
    [ -z "$crate" ] && continue
    kb=$(awk '/^VmRSS|^VmSwap/ {s+=$2} END {print s+0}' /proc/$pid/status 2>/dev/null)
    [ -z "$kb" ] && continue
    if [ "${PEAK[$crate]:-0}" -lt "$kb" ]; then PEAK[$crate]=$kb; fi
  done
  : > "$OUT.tmp"
  for c in "${!PEAK[@]}"; do echo "$((PEAK[$c] / 1024)) MB  $c" >> "$OUT.tmp"; done
  sort -rn "$OUT.tmp" > "$OUT"
  sleep 1
done
