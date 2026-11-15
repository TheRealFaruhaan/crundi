#!/bin/bash
# Frees the disk a native-client build and its tests take up, on this shared
# machine. Run it at the end of every step (README.md, "Testing phase 2").
#
#   native-client/tools/clean-build.sh
#
# - The shared target dir ($CARGO_TARGET_DIR, default
#   ~/.cache/crundi-native-target): only the 2 newest build-script outputs
#   under debug/build/crundi-native/ (and release/, if there is one);
#   debug/incremental removed when it is over 4 GB.
# - /tmp: the suites' projects and configs (/tmp/crundi-native-*: proj-*,
#   demo*, resume-*, fr-*, config-*), when no test server is running; and
#   the test servers' data dirs (/tmp/crundi-devtest-<port>) whose server is
#   not running.
# Prints the space freed. Removes nothing a running test server uses.
set -u
T=${CARGO_TARGET_DIR:-$HOME/.cache/crundi-native-target}
before=$(df -k --output=avail / | tail -1)

for prof in debug release; do
  d="$T/$prof/build"
  [ -d "$d" ] || continue
  # The newest 2 of the client's build-script dirs (crundi-native/<hash>).
  ls -dt "$d"/crundi-native/*/ 2>/dev/null | tail -n +3 | while read -r x; do rm -rf "$x"; done
done
inc="$T/debug/incremental"
if [ -d "$inc" ] && [ "$(du -sk "$inc" | cut -f1)" -gt $((4 * 1024 * 1024)) ]; then rm -rf "$inc"; fi

# The suites' projects and configs, unless a test server is running now.
running=0
for d in /tmp/crundi-devtest-*; do
  pid=$(cat "$d/server.pid" 2>/dev/null || true)
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && running=1
done
if [ $running = 0 ]; then rm -rf /tmp/crundi-native-* 2>/dev/null; fi
for d in /tmp/crundi-devtest-*; do
  [ -d "$d" ] || continue
  pid=$(cat "$d/server.pid" 2>/dev/null || true)
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then continue; fi
  rm -rf "$d"
done

after=$(df -k --output=avail / | tail -1)
echo "freed $(( (after - before) / 1024 )) MB"
