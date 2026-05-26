#!/usr/bin/env bash
# Prepend direct download links to a GitHub release body (Assets stay at bottom in GitHub UI).
set -euo pipefail

TAG="${1:?usage: prepend-download-links.sh <tag>}"
REPO="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}"

VERSION="${TAG#v}"
BASE="https://github.com/${REPO}/releases/download/${TAG}"

DOWNLOAD_SECTION="$(cat <<EOF
## Download

| Platform | Recommended | Portable |
|----------|-------------|----------|
| **Windows** | [Setup.exe](${BASE}/FluxFS-${VERSION}-windows-x86_64-setup.exe) | [ZIP](${BASE}/fluxfs-windows-x86_64.zip) |
| **macOS** | [DMG](${BASE}/FluxFS-${VERSION}-macos-x86_64.dmg) | — |
| **Linux** | [\`.deb\`](${BASE}/fluxfs_${VERSION}-1_amd64.deb) | [tar.gz](${BASE}/fluxfs-linux-x86_64.tar.gz) |

Install guide: [docs/INSTALL.md](https://github.com/${REPO}/blob/main/docs/INSTALL.md)

---
EOF
)"

CURRENT="$(gh release view "$TAG" --repo "$REPO" --json body -q .body)"

if [[ "$CURRENT" == "## Download"* ]]; then
  echo "Release notes already have a Download section at the top."
  exit 0
fi

NEW_BODY="${DOWNLOAD_SECTION}

${CURRENT}"

gh release edit "$TAG" --repo "$REPO" --notes "$NEW_BODY"
echo "Prepended download links to ${TAG} release notes."
