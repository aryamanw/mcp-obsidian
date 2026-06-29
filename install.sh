#!/usr/bin/env bash
set -euo pipefail

REPO_OWNER="aryamanw"
REPO_NAME="obsidian-mcp"
REPO="$REPO_OWNER/$REPO_NAME"

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${VERSION:-latest}"
ADD_TO_PATH=0
QUIET=0

while [ $# -gt 0 ]; do
    case "$1" in
        --install-dir) INSTALL_DIR="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --add-to-path) ADD_TO_PATH=1; shift ;;
        --quiet) QUIET=1; shift ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

info() { [ "$QUIET" -eq 0 ] && printf '\033[36m[INFO]\033[0m %s\n' "$*" || true; }
ok()   { [ "$QUIET" -eq 0 ] && printf '\033[32m[OK]\033[0m   %s\n' "$*" || true; }
err()  { printf '\033[31m[ERR]\033[0m  %s\n' "$*" >&2; exit 1; }

OS="$(uname -s)"
case "$OS" in
    Darwin) PLATFORM="apple-darwin" ;;
    Linux)  PLATFORM="unknown-linux-gnu" ;;
    *) err "Unsupported OS: $OS (this script supports macOS and Linux; use install.ps1 on Windows)" ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)  TARGET="x86_64-$PLATFORM" ;;
    arm64|aarch64) TARGET="aarch64-$PLATFORM" ;;
    *) err "Unsupported architecture: $ARCH" ;;
esac
info "Detected platform: $TARGET"

BINARY_NAME="obsidian-mcp"
BINARY_PATH="$INSTALL_DIR/$BINARY_NAME"

if [ -f "$BINARY_PATH" ]; then
    info "Already installed at $BINARY_PATH"
fi

if [ "$VERSION" = "latest" ]; then
    info "Fetching latest release..."
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/')"
    [ -n "$VERSION" ] || err "Could not determine latest version"
    info "Latest version: $VERSION"
fi

ARCHIVE_NAME="obsidian-mcp-$VERSION-$TARGET.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/v$VERSION/$ARCHIVE_NAME"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
DOWNLOAD_PATH="$TMP_DIR/$ARCHIVE_NAME"

info "Downloading $DOWNLOAD_URL ..."
curl -fsSL -o "$DOWNLOAD_PATH" "$DOWNLOAD_URL" || err "Download failed"

info "Extracting..."
mkdir -p "$INSTALL_DIR"
tar xzf "$DOWNLOAD_PATH" -C "$TMP_DIR"

EXTRACTED_BINARY="$TMP_DIR/$BINARY_NAME"
[ -f "$EXTRACTED_BINARY" ] || err "Binary not found after extraction"

mv "$EXTRACTED_BINARY" "$BINARY_PATH"
chmod +x "$BINARY_PATH"

if command -v xattr >/dev/null 2>&1; then
    xattr -d com.apple.quarantine "$BINARY_PATH" 2>/dev/null || true
fi

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        info "$INSTALL_DIR already in PATH"
        ;;
    *)
        if [ "$ADD_TO_PATH" -eq 1 ] || { [ "$QUIET" -eq 0 ] && [ -t 0 ]; }; then
            if [ "$ADD_TO_PATH" -eq 0 ]; then
                read -r -p "Add $INSTALL_DIR to your PATH? (Y/n) " choice
                case "$choice" in
                    n|N) ADD_TO_PATH=0 ;;
                    *) ADD_TO_PATH=1 ;;
                esac
            fi
            if [ "$ADD_TO_PATH" -eq 1 ]; then
                SHELL_RC=""
                case "$SHELL" in
                    */zsh)  SHELL_RC="$HOME/.zshrc" ;;
                    */bash) SHELL_RC="$HOME/.bashrc" ;;
                esac
                if [ -n "$SHELL_RC" ]; then
                    echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_RC"
                    ok "Added $INSTALL_DIR to PATH in $SHELL_RC (restart your shell to apply)"
                else
                    info "Add this to your shell profile: export PATH=\"$INSTALL_DIR:\$PATH\""
                fi
            fi
        fi
        ;;
esac

ok "Installation complete!"
info ""
info "Next steps:"
info "  1. Set your vault path:"
info "     export OBSIDIAN_VAULT=\"/path/to/your/vault\""
info ""
info "  2. Run the server:"
info "     obsidian-mcp"
info ""
info "  3. Or configure your MCP client (Claude, VS Code, etc.) to use:"
info "     $BINARY_PATH"
