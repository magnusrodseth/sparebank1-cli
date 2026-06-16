#!/bin/bash
# Install the latest `sb1` release binary from GitHub.
#   curl -fsSL https://raw.githubusercontent.com/magnusrodseth/sparebank1-cli/main/install.sh | bash
set -euo pipefail

REPO="magnusrodseth/sparebank1-cli"
BINARY="sb1"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$OS" in
  darwin) OS="darwin" ;;
  linux) OS="linux" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac
case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

LATEST=$(curl -sf "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
if [ -z "$LATEST" ]; then
  echo "Failed to find a release for $REPO." >&2
  exit 1
fi

INSTALL_DIR="${SB1_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$INSTALL_DIR"
ASSET="${BINARY}-${OS}-${ARCH}.tar.gz"
URL="https://github.com/$REPO/releases/download/$LATEST/$ASSET"

echo "Installing $BINARY $LATEST ($OS/$ARCH) to $INSTALL_DIR ..."
curl -sfL "$URL" | tar xz -C "$INSTALL_DIR"
chmod +x "$INSTALL_DIR/$BINARY"

echo "Done. Ensure $INSTALL_DIR is on your PATH, then run: $BINARY --help"
