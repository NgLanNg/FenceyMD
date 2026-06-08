#!/usr/bin/env bash
# Build the Linux bundles (.deb + .AppImage) from any OS that has Docker —
# including an Apple-silicon Mac. Builds inside a clean Ubuntu container so the
# host's node_modules / Rust target are never touched.
#
#   ./scripts/docker-build-linux.sh            # x86_64 (default; emulated on ARM Macs → slow)
#   ./scripts/docker-build-linux.sh arm64      # native on ARM Macs → fast, ARM Linux artifacts
#
# Output: dist-linux/  (on the host)
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="${1:-amd64}"
OUT="$(pwd)/dist-linux"
mkdir -p "$OUT"

echo "▶ Building Linux/$ARCH bundles in a container (output → dist-linux/)…"

docker run --rm --platform "linux/$ARCH" \
  -v "$(pwd)":/src:ro \
  -v "$OUT":/out \
  ubuntu:22.04 bash -euo pipefail -c '
    export DEBIAN_FRONTEND=noninteractive
    echo "▶ apt deps…"
    apt-get update -qq
    apt-get install -y -qq \
      curl ca-certificates build-essential file pkg-config xz-utils \
      libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
      libayatana-appindicator3-dev libxdo-dev libssl-dev >/dev/null

    echo "▶ node 20…"
    curl -fsSL https://deb.nodesource.com/setup_20.x | bash - >/dev/null 2>&1
    apt-get install -y -qq nodejs >/dev/null

    echo "▶ rust…"
    curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -q >/dev/null
    . "$HOME/.cargo/env"

    # AppImage tooling normally needs FUSE (absent in containers); this makes
    # linuxdeploy extract-and-run instead of mounting.
    export APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1

    echo "▶ copy source (excluding node_modules / target / dist / .git)…"
    mkdir -p /build
    tar -C /src --exclude=node_modules --exclude=dist --exclude=dist-linux \
        --exclude=src-tauri/target --exclude=.git -cf - . | tar -C /build -xf -
    cd /build

    echo "▶ npm install…"; npm install --no-audit --no-fund
    # Don'\''t let a single bundler target (e.g. AppImage) failing throw away the
    # others — collect whatever was produced regardless.
    echo "▶ tauri build…"; npm run build:desktop || echo "⚠ bundling reported errors — collecting what built"

    echo "▶ collecting artifacts…"
    cp -v src-tauri/target/release/bundle/deb/*.deb            /out/ 2>/dev/null || true
    cp -v src-tauri/target/release/bundle/appimage/*.AppImage  /out/ 2>/dev/null || true
    cp -v src-tauri/target/release/bundle/rpm/*.rpm            /out/ 2>/dev/null || true
    echo "DONE_OK"
  '

echo
echo "✅ Linux bundles in: $OUT"
ls -la "$OUT"
