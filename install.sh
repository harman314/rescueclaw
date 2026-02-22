#!/usr/bin/env bash
set -euo pipefail

VERSION="${RESCUECLAW_VERSION:-latest}"
REPO="harman314/rescueclaw"
INSTALL_DIR="/usr/local/bin"

# Detect arch
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  ARCH="amd64" ;;
    aarch64) ARCH="arm64" ;;
    *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
if [ "$OS" != "linux" ]; then
    echo "❌ RescueClaw only supports Linux. Got: $OS"
    exit 1
fi

# Download
if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/$REPO/releases/latest/download/rescueclaw-linux-$ARCH"
else
    URL="https://github.com/$REPO/releases/download/$VERSION/rescueclaw-linux-$ARCH"
fi

echo "🛟 Installing RescueClaw..."
echo "   Downloading from $URL"

if ! curl -fsSL "$URL" -o /tmp/rescueclaw; then
    echo "❌ Download failed. Check your internet connection or release availability."
    exit 1
fi

chmod +x /tmp/rescueclaw
sudo mv /tmp/rescueclaw "$INSTALL_DIR/rescueclaw"

echo "   ✓ Installed to $INSTALL_DIR/rescueclaw"
echo ""

# Run setup wizard
echo "Starting setup wizard..."
echo ""
rescueclaw setup
