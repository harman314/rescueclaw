#!/usr/bin/env bash
set -euo pipefail

VERSION="${RESCUECLAW_VERSION:-latest}"
REPO="harman314/rescueclaw"
INSTALL_DIR="/usr/local/bin"

# Detect arch
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  ARCH="amd64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$OS" in
    linux)  PLATFORM="linux" ;;
    darwin) PLATFORM="macos" ;;
    *) echo "❌ Unsupported OS: $OS"; exit 1 ;;
esac

BINARY_NAME="rescueclaw-${PLATFORM}-${ARCH}.tar.gz"

# Download
if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/$REPO/releases/latest/download/$BINARY_NAME"
else
    URL="https://github.com/$REPO/releases/download/$VERSION/$BINARY_NAME"
fi

echo "🛟 Installing RescueClaw..."
echo "   Platform: $PLATFORM/$ARCH"
echo "   Downloading from $URL"

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

if ! curl -fsSL "$URL" -o "$TMPDIR/$BINARY_NAME"; then
    echo "❌ Download failed. Check your internet connection or release availability."
    exit 1
fi

tar xzf "$TMPDIR/$BINARY_NAME" -C "$TMPDIR"

if [ -w "$INSTALL_DIR" ]; then
    mv "$TMPDIR/rescueclaw" "$INSTALL_DIR/rescueclaw"
else
    sudo mv "$TMPDIR/rescueclaw" "$INSTALL_DIR/rescueclaw"
fi

chmod +x "$INSTALL_DIR/rescueclaw"

echo "   ✓ Installed to $INSTALL_DIR/rescueclaw"
echo ""
echo "Run 'rescueclaw setup' to configure."
