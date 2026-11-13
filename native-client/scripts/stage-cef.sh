#!/usr/bin/env bash
# Put CEF's runtime files next to the built binary (libcef.so, its resources and
# locales), as cef-rs's bundle-cef-app does. Symlinks: CEF is 1.5 GB.
#   CEF_PATH   the unpacked CEF (Release + Resources flattened, as cef-rs lays it out)
#   $1         the binary's folder (default target/debug)
set -euo pipefail
CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef/cef_linux_x64}"
OUT="${1:-$(dirname "$0")/../target/debug}"
for f in libcef.so libvk_swiftshader.so libvulkan.so.1 vk_swiftshader_icd.json icudtl.dat v8_context_snapshot.bin \
         chrome_100_percent.pak chrome_200_percent.pak resources.pak locales; do
  [ -e "$CEF_PATH/$f" ] && ln -sfn "$CEF_PATH/$f" "$OUT/$f"
done
echo "CEF staged in $OUT"
