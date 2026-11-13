#!/bin/bash
# A fresh, isolated dev Crundi on :8889 for design work and tests: its own
# data and config (nothing shared with a live server; never the live Telegram
# bot: its .env is empty), a password sign-in, a project "Demo" and one
# seeded chat. Credentials go to /tmp/crundi-devtest/session.json
# ({password, secret, chat}) for tools/web-session.mjs and test/look.mjs.
#
#   native-client/tools/test-server.sh        (stops the previous one on :8889 first)
set -e
cd "$(dirname "$0")/../.."
P=$(ss -ltnp | grep ":8889 " | grep -o 'pid=[0-9]*' | cut -d= -f2 | head -1); [ -n "$P" ] && kill $P && sleep 2
rm -rf /tmp/crundi-devtest/data/*; PD=/tmp/crundi-native-proj-$(date +%s); mkdir -p $PD /tmp/crundi-devtest/config/crundi
printf 'TELEGRAM_BOT_TOKEN=\nALLOWED_USERNAME=\nPROJECTS_DIR=\n' > /tmp/crundi-devtest/config/crundi/.env
(env -u TELEGRAM_BOT_TOKEN -u PROJECTS_DIR XDG_CONFIG_HOME=/tmp/crundi-devtest/config DATA_DIR=/tmp/crundi-devtest/data DOTENV_PATH=/tmp/crundi-devtest/config/crundi/.env CRUNDI_DEV=1 setsid node src/index.js --dev > /tmp/crundi-devtest/server.log 2>&1 < /dev/null &)
until grep -q "Local:" /tmp/crundi-devtest/server.log; do sleep 1; done
timeout 40 node -e '
const S="http://localhost:8889";const j=(m,p,b,t)=>fetch(S+p,{method:m,headers:{"Content-Type":"application/json",...(t?{Authorization:"Bearer "+t}:{})},body:b?JSON.stringify(b):undefined}).then(r=>r.json());
(async()=>{const pw="native-design-study-"+Date.now();const s=await j("POST","/api/auth/setup",{method:"password",password:pw});
await j("POST","/api/projects",{alias:"demo",name:"Demo",path:process.argv[1],create:true},s.token);
const c=await j("POST","/api/ui-sessions/create",{project:"demo",title:"Design study"},s.token);
await j("POST","/api/ui-sessions/"+c.id+"/send",{text:"Reply with a short markdown list of three fruits, then run the shell command: ls -la"},s.token);
require("fs").writeFileSync("/tmp/crundi-devtest/session.json",JSON.stringify({password:pw,secret:s.totpSecret,chat:c.id}));console.log("seeded",c.id)})()' $PD
