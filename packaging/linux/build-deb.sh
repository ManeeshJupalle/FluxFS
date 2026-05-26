#!/usr/bin/env bash
# Build a .deb package (requires: cargo, cargo-deb).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"

echo "Building FluxFS ${VERSION} release binaries..."
cargo build --release --bins

if ! command -v cargo-deb >/dev/null 2>&1; then
    echo "Installing cargo-deb..."
    cargo install cargo-deb --locked
fi

cargo deb --no-build

mkdir -p dist
cp -f target/debian/*.deb "dist/fluxfs_${VERSION}_amd64.deb"
echo "Created dist/fluxfs_${VERSION}_amd64.deb"
