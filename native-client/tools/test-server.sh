#!/bin/bash
# A fresh, isolated dev Crundi for design work and tests: its own data and
# config under /tmp/crundi-devtest-<port> (nothing shared with a live server,
# nor with a test server on another port), never the live Telegram bot (its
# .env is empty and TELEGRAM_BOT_TOKEN is unset), no homepool listeners (so
# two can run side by side), and no Cloudflare (--dev).
#
# ALWAYS start a test server through this script, never `node src/index.js`
# by hand: a dev server started without the empty .env picks up the real one
# and runs the LIVE Telegram bot. test/phase1.mjs and test/phase2.mjs refuse a
# server that has a bot configured.
#
#   native-client/tools/test-server.sh [port] [--bare]
#
#   port     DEV_WEB_PORT, or the first argument; default 8889. In a factory
#            run, use the run's range (e.g. 4810-4819).
#   --bare   no sign-in set up and nothing seeded: what test/phase1.mjs and
#            test/phase2.mjs need. Without it: a password sign-in, a project
#            "Demo" and one seeded chat, with the credentials in
#            /tmp/crundi-devtest-<port>/session.json ({password, secret, chat})
#            for tools/web-session.mjs <port> and test/look.mjs <port>.
#
# It stops the previous test server on the same port first (only one this
# script started there: its PID is in /tmp/crundi-devtest-<port>/server.pid),
# and refuses a port held by anything else. Stop it with
#   kill $(cat /tmp/crundi-devtest-<port>/server.pid)
set -e
cd "$(dirname "$0")/../.."
PORT=${DEV_WEB_PORT:-8889}; BARE=
for a in "$@"; do case "$a" in --bare) BARE=1 ;; *[!0-9]*|'') echo "usage: $0 [port] [--bare]" >&2; exit 2 ;; *) PORT=$a ;; esac; done
case "$PORT" in 22|53|80|443|8888) echo "port $PORT belongs to the machine or the live Crundi" >&2; exit 2 ;; esac
if [ "$PORT" -ge 7480 ] && [ "$PORT" -le 7599 ]; then echo "port $PORT is homepool's (7480-7599)" >&2; exit 2; fi
D=/tmp/crundi-devtest-$PORT

# The previous one on this port, if this script started it; anything else there is not ours.
HOLDER=$(ss -ltnpH "sport = :$PORT" 2>/dev/null | grep -o 'pid=[0-9]*' | cut -d= -f2 | head -1)
if [ -n "$HOLDER" ]; then
  if [ -f "$D/server.pid" ] && [ "$HOLDER" = "$(cat "$D/server.pid")" ]; then
    kill "$HOLDER"; for i in $(seq 20); do kill -0 "$HOLDER" 2>/dev/null || break; sleep 0.5; done
  else
    echo "port $PORT is taken by pid $HOLDER, which this script did not start; pick another port" >&2; exit 1
  fi
elif ss -ltnH "sport = :$PORT" | grep -q .; then
  echo "port $PORT is taken (by a process this user cannot see); pick another port" >&2; exit 1
fi

rm -rf "$D"; mkdir -p "$D/data" "$D/config/crundi" "$D/proj"
printf 'TELEGRAM_BOT_TOKEN=\nALLOWED_USERNAME=\nPROJECTS_DIR=\n' > "$D/config/crundi/.env"
env -u TELEGRAM_BOT_TOKEN -u ALLOWED_USERNAME -u PROJECTS_DIR \
  XDG_CONFIG_HOME="$D/config" DATA_DIR="$D/data" DOTENV_PATH="$D/config/crundi/.env" \
  CRUNDI_DEV=1 DEV_WEB_PORT="$PORT" HOMEPOOL_DISABLE=1 \
  setsid node src/index.js --dev > "$D/server.log" 2>&1 < /dev/null &
echo $! > "$D/server.pid"
for i in $(seq 60); do grep -q "Local:" "$D/server.log" && break; kill -0 "$(cat "$D/server.pid")" 2>/dev/null || break; sleep 1; done
if ! grep -q "Local:" "$D/server.log"; then echo "the server did not start; see $D/server.log" >&2; tail -20 "$D/server.log" >&2; exit 1; fi
echo "test server on http://localhost:$PORT  pid $(cat "$D/server.pid")  log $D/server.log"
[ -n "$BARE" ] && exit 0

timeout 40 node -e '
const [port, dir] = process.argv.slice(1);
const S="http://localhost:"+port;const j=(m,p,b,t)=>fetch(S+p,{method:m,headers:{"Content-Type":"application/json",...(t?{Authorization:"Bearer "+t}:{})},body:b?JSON.stringify(b):undefined}).then(r=>r.json());
(async()=>{const pw="native-design-study-"+Date.now();const s=await j("POST","/api/auth/setup",{method:"password",password:pw});
await j("POST","/api/projects",{alias:"demo",name:"Demo",path:dir+"/proj",create:true},s.token);
const c=await j("POST","/api/ui-sessions/create",{project:"demo",title:"Design study"},s.token);
await j("POST","/api/ui-sessions/"+c.id+"/send",{text:"Reply with a short markdown list of three fruits, then run the shell command: ls -la"},s.token);
require("fs").writeFileSync(dir+"/session.json",JSON.stringify({password:pw,secret:s.totpSecret,chat:c.id,server:S}));console.log("seeded",c.id,"session",dir+"/session.json")})()' "$PORT" "$D"
