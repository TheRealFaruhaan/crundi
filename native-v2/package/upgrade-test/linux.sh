#!/bin/bash
# The Linux upgrade test, inside Docker (the crundi-upgrade-test image), as
# root for apt and as the user "tester" for the app: install the latest
# Electron .deb, start it and turn launch at login on through Electron itself,
# give it data and a config, install the new .deb over it, and check every one
# carried over, the package is the native one, and the server starts from the
# new files with the carried config. Then remove it, and check the data stays.
#
#   linux.sh <electron.deb> <native.deb> full|client <version>
set -uo pipefail
OLD=$1 NEW=$2 EDITION=$3 VERSION=$4
HERE=$(cd "$(dirname "$0")" && pwd)
PORT=4861
if [ "$EDITION" = client ]; then PKG=crundi-client NAME="Crundi Client" EXE=crundi-client UD="Crundi Client"
else PKG=crundi NAME=Crundi EXE=crundi UD=crundi-desktop; fi
H=/home/tester
USERDATA="$H/.config/$UD"
AUTOSTART="$H/.config/autostart/$NAME.desktop"
FAIL=()
check() { if eval "$2"; then echo "ok    $1"; else echo "FAIL  $1"; FAIL+=("$1"); fi; }
as_tester() { runuser -u tester -- env HOME=$H DISPLAY=:99 DBUS_SESSION_BUS_ADDRESS="unix:path=/tmp/tester-bus" "$@"; }

echo "== apt"
apt-get update -qq >/dev/null
echo "== Electron: $(basename "$OLD")"
apt-get install -y -qq --no-install-recommends "$OLD" >/dev/null 2>&1
check "Electron $PKG installed ($(dpkg-query -W -f='${Version}' $PKG))" "[ -f '/opt/$NAME/resources/app.asar' ]"

# A desktop for the apps: Xvfb and a session bus, as tester.
Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 & XPID=$!
runuser -u tester -- dbus-daemon --session --address=unix:path=/tmp/tester-bus --fork --print-pid > /tmp/bus.pid
sleep 1

# Launch at login, set by Electron itself (app/main.js setLaunchAtStartup's file).
as_tester "/opt/$NAME/$EXE" --no-sandbox --inspect=9230 >/tmp/electron.log 2>&1 & EPID=$!
OLDNODE=$(ls /opt/$NAME/resources/node/bin/node 2>/dev/null || echo /opt/Crundi/resources/node/bin/node)
[ -x "$OLDNODE" ] || OLDNODE=$(command -v node || true)
if [ -z "$OLDNODE" ]; then
  tar -C /tmp -xJf /package-node.tar.xz && OLDNODE=$(ls /tmp/node-v*/bin/node | head -1)
fi
LOGIN=$(as_tester "$OLDNODE" "$HERE/electron-login.mjs" 9230 2>/dev/null); echo "Electron says: $LOGIN"
# The file Electron itself names (app.getName(): the package name, "crundi").
F=$(echo "$LOGIN" | sed -n 's/.*"file":"\([^"]*\)".*/\1/p'); [ -n "$F" ] && AUTOSTART=$F
sleep 3
for p in $(pgrep -u tester -f "/opt/$NAME/"); do kill "$p" 2>/dev/null; done
sleep 2
for p in $(pgrep -u tester -f "/opt/$NAME/"); do kill -9 "$p" 2>/dev/null; done
check "Electron made its autostart file $AUTOSTART" "[ -f '$AUTOSTART' ]"
[ -f "$AUTOSTART" ] && cat "$AUTOSTART"
ls -d "$H/.config"/*/ 2>/dev/null | sed 's/^/  config: /'
check "Electron's userData is $USERDATA" "[ -d '$USERDATA' ]"

# Data and config, as a used install has them.
as_tester mkdir -p "$USERDATA/data/projects"
as_tester bash -c "printf 'TELEGRAM_BOT_TOKEN=\nALLOWED_USERNAME=\nPROJECTS_DIR=\nWEB_PORT=0\n# upgrade-test %s\n' \"\$(date -Is)\" > '$USERDATA/.env'
  echo '{\"mode\":\"local\",\"serverUrl\":\"\",\"voiceWake\":false}' > '$USERDATA/client.json'
  echo '{\"enabled\":true,\"channel\":\"dev\"}' > '$USERDATA/update-config.json'
  echo '{\"enabled\":false}' > '$USERDATA/worker.json'
  echo '{\"carried\":true}' > '$USERDATA/data/projects/upgrade-marker.json'"
SUMS=$(cd "$USERDATA" && find . -type f \( -name '.env' -o -name '*.json' \) -not -path './Partitions/*' | sort | xargs -d '\n' sha256sum)
ASUM=$(sha256sum "$AUTOSTART" 2>/dev/null)
echo "user data: $(echo "$SUMS" | wc -l) files"

echo "== Native: $(basename "$NEW")"
T=$(date +%s)
check "apt installs the new .deb over Electron's" "apt-get install -y -qq --no-install-recommends '$NEW' >/tmp/apt.log 2>&1 || { tail -20 /tmp/apt.log; false; }"
echo "  took $(( $(date +%s) - T )) s"
check "one $PKG package, version $VERSION" "[ \"\$(dpkg-query -W -f='\${Version}' $PKG)\" = '$VERSION' ] && [ \$(dpkg -l | grep -c \"^ii  $PKG \") -eq 1 ]"
check "Electron's files are gone (app.asar, libffmpeg)" "[ ! -e '/opt/$NAME/resources/app.asar' ] && [ ! -e '/opt/$NAME/libffmpeg.so' ]"
check "/opt/$NAME/$EXE is the native client" "[ -x '/opt/$NAME/$EXE' ] && ! grep -q 'Electron Framework' '/opt/$NAME/$EXE' && grep -q 'crundi-browser-host' '/opt/$NAME/$EXE'"
for f in crundi-browser-host crundi-worker libcef.so icudtl.dat; do check "$f installed" "[ -e '/opt/$NAME/$f' ]"; done
check "/usr/bin/$EXE starts /opt/$NAME/$EXE" "[ \"\$(readlink -f /usr/bin/$EXE)\" = '/opt/$NAME/$EXE' ]"
check "desktop entry starts it" "grep -q 'Exec=\"/opt/$NAME/$EXE\"' /usr/share/applications/$PKG.desktop"
MISSING=$(find "/opt/$NAME" -type f \( -name '*.so*' -o -perm -u+x \) -not -path '*/node_modules/*' -exec sh -c '[ "$(head -c4 "$1" | tail -c3)" = ELF ] && LD_LIBRARY_PATH=/opt/'"'$NAME'"' ldd "$1" 2>/dev/null | grep "not found" | sed "s|^|$1: |"' _ {} \; | sort -u)
check "every library the binaries need is there" "[ -z \"\$MISSING\" ]"; [ -n "$MISSING" ] && echo "$MISSING"
if [ "$EDITION" != client ]; then
  check "node/bin/node is Node 22 ($(/opt/$NAME/node/bin/node -v))" "/opt/$NAME/node/bin/node -v | grep -q '^v22\.'"
  for f in server/src/index.js server/app/vendor server/skills/crundi server/node_modules/node-pty server/node_modules/cloudflared/bin/cloudflared server/.env.example; do check "$f installed" "[ -e '/opt/$NAME/$f' ]"; done
  check "node-pty loads in the bundled Node" "as_tester /opt/$NAME/node/bin/node -e \"const p=require('/opt/$NAME/server/node_modules/node-pty');const t=p.spawn('/bin/sh',['-c','echo pty-ok'],{});t.onData(d=>{if(/pty-ok/.test(d)){process.exit(0)}});setTimeout(()=>process.exit(1),5000)\""
else
  check "no server in the client" "[ ! -e '/opt/$NAME/server' ]"
fi

NOW=$(cd "$USERDATA" && echo "$SUMS" | sha256sum -c --quiet 2>&1)
check "data and config kept ($(echo "$SUMS" | wc -l) files)" "[ -z \"\$NOW\" ]"; [ -n "$NOW" ] && echo "$NOW"
check "launch at login kept, unchanged" "[ \"\$(sha256sum '$AUTOSTART' 2>/dev/null)\" = '$ASUM' ] && [ -n '$ASUM' ]"
check "it starts the native client" "grep -q '^Exec=\"/opt/$NAME/$EXE\"' '$AUTOSTART'"
check "one autostart entry" "[ \$(grep -l '/opt/$NAME/' $H/.config/autostart/*.desktop 2>/dev/null | wc -l) -eq 1 ]"

if [ "$EDITION" != client ]; then
  # As the client starts it (app/main.js startBot): the bundled Node, server/src/index.js, the carried .env and data.
  as_tester env -C "/opt/$NAME/server" DOTENV_PATH="$USERDATA/.env" DATA_DIR="$USERDATA/data" ELECTRON_RUN=1 \
    CRUNDI_DEV=1 DEV_WEB_PORT=$PORT HOMEPOOL_DISABLE=1 PATH="/opt/$NAME/node/bin:/usr/bin:/bin" \
    "/opt/$NAME/node/bin/node" --no-deprecation "/opt/$NAME/server/src/index.js" --dev >/tmp/server.log 2>&1 & SPID=$!
  UP=
  for i in $(seq 90); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && { UP=1; break; }; kill -0 $SPID 2>/dev/null || break; sleep 1; done
  check "the server from /opt/$NAME/server answers on :$PORT" "[ -n '$UP' ]"
  [ -z "$UP" ] && tail -40 /tmp/server.log
  kill $SPID 2>/dev/null; sleep 1
fi

# The native client itself, as the desktop entry starts it: report whether it
# stays up and, all-in-one, whether it starts the bundled server.
check "/opt/$NAME/version is $VERSION" "[ \"\$(cat '/opt/$NAME/version' 2>/dev/null)\" = '$VERSION' ]"
as_tester "/opt/$NAME/$EXE" >/tmp/client.log 2>&1 & CPID=$!
CLOG="$H/.config/crundi-native/crundi.log"
# The first update check is 8 s after launch (the carried update-config: on, dev).
for i in $(seq 40); do grep -qE '\[update\] (Up to date|Available|check failed)' "$CLOG" 2>/dev/null && break; sleep 1; done
check "client running after $i s" "kill -0 $CPID 2>/dev/null"
check "the client reports version $VERSION" "grep -q '\[lifecycle\] version=$VERSION ' '$CLOG'"
UPD=$(grep -oE '\[update\] (Up to date|Available: .*|check failed.*)' "$CLOG" | tail -1); echo "  updater: $UPD"
OFFER=$(echo "$UPD" | sed -n 's/^\[update\] Available: //p')
check "it offers no update to its own version or older ($UPD)" "[ \"\$UPD\" = '[update] Up to date' ] || [[ '$OFFER' != *-* && -n '$OFFER' && '$VERSION' == '$OFFER'-* ]] || { [ -n '$OFFER' ] && [ '$OFFER' != '$VERSION' ] && [ \"\$(printf '%s\n%s\n' '$VERSION' '$OFFER' | sort -V | tail -1)\" = '$OFFER' ]; }"
if [ "$EDITION" != client ]; then
  # The server the client starts (startBot): its port from the line the client reads, then it answers.
  CPORT=; CUP=
  for i in $(seq 90); do
    CPORT=$(grep -oE 'HTTP server on [^ ]*:[0-9]+' /tmp/client.log | tail -1 | sed 's/.*://')
    [ -n "$CPORT" ] && curl -s -o /dev/null "http://127.0.0.1:$CPORT/" && { CUP=1; break; }
    sleep 1
  done
  check "the client started the bundled server ($(pgrep -u tester -f "/opt/$NAME/server/src/index.js" | head -1))" "pgrep -u tester -f '/opt/$NAME/server/src/index.js' >/dev/null"
  check "…and it answers on :${CPORT:-?}" "[ -n '$CUP' ]"
  [ -z "$CUP" ] && tail -40 /tmp/client.log
fi
for p in $(pgrep -u tester -f "/opt/$NAME/"); do kill "$p" 2>/dev/null; done
sleep 2; for p in $(pgrep -u tester -f "/opt/$NAME/"); do kill -9 "$p" 2>/dev/null; done

echo "== Remove"
apt-get remove -y -qq "$PKG" >/dev/null 2>&1
check "remove takes the files" "[ ! -e '/opt/$NAME/$EXE' ] && [ ! -e /usr/bin/$EXE ]"
check "remove keeps the data" "[ -f '$USERDATA/.env' ] && [ -f '$USERDATA/data/projects/upgrade-marker.json' ]"

kill $XPID $(cat /tmp/bus.pid) 2>/dev/null
echo "size: native $(du -m "$NEW" | cut -f1) MB (.deb), Electron $(du -m "$OLD" | cut -f1) MB"
if [ ${#FAIL[@]} -gt 0 ]; then echo; echo "${#FAIL[@]} FAILED:"; printf '  %s\n' "${FAIL[@]}"; exit 1; fi
echo; echo "Linux upgrade test passed ($EDITION)"
