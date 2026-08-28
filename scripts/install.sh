#!/usr/bin/env bash
#
# install.sh — install the Crundi server on Linux, natively.
#
# Runs as YOU, not as root and not in a container. That is the point: Claude
# works on your files with your git identity, your SSH keys and your shell, and
# services it starts outlive the terminal you started them from. A container
# turns every one of those into a mounting problem.
#
#   ./scripts/install.sh                 install or upgrade, then enable at boot
#   ./scripts/install.sh --no-service    just install; start it yourself
#   ./scripts/install.sh --prefix DIR    somewhere other than ~/.local/share/crundi
#   ./scripts/install.sh --as-user NAME  run as NAME (created if missing), system service
#   ./scripts/install.sh --as-root       really install for root (see below)
#
# Run as root with no --as-user and it installs for a dedicated 'crundi' user
# instead of root. That is not tidiness: Claude Code REFUSES
# --dangerously-skip-permissions when it is running as root, so a root install
# silently loses that mode, and everything Claude does inherits root's git
# config and SSH keys.
#
# Re-running upgrades in place and keeps your data. Nothing is written outside
# the prefix, ~/.config/crundi and the systemd user unit.

set -euo pipefail

PREFIX=""
CONFIG_DIR=""
UNIT_DIR=""
WITH_SERVICE=1
ASSUME_YES=0
MIN_NODE_MAJOR=20
SERVICE_USER=""       # non-empty => install for that user with a SYSTEM unit
AS_ROOT=0
PREFIX_SET=0

say()  { printf '\033[36m›\033[0m %s\n' "$*"; }
warn() { printf '\033[33m!\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)     PREFIX="${2:?--prefix needs a directory}"; PREFIX_SET=1; shift 2 ;;
    --as-user)    SERVICE_USER="${2:?--as-user needs a name}"; shift 2 ;;
    --as-root)    AS_ROOT=1; shift ;;
    --no-service) WITH_SERVICE=0; shift ;;
    -y|--yes)     ASSUME_YES=1; shift ;;
    -h|--help)    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)            die "Unknown option: $1" ;;
  esac
done

# ─── Who this install belongs to ───
#
# An upgrade re-runs this with an explicit --prefix (that is how the in-app
# updater works), and it runs as the service user, not root — so this only ever
# fires on a genuine root install.
if [ "$(id -u)" -eq 0 ] && [ -z "$SERVICE_USER" ] && [ "$AS_ROOT" -eq 0 ] && [ "$PREFIX_SET" -eq 0 ]; then
  SERVICE_USER=crundi
  say "Running as root — installing for a dedicated '${SERVICE_USER}' user instead."
  say "Claude Code refuses --dangerously-skip-permissions as root, so a root install loses that mode."
  say "Pass --as-root to override."
fi

if [ -n "$SERVICE_USER" ]; then
  [ "$(id -u)" -eq 0 ] || die "--as-user needs root, so it can create the user and write a system unit."
  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    say "Creating user ${SERVICE_USER}"
    useradd -m -s /bin/bash -c 'Crundi server' "$SERVICE_USER"
  fi
  TARGET_HOME="$(getent passwd "$SERVICE_USER" | cut -d: -f6)"
  [ -n "$TARGET_HOME" ] || die "Could not find a home directory for ${SERVICE_USER}."
  [ "$PREFIX_SET" -eq 1 ] || PREFIX="${TARGET_HOME}/.local/share/crundi"
  CONFIG_DIR="${TARGET_HOME}/.config/crundi"
  UNIT_DIR=/etc/systemd/system
else
  TARGET_HOME="$HOME"
  [ "$PREFIX_SET" -eq 1 ] || PREFIX="${HOME}/.local/share/crundi"
  CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/crundi"
  UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  [ "$(id -u)" -eq 0 ] && warn "Installing for root. Claude will act with root's git config and SSH keys, and --dangerously-skip-permissions will not work."
fi

# ─── Source ───
# Works both from a checkout and from the release tarball, which has the same
# shape minus the git metadata.
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -f "$SRC/package.json" ] || die "Cannot find package.json — run this from inside the Crundi directory."

# ─── Node ───
if ! command -v node >/dev/null 2>&1; then
  die "Node.js is not installed. Install Node ${MIN_NODE_MAJOR}+ and run this again:
     Debian/Ubuntu  curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash - && sudo apt install -y nodejs
     Fedora         sudo dnf install -y nodejs
     Arch           sudo pacman -S nodejs npm"
fi
NODE_MAJOR="$(node -p 'process.versions.node.split(".")[0]')"
[ "$NODE_MAJOR" -ge "$MIN_NODE_MAJOR" ] || die "Node ${NODE_MAJOR} is too old. Crundi needs ${MIN_NODE_MAJOR} or newer."
say "Node $(node -v)"

# ─── Native modules ───
#
# node-pty has no prebuilt binary of its own, so it is normally compiled at
# install time — which means a C++ toolchain on a machine that may not want one.
# The release tarball avoids that entirely by shipping node_modules already
# built on CI. Native code is tied to Node's ABI, so that is only usable if the
# Node here matches the one it was built against.
NODE_ABI="$(node -p 'process.versions.modules')"
PREBUILT=0
if [ -f "$SRC/.prebuilt" ] && [ -d "$SRC/node_modules" ]; then
  BUILT_ABI="$(cat "$SRC/.prebuilt" 2>/dev/null | tr -d '[:space:]')"
  if [ "$BUILT_ABI" = "$NODE_ABI" ]; then
    PREBUILT=1
    say "Using the bundled prebuilt modules (Node ABI ${NODE_ABI}) — nothing to compile"
  else
    warn "Bundled modules were built for Node ABI ${BUILT_ABI}, this Node is ${NODE_ABI}. Rebuilding from source."
  fi
fi

if [ "$PREBUILT" -eq 0 ]; then
  MISSING=""
  command -v python3 >/dev/null 2>&1 || MISSING="$MISSING python3"
  command -v make    >/dev/null 2>&1 || MISSING="$MISSING make"
  { command -v c++ >/dev/null 2>&1 || command -v g++ >/dev/null 2>&1; } || MISSING="$MISSING g++"

  if [ -n "$MISSING" ]; then
    # Work out how this machine installs things, and offer to do it. Asking
    # first: this needs sudo, and running a package manager on someone's box
    # without saying so is not on.
    PKG_CMD=""
    if   command -v apt-get >/dev/null 2>&1; then PKG_CMD="sudo apt-get update && sudo apt-get install -y python3 make g++"
    elif command -v dnf     >/dev/null 2>&1; then PKG_CMD="sudo dnf install -y python3 make gcc-c++"
    elif command -v yum     >/dev/null 2>&1; then PKG_CMD="sudo yum install -y python3 make gcc-c++"
    elif command -v pacman  >/dev/null 2>&1; then PKG_CMD="sudo pacman -S --needed --noconfirm python make gcc"
    elif command -v zypper  >/dev/null 2>&1; then PKG_CMD="sudo zypper install -y python3 make gcc-c++"
    elif command -v apk     >/dev/null 2>&1; then PKG_CMD="sudo apk add python3 make g++"
    fi

    if [ -z "$PKG_CMD" ]; then
      die "Missing build tools:${MISSING}
   Crundi compiles node-pty (the terminal backend), and this machine has no
   package manager I recognise. Install python3, make and a C++ compiler, then
   run this again."
    fi

    warn "Missing build tools:${MISSING}"
    say  "These are needed to compile node-pty, the terminal backend."
    say  "Proposed:  ${PKG_CMD}"
    if [ "$ASSUME_YES" -eq 1 ]; then
      REPLY_OK=y
    elif [ -t 0 ]; then
      printf '  Install them now? [y/N] '
      read -r REPLY_OK </dev/tty || REPLY_OK=n
    else
      REPLY_OK=n
    fi
    case "${REPLY_OK:-n}" in
      [yY]*)
        say "Installing build tools"
        eval "$PKG_CMD" || die "Installing the build tools failed. Run it by hand and try again."
        ;;
      *)
        die "Cannot continue without a C++ toolchain. Either run:
     ${PKG_CMD}
   or use the release tarball, which ships them already compiled."
        ;;
    esac
  fi
  say "Build tools present"
fi

# ─── Claude Code ───
# Not installed automatically: it is a separate tool with its own login, and
# quietly adding a global npm package on someone's behalf is not our call.
if command -v claude >/dev/null 2>&1; then
  say "Claude Code $(claude --version 2>/dev/null | head -1)"
else
  warn "Claude Code is not installed. Crundi will start, but every session will fail to launch."
  warn "  npm install -g @anthropic-ai/claude-code   (then run: claude)"
fi

# ─── Install ───
say "Installing to ${PREFIX}"
mkdir -p "$PREFIX" "$CONFIG_DIR"

# Copy rather than symlink: an upgrade should not break a running server by
# swapping files under it, and the source directory may well be a git checkout
# you keep working in.
for item in src scripts assets package.json package-lock.json; do
  [ -e "$SRC/$item" ] && cp -r "$SRC/$item" "$PREFIX/"
done
mkdir -p "$PREFIX/app"
[ -d "$SRC/app/vendor" ] && cp -r "$SRC/app/vendor" "$PREFIX/app/"

if [ "$PREBUILT" -eq 1 ]; then
  say "Copying the prebuilt modules"
  cp -r "$SRC/node_modules" "$PREFIX/"
  cp "$SRC/.prebuilt" "$PREFIX/" 2>/dev/null || true
else
say "Installing dependencies (production only) — this compiles node-pty, so give it a minute"
NPM_LOG="$(mktemp)"
if ! ( cd "$PREFIX" && npm ci --omit=dev --no-audit --no-fund ) >"$NPM_LOG" 2>&1; then
  # ci insists the lockfile match exactly; fall back before giving up.
  if ! ( cd "$PREFIX" && npm install --omit=dev --no-audit --no-fund ) >>"$NPM_LOG" 2>&1; then
    warn "Dependency install failed. The last of the output:"
    tail -25 "$NPM_LOG" >&2
    die "Full log: ${NPM_LOG}"
  fi
fi
rm -f "$NPM_LOG"
fi

# ─── Service ───
if [ "$WITH_SERVICE" -eq 1 ]; then
  if ! command -v systemctl >/dev/null 2>&1; then
    warn "systemd not found — skipping the service. Start it with: node ${PREFIX}/src/index.js"
    WITH_SERVICE=0
  fi
fi

# Everything under the prefix and the config dir was written by root; hand it
# to the user that will actually run it, or the service cannot read its own
# certificate, let alone write history.
if [ -n "$SERVICE_USER" ]; then
  say "Handing ${PREFIX} and ${CONFIG_DIR} to ${SERVICE_USER}"
  mkdir -p "$CONFIG_DIR"
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "$PREFIX" "$CONFIG_DIR"
fi

# A system unit running as the service user. Not a user unit: those need
# lingering to survive logout, and there is no login session here to linger.
if [ "$WITH_SERVICE" -eq 1 ] && [ -n "$SERVICE_USER" ]; then
  mkdir -p "$UNIT_DIR"
  cat > "$UNIT_DIR/crundi.service" <<UNIT
[Unit]
Description=Crundi
Documentation=https://github.com/TheRealFaruhaan/crundi
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
Group=${SERVICE_USER}
ExecStart=$(command -v node) --no-deprecation ${PREFIX}/src/index.js
WorkingDirectory=${PREFIX}
Restart=on-failure
RestartSec=5
Environment=NODE_ENV=production
Environment=HOME=${TARGET_HOME}
Environment=PATH=${TARGET_HOME}/.local/bin:/usr/local/bin:/usr/bin:/bin
# TLS_MODE binds 443 and 80, which are privileged. This grants exactly that and
# nothing else, so the server never has to run as root to answer on them.
AmbientCapabilities=CAP_NET_BIND_SERVICE
# Long-running services Crundi starts are children of this unit; without this
# they are killed the moment the unit restarts.
KillMode=mixed
TimeoutStopSec=20

[Install]
WantedBy=multi-user.target
UNIT

  systemctl daemon-reload
  systemctl enable crundi.service >/dev/null 2>&1 || true
  systemctl restart crundi.service
  sleep 2
  if systemctl is-active --quiet crundi.service; then
    say "Service is running as ${SERVICE_USER}"
  else
    warn "Service did not start. Logs:  journalctl -u crundi -n 40 --no-pager"
  fi

elif [ "$WITH_SERVICE" -eq 1 ]; then
  mkdir -p "$UNIT_DIR"
  cat > "$UNIT_DIR/crundi.service" <<UNIT
[Unit]
Description=Crundi
Documentation=https://github.com/TheRealFaruhaan/crundi
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=$(command -v node) --no-deprecation ${PREFIX}/src/index.js
WorkingDirectory=${PREFIX}
Restart=on-failure
RestartSec=5
# Claude and the terminals it spawns inherit this environment, so it wants to
# look like a login shell rather than a bare service.
Environment=NODE_ENV=production
Environment=HOME=${HOME}
Environment=PATH=${HOME}/.local/bin:/usr/local/bin:/usr/bin:/bin
# Long-running services Crundi starts are children of this unit; without this
# they are killed the moment the unit restarts.
KillMode=mixed
TimeoutStopSec=20

[Install]
WantedBy=default.target
UNIT

  systemctl --user daemon-reload
  systemctl --user enable crundi.service >/dev/null 2>&1 || true

  # A user service normally stops at logout. Lingering is what makes this a
  # server rather than something that dies when you close SSH.
  if command -v loginctl >/dev/null 2>&1; then
    if ! loginctl show-user "$USER" --property=Linger 2>/dev/null | grep -q 'Linger=yes'; then
      say "Enabling lingering so Crundi runs without you logged in"
      loginctl enable-linger "$USER" 2>/dev/null \
        || warn "Could not enable lingering (needs sudo: sudo loginctl enable-linger $USER). Crundi will stop when you log out."
    fi
  fi

  systemctl --user restart crundi.service
  sleep 2
  if systemctl --user is-active --quiet crundi.service; then
    say "Service is running"
  else
    warn "Service did not start. Logs:  journalctl --user -u crundi -n 40 --no-pager"
  fi
fi

PORT="$(grep -sE '^WEB_PORT=' "$CONFIG_DIR/.env" | cut -d= -f2 || true)"
PORT="${PORT:-8888}"

cat <<DONE

  Crundi is installed.

    Open        http://localhost:${PORT}
    Data        ${CONFIG_DIR}
    Program     ${PREFIX}

  On first load it will ask you to set up a way to sign in. Until you do, that
  is the only thing it will let anyone do — but it is first-come, so do it
  before opening the port to anything you do not trust.

DONE

if [ "$WITH_SERVICE" -eq 1 ] && [ -n "$SERVICE_USER" ]; then
cat <<DONE
    Runs as     ${SERVICE_USER}
    Status      systemctl status crundi
    Logs        journalctl -u crundi -f
    Stop        systemctl stop crundi
    Upgrade     re-run this script, or use Settings in the app

  Claude Code logs in per user, so sign it in as the one that runs it:

    sudo -u ${SERVICE_USER} -H claude

DONE
elif [ "$WITH_SERVICE" -eq 1 ]; then
cat <<DONE
    Status      systemctl --user status crundi
    Logs        journalctl --user -u crundi -f
    Stop        systemctl --user stop crundi
    Upgrade     re-run this script

DONE
else
cat <<DONE
    Start       node ${PREFIX}/src/index.js

DONE
fi

say "Back up ${CONFIG_DIR} — it holds your projects list, history and sign-in credentials."
