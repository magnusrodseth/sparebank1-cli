#!/bin/bash
# Install a prebuilt `sb1` release binary from GitHub.
#
#   curl -fsSL https://raw.githubusercontent.com/magnusrodseth/sparebank1-cli/main/install.sh | bash
#
# Installs the latest release by default. Pin a version with SB1_VERSION:
#   curl -fsSL .../install.sh | SB1_VERSION=v1.1.0 bash
# Override the install dir with SB1_INSTALL_DIR (default: ~/.local/bin).
set -euo pipefail

REPO="magnusrodseth/sparebank1-cli"
BINARY="sb1"

die() { echo "error: $*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required but not found on PATH."
command -v tar  >/dev/null 2>&1 || die "tar is required but not found on PATH."

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$OS" in
  darwin) OS="darwin" ;;
  linux) OS="linux" ;;
  *) die "unsupported OS: $OS (sb1 ships macOS and Linux binaries; build from source with 'cargo install --path .')." ;;
esac
case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="aarch64" ;;
  *) die "unsupported architecture: $ARCH." ;;
esac

ASSET="${BINARY}-${OS}-${ARCH}.tar.gz"

# Resolve the download URL WITHOUT calling the GitHub API. The
# /releases/latest/download/<asset> path is a plain redirect served by
# GitHub, so it is immune to the anonymous API rate limit (60 req/hr) that
# silently breaks API-based installers when an agent has been busy.
VERSION="${SB1_VERSION:-}"
if [ -n "$VERSION" ]; then
  URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
  LABEL="$VERSION"
else
  URL="https://github.com/$REPO/releases/latest/download/$ASSET"
  LABEL="latest"
fi

INSTALL_DIR="${SB1_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$INSTALL_DIR" || die "could not create install dir: $INSTALL_DIR"

echo "Installing $BINARY ($LABEL, $OS/$ARCH) to $INSTALL_DIR ..."

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Download to a temp file first so a failed/HTML error response never gets
# piped into tar (which would otherwise emit a cryptic "not in gzip format").
if ! curl -fsSL --retry 3 -o "$tmp/$ASSET" "$URL"; then
  die "download failed: $URL
  - Check your connection, or that a release exists for this platform.
  - Pin a known version:  SB1_VERSION=v1.1.0 bash
  - Or build from source:  cargo install --path ."
fi

tar xzf "$tmp/$ASSET" -C "$tmp" || die "failed to extract $ASSET (corrupt download?)."
[ -f "$tmp/$BINARY" ] || die "archive did not contain the '$BINARY' binary."

install -m 0755 "$tmp/$BINARY" "$INSTALL_DIR/$BINARY" \
  || die "failed to install binary to $INSTALL_DIR (permissions?)."

echo "Installed: $INSTALL_DIR/$BINARY"

# Confirm it's runnable and tell the user how to make it reachable.
if command -v "$BINARY" >/dev/null 2>&1 && [ "$(command -v "$BINARY")" = "$INSTALL_DIR/$BINARY" ]; then
  echo "Done. Run: $BINARY --help"
else
  echo
  echo "NOTE: $INSTALL_DIR is not on your PATH. Add it, e.g.:"
  echo "    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc && exec \$SHELL"
  echo "Then run: $BINARY --help"
fi
