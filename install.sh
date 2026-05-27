#!/bin/bash
set -euo pipefail

REPO="vovasilenko/copycara"
VERSION="${1:-latest}"
BIN_NAME="copycara"
INSTALL_DIR="${COPYCARA_INSTALL_DIR:-/usr/local/bin}"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS-$ARCH" in
    linux-x86_64)   BINARY="$BIN_NAME-linux-x86_64" ;;
    linux-aarch64)  BINARY="$BIN_NAME-linux-arm64" ;;
    darwin-x86_64)  BINARY="$BIN_NAME-macos-x86_64" ;;
    darwin-arm64|darwin-aarch64)
                    BINARY="$BIN_NAME-macos-arm64" ;;
    *)
        echo "Unsupported platform: $OS / $ARCH"
        echo "For Rust users: cargo install --git https://github.com/$REPO.git"
        exit 1
        ;;
esac

if [ "$VERSION" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$BINARY"
else
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$BINARY"
fi

echo "Installing Copycara $VERSION for $OS-$ARCH..."
echo "  Downloading $DOWNLOAD_URL"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$BIN_NAME" || {
    echo "Download failed. If you have Rust installed, try:"
    echo "  cargo install --git https://github.com/$REPO.git"
    exit 1
}

chmod +x "$TMP_DIR/$BIN_NAME"

# Install (may require sudo)
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
else
    sudo mv "$TMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
fi

echo ""
echo "Copycara installed to $INSTALL_DIR/$BIN_NAME"
echo ""
$INSTALL_DIR/$BIN_NAME --version 2>/dev/null || echo "Installation complete."
echo ""
echo "Next steps:"
echo "  cd /path/to/your/project"
echo "  copycara init"
