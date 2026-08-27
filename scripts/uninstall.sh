#!/usr/bin/env bash
#
# uninstall.sh — remove the Crundi server installed by install.sh.
#
# Leaves your data alone by default: ~/.config/crundi holds your projects list,
# history and sign-in credentials, and deleting that silently would be a poor
# way to repay someone for typing "uninstall".
#
#   ./scripts/uninstall.sh            remove the program and the service
#   ./scripts/uninstall.sh --purge    also delete ~/.config/crundi

set -euo pipefail

PREFIX="${HOME}/.local/share/crundi"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/crundi"
UNIT="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/crundi.service"
PURGE=0

say()  { printf '\033[36m›\033[0m %s\n' "$*"; }
warn() { printf '\033[33m!\033[0m %s\n' "$*" >&2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --purge)   PURGE=1; shift ;;
    --prefix)  PREFIX="${2:?--prefix needs a directory}"; shift 2 ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)         warn "Unknown option: $1"; exit 1 ;;
  esac
done

if command -v systemctl >/dev/null 2>&1; then
  systemctl --user stop crundi.service 2>/dev/null || true
  systemctl --user disable crundi.service 2>/dev/null || true
fi
[ -f "$UNIT" ] && rm -f "$UNIT" && say "Removed the service unit"
command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload 2>/dev/null || true

if [ -d "$PREFIX" ]; then
  rm -rf "$PREFIX"
  say "Removed ${PREFIX}"
fi

if [ "$PURGE" -eq 1 ]; then
  if [ -d "$CONFIG_DIR" ]; then
    rm -rf "$CONFIG_DIR"
    say "Removed ${CONFIG_DIR} — projects list, history and credentials are gone."
  fi
else
  say "Left ${CONFIG_DIR} in place (pass --purge to delete it too)."
fi

say "Lingering was not changed. To undo it:  loginctl disable-linger $USER"
say "Claude Code was not installed by Crundi and has not been removed."
