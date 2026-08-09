#!/usr/bin/env bash
set -euo pipefail

MIN_VERSION="0.1.0"

version_ge() {
    # Returns 0 if $1 >= $2 (semver)
    printf '%s\n%s\n' "$2" "$1" | sort -V -C
}

if command -v borescope >/dev/null 2>&1; then
    current=$(borescope --version 2>/dev/null | awk '{print $2}' || echo "0.0.0")
    if version_ge "$current" "$MIN_VERSION"; then
        echo "borescope $current — OK"
        exit 0
    fi
    echo "borescope $current is below minimum $MIN_VERSION — attempting update"
fi

# Attempt to install from GitHub releases
REPO="gleicon/borescope"  # update when published
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

case "$OS" in
    linux) TRIPLE="${ARCH}-unknown-linux-musl" ;;
    darwin) TRIPLE="${ARCH}-apple-darwin" ;;
    *) echo "unsupported OS: $OS" >&2; exit 1 ;;
esac

URL="https://github.com/${REPO}/releases/latest/download/borescope-${TRIPLE}.tar.gz"
DEST="${HOME}/.local/bin/borescope"

mkdir -p "$(dirname "$DEST")"

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$URL" | tar -xz -C "$(dirname "$DEST")" borescope
elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$URL" | tar -xz -C "$(dirname "$DEST")" borescope
else
    echo "error: curl or wget required to install borescope" >&2
    exit 1
fi

chmod +x "$DEST"
echo "installed borescope to $DEST"
echo "ensure $HOME/.local/bin is in your PATH"
