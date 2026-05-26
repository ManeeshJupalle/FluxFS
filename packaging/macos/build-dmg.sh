#!/usr/bin/env bash
# Build a macOS .dmg with FluxFS binaries and a Setup helper.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"

echo "Building FluxFS ${VERSION} release binaries..."
cargo build --release --bins

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

APP_DIR="$STAGING/FluxFS"
mkdir -p "$APP_DIR"

cp target/release/flux target/release/fluxfs-tray "$APP_DIR/"
ln -sf flux "$APP_DIR/fluxfs"

cat > "$APP_DIR/Setup.command" <<'EOF'
#!/bin/bash
cd "$(dirname "$0")"
./flux setup
EOF
chmod +x "$APP_DIR/Setup.command"

ln -sf /Applications "$STAGING/Applications"

mkdir -p dist
DMG="dist/FluxFS-${VERSION}-macos-x86_64.dmg"
hdiutil create -volname "FluxFS ${VERSION}" -srcfolder "$STAGING" -ov -format UDZO "$DMG"
echo "Created $DMG"
echo "After copying to Applications, double-click Setup.command or run: flux setup"
