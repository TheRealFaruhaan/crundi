#!/bin/bash
# Wrap a staged install folder (package/stage.mjs) into the .deb that
# replaces Electron's: same package name, same /opt place and executable,
# same /usr/bin link and desktop entry (package/CONTRACT.md, "Identity").
#
#   native-v2/package/linux/build-deb.sh full|client <stage dir> <out dir> <version>
#
# Writes Crundi-<v>-linux-x64.deb + latest-linux.yml, or
# Crundi-Client-<v>-linux-x64.deb + client-linux.yml.
set -euo pipefail
EDITION=$1 STAGE=$(realpath "$2") OUT=$(realpath -m "$3") VERSION=$4
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
if [ "$EDITION" = client ]; then
  PKG=crundi-client NAME="Crundi Client" EXE=crundi-client FILE="Crundi-Client-$VERSION-linux-x64.deb" FEED=client-linux.yml
  SYNOPSIS="Desktop app for a Crundi server running elsewhere"
else
  PKG=crundi NAME=Crundi EXE=crundi FILE="Crundi-$VERSION-linux-x64.deb" FEED=latest-linux.yml
  SYNOPSIS="Self-hosted workbench for your projects"
fi
DESC="Self-hosted remote workbench for your projects — agent terminals, files, git, kanban, mindmap and scheduling, reachable from anywhere."
ROOT=$(mktemp -d "${TMPDIR:-/tmp}/crundi-deb.XXXX")
trap 'rm -rf "$ROOT"' EXIT
mkdir -p "$ROOT/DEBIAN" "$ROOT/opt" "$ROOT/usr/share/applications" "$ROOT/usr/share/icons/hicolor/512x512/apps" "$ROOT/usr/share/doc/$PKG"
cp -a "$STAGE" "$ROOT/opt/$NAME"
cp "$REPO/assets/icon_512x512.png" "$ROOT/usr/share/icons/hicolor/512x512/apps/$PKG.png"
cat > "$ROOT/usr/share/applications/$PKG.desktop" <<DESK
[Desktop Entry]
Name=$NAME
Exec="/opt/$NAME/$EXE" %U
Terminal=false
Type=Application
Icon=$PKG
StartupWMClass=$NAME
Comment=$DESC
Categories=Development;
DESK
printf '%s (%s) stable; urgency=low\n\n  * The native client replaces the Electron app.\n\n -- Obitech <TheRealFaruhaan@users.noreply.github.com>\n' "$PKG" "$VERSION" | gzip -9n > "$ROOT/usr/share/doc/$PKG/changelog.gz"

# The glibc the binaries were built against (a CI build on 24.04 asks 2.39).
GLIBC=$(find "$STAGE" -maxdepth 1 -type f -perm -u+x -exec objdump -T {} \; 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
SIZE=$(du -sk --exclude=DEBIAN "$ROOT" | cut -f1)
cat > "$ROOT/DEBIAN/control" <<CTL
Package: $PKG
Version: $VERSION
License: MIT
Vendor: Obitech <TheRealFaruhaan@users.noreply.github.com>
Architecture: amd64
Maintainer: Obitech <TheRealFaruhaan@users.noreply.github.com>
Installed-Size: $SIZE
Depends: libc6 (>= ${GLIBC:-2.39}), libnss3, libgbm1, libasound2t64 | libasound2, libxkbcommon0, libxkbcommon-x11-0, libxcb1, libfontconfig1, libfreetype6, libvulkan1, mesa-vulkan-drivers | vulkan-icd, libatk1.0-0t64 | libatk1.0-0, libatk-bridge2.0-0t64 | libatk-bridge2.0-0, libcups2t64 | libcups2, libxcomposite1, libxdamage1, libxrandr2, libpango-1.0-0, libcairo2, libdbus-1-3, libgtk-3-0t64 | libgtk-3-0, xdg-utils
Recommends: libayatana-appindicator3-1
Section: default
Priority: optional
Homepage: https://crundi.faruhaan.com
Description: $SYNOPSIS
  $DESC
CTL
# As Electron's: /usr/bin/<exe> through update-alternatives, desktop database.
cat > "$ROOT/DEBIAN/postinst" <<POST
#!/bin/bash
if type update-alternatives >/dev/null 2>&1; then
    if [ -L '/usr/bin/$EXE' -a -e '/usr/bin/$EXE' -a "\`readlink '/usr/bin/$EXE'\`" != '/etc/alternatives/$EXE' ]; then
        rm -f '/usr/bin/$EXE'
    fi
    update-alternatives --install '/usr/bin/$EXE' '$EXE' '/opt/$NAME/$EXE' 100 || ln -sf '/opt/$NAME/$EXE' '/usr/bin/$EXE'
else
    ln -sf '/opt/$NAME/$EXE' '/usr/bin/$EXE'
fi
if hash update-desktop-database 2>/dev/null; then
    update-desktop-database /usr/share/applications || true
fi
exit 0
POST
cat > "$ROOT/DEBIAN/postrm" <<POST
#!/bin/bash
if [ "\$1" = remove ] || [ "\$1" = purge ]; then
  if type update-alternatives >/dev/null 2>&1; then
      update-alternatives --remove '$EXE' '/opt/$NAME/$EXE' || true
  else
      rm -f '/usr/bin/$EXE'
  fi
fi
exit 0
POST
chmod 0755 "$ROOT/DEBIAN/postinst" "$ROOT/DEBIAN/postrm"
mkdir -p "$OUT"
dpkg-deb -Zxz -z6 --root-owner-group --build "$ROOT" "$OUT/$FILE" >/dev/null
node "$REPO/native-v2/package/feed.mjs" yml "$OUT/$FILE" "$OUT/$FEED" "$VERSION"
echo "$OUT/$FILE"
