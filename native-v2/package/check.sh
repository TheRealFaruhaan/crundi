#!/bin/bash
# The packaging stream's check (package/CONTRACT.md): build the .debs from
# the release build and run the Docker upgrade test from the latest Electron
# release, for both editions. On the owner's PC: it takes the shared build
# lock for the whole run, builds nothing of the client (the release binary is
# the one in the shared target folder), and installs nothing on the host.
#
#   native-v2/package/check.sh [full|client|both]   (default both)
#
#   CRUNDI_NATIVE_BIN   the client's release folder (default ~/.cache/crundi-native-target/release)
#   CEF_PATH            CEF (default ~/.local/share/cef/cef_linux_x86_64)
#   ELECTRON_REF        the Electron release (default ~/.cache/crundi-electron-ref)
#   PACK_OUT            work and output folder (default ~/.cache/crundi-pack)
set -euo pipefail
if [ -z "${CRUNDI_BUILD_LOCKED:-}" ]; then
  exec env CRUNDI_BUILD_LOCKED=1 flock "$HOME/.cache/crundi-build.lock" timeout 60m "$0" "$@"
fi
WHICH=${1:-both}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN=${CRUNDI_NATIVE_BIN:-$HOME/.cache/crundi-native-target/release}
CEF=${CEF_PATH:-$HOME/.local/share/cef/cef_linux_x86_64}
REF=${ELECTRON_REF:-$HOME/.cache/crundi-electron-ref}
OUT=${PACK_OUT:-$HOME/.cache/crundi-pack}
VERSION=$(node -p "require('$REPO/package.json').version")
mkdir -p "$OUT"
t0=$(date +%s); step() { echo; echo "== $1 ($(( $(date +%s) - t0 )) s)"; }
DOCKER_AS_ME=(--rm --user "$(id -u):$(id -g)" -e HOME=/tmp/home)

step "Build tools image"
docker image inspect crundi-pack-build >/dev/null 2>&1 || docker build -q -t crundi-pack-build -f "$REPO/native-v2/package/linux/Dockerfile.build" "$REPO/native-v2/package/linux"

step "Node 22"
NODE_TAR=$(ls "$REF"/node-v22.*-linux-x64.tar.xz | tail -1)
NODE_DIR="$OUT/$(basename "$NODE_TAR" .tar.xz)"
[ -x "$NODE_DIR/bin/node" ] || tar -C "$OUT" -xJf "$NODE_TAR"
"$NODE_DIR/bin/node" -v

step "Worker (Ubuntu 24.04, the PC's Rust toolchain)"
mkdir -p "$OUT/worker-target"
docker run "${DOCKER_AS_ME[@]}" -v "$REPO/worker:/src/worker:ro" -v "$OUT/worker-target:/target" \
  -v "$HOME/.rustup:/rustup:ro" -v "$HOME/.cargo:/cargo" \
  -e RUSTUP_HOME=/rustup -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target -e PATH=/cargo/bin:/usr/bin:/bin \
  -e CRUNDI_WORKER_VERSION="$VERSION" crundi-pack-build \
  cargo build --release --quiet --manifest-path /src/worker/Cargo.toml
WORKER="$OUT/worker-target/release/crundi-worker"
ls -la "$WORKER"

step "Server node_modules (built by the bundled Node, Ubuntu 24.04)"
HASH=$(cat "$REPO/package-lock.json" "$NODE_TAR" | sha256sum | cut -c1-16)
MOD="$OUT/modules"
if [ "$(cat "$MOD/.built" 2>/dev/null)" != "$HASH" ]; then
  rm -rf "$MOD" && mkdir -p "$MOD" "$OUT/npm-cache"
  cp "$REPO/package.json" "$REPO/package-lock.json" "$MOD/"
  docker run "${DOCKER_AS_ME[@]}" -v "$MOD:/m" -v "$NODE_DIR:/node:ro" -v "$OUT/npm-cache:/tmp/home/.npm" -w /m \
    -e PATH=/node/bin:/usr/bin:/bin crundi-pack-build npm ci --omit=dev --no-audit --no-fund --loglevel=error
  echo "$HASH" > "$MOD/.built"
fi

DIST="$OUT/dist"; rm -rf "$DIST"; mkdir -p "$DIST"
build() {
  local ed=$1
  step "Stage and .deb ($ed)"
  node "$REPO/native-v2/package/stage.mjs" --out "$OUT/stage-$ed" --edition "$ed" --bin "$BIN" --cef "$CEF" \
    --worker "$WORKER" --node "$NODE_DIR" --modules "$MOD/node_modules"
  "$REPO/native-v2/package/linux/build-deb.sh" "$ed" "$OUT/stage-$ed" "$DIST" "$VERSION"
}
upgrade() {
  local ed=$1 old new
  if [ "$ed" = client ]; then old=$(ls "$REF"/Crundi-Client-*-linux-x64.deb | tail -1); new="$DIST/Crundi-Client-$VERSION-linux-x64.deb"
  else old=$(ls "$REF"/Crundi-[0-9]*-linux-x64.deb | tail -1); new="$DIST/Crundi-$VERSION-linux-x64.deb"; fi
  step "Docker upgrade test ($ed): $(basename "$old") -> $(basename "$new")"
  docker run --rm --user root -v "$old:/pkgs/old.deb:ro" -v "$new:/pkgs/new.deb:ro" \
    -v "$REPO/native-v2/package:/package:ro" -v "$NODE_TAR:/package-node.tar.xz:ro" \
    "$IMAGE" bash /package/upgrade-test/linux.sh /pkgs/old.deb /pkgs/new.deb "$ed" "$VERSION"
}
# The test runs on Ubuntu 24.04 (the oldest the package supports) unless the
# client binary needs a newer glibc (a release build made on a newer PC):
# then on the first Ubuntu that has it. CI builds on 24.04 and tests there.
NEED=$(objdump -T "$BIN/crundi-v2" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
IMAGE=crundi-upgrade-test
if [ "$(printf '%s\n2.39\n' "$NEED" | sort -V | tail -1)" != 2.39 ]; then
  IMAGE=crundi-upgrade-test:ubuntu-26.04
  echo "the client binary needs glibc $NEED: upgrade test on Ubuntu 26.04"
  docker image inspect "$IMAGE" >/dev/null 2>&1 || docker build -q -t "$IMAGE" --build-arg BASE=ubuntu:26.04 "$REPO/native-v2/package/upgrade-test"
fi
EDS=(full client); [ "$WHICH" = both ] || EDS=("$WHICH")
for ed in "${EDS[@]}"; do build "$ed"; done
for ed in "${EDS[@]}"; do upgrade "$ed"; done

step "Sizes"
for f in "$DIST"/*.deb "$REF"/Crundi-*-linux-x64.deb; do printf '%-42s %6d MB\n' "$(basename "$f")" "$(( $(stat -c %s "$f") / 1048576 ))"; done
for ed in "${EDS[@]}"; do printf 'installed %-8s native %5d MB\n' "$ed" "$(du -sm "$OUT/stage-$ed" | cut -f1)"; done
echo; echo "package check passed ($(( $(date +%s) - t0 )) s)"
