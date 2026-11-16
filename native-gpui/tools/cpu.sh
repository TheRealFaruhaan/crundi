#!/bin/bash
# CPU use of a process and all its children (CEF's helpers) over SECS
# seconds, in % of one core, and their memory:  tools/cpu.sh <pid> [secs]
P=$1; S=${2:-10}; HZ=$(getconf CLK_TCK)
pids() { echo $P; pgrep -P $P | while read c; do echo $c; pgrep -P $c; done; }
ticks() { for p in $(pids); do awk '{print $14+$15}' /proc/$p/stat 2>/dev/null; done | awk '{s+=$1} END {print s+0}'; }
main_ticks() { awk '{print $14+$15}' /proc/$P/stat; }
a=$(ticks); m=$(main_ticks); sleep $S; b=$(ticks); n=$(main_ticks)
rss=$(for p in $(pids); do awk '/VmRSS/ {print $2}' /proc/$p/status 2>/dev/null; done | awk '{s+=$1} END {printf "%d", s/1024}')
echo "all: $(( (b-a)*100/HZ/S ))%  main process: $(( (n-m)*100/HZ/S ))%  memory (all processes): ${rss} MB"
