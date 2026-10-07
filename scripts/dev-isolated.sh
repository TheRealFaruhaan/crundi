#!/usr/bin/env bash
# Start a dev Crundi that cannot touch the live one on the same machine.
#
# `npm run dev:headless` alone is not enough here: started from a shell that
# Crundi itself spawned, it inherits the live server's environment - the
# Telegram token (two pollers on one token fight), the Cloudflare tokens, the
# TLS settings and the live data folder (schedules would fire twice).
#
# This strips those, uses .dev-data/ for state, and binds loopback on 8889.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .dev-data
[ -f .env.dev ] || printf 'PROJECTS_DIR=%s\nTLS_MODE=off\n' "${PROJECTS_DIR:-$HOME/projects}" > .env.dev
for v in TELEGRAM_BOT_TOKEN ALLOWED_USERNAME CLOUDFLARE_TUNNEL_TOKEN CLOUDFLARE_TUNNEL_URL CLOUDFLARE_DNS_TOKEN \
         TLS_MODE TLS_DOMAIN TLS_EMAIL TLS_WILDCARD TLS_STAGING WEB_PORT \
         CRUNDI_API_URL CRUNDI_API_KEY CRUNDI_UI_SESSION CRUNDI_CHAT_ID CRUNDI_PROJECT CRUNDI_TERMINAL_ID; do
  unset "$v" || true
done
export DOTENV_PATH="$PWD/.env.dev" DATA_DIR="$PWD/.dev-data" CRUNDI_DEV=1
exec node src/index.js --dev "$@"
