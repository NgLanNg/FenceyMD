#!/usr/bin/env bash
# Build FenceyMD for Linux (.deb, .rpm, .AppImage).
# Run this ON a Linux machine (x86_64) — Tauri can't be cross-compiled from macOS.
#
#   chmod +x scripts/build-linux.sh && ./scripts/build-linux.sh
#
# Output: src-tauri/target/release/bundle/{deb,rpm,appimage}/
set -euo pipefail
cd "$(dirname "$0")/.."

echo "▶ FenceyMD — Linux build"

# 1. System libraries (Debian/Ubuntu). On Fedora use the dnf block below.
if command -v apt-get >/dev/null 2>&1; then
  echo "▶ Installing system deps (sudo apt-get)…"
  sudo apt-get update
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    build-essential curl wget file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libgtk-3-dev
elif command -v dnf >/dev/null 2>&1; then
  echo "▶ Installing system deps (sudo dnf)…"
  sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file \
    libappindicator-gtk3-devel librsvg2-devel gtk3-devel libxdo-devel
else
  echo "⚠ Unknown package manager — install WebKitGTK 4.1 + GTK3 dev packages manually."
fi

# 2. Rust toolchain.
if ! command -v cargo >/dev/null 2>&1; then
  echo "▶ Installing Rust…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  . "$HOME/.cargo/env"
fi

# 3. Node deps + build.
echo "▶ npm install…"
npm install
echo "▶ Building (vite + tauri)…"
npm run build:desktop

echo
echo "✅ Done. Bundles under:"
echo "   src-tauri/target/release/bundle/deb/*.deb"
echo "   src-tauri/target/release/bundle/appimage/*.AppImage"
echo "   src-tauri/target/release/bundle/rpm/*.rpm  (if rpm tooling present)"
