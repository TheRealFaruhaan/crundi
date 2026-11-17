#!/bin/sh
# Adds Crundi (this unpacked GPUI build) to the desktop's apps, with its icon:
# the dock, the app grid and the window switcher show Crundi's logo (Wayland
# takes a window's icon from here, matched by StartupWMClass). Run it once
# from the unpacked folder; run it again after moving the folder.
#   ./install-desktop-entry.sh            add
#   ./install-desktop-entry.sh --remove   take it away
set -eu
DIR=$(cd "$(dirname "$0")" && pwd)
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
if [ "${1:-}" = "--remove" ]; then
  rm -f "$APPS/crundi.desktop" "$ICONS/crundi.png"
  echo "Removed Crundi from the desktop's apps."
  exit 0
fi
mkdir -p "$APPS" "$ICONS"
cp "$DIR/crundi.png" "$ICONS/crundi.png"
cat > "$APPS/crundi.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Crundi
Comment=Crundi's PC client
Exec="$DIR/crundi-gpui"
Icon=crundi
Terminal=false
Categories=Development;
StartupWMClass=crundi
DESKTOP
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" >/dev/null 2>&1 || true
echo "Added Crundi to the desktop's apps ($APPS/crundi.desktop)."
