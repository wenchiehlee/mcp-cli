#!/bin/bash
# Install script for mcp-cli
# Usage: curl -fsSL https://raw.githubusercontent.com/philschmid/mcp-cli/main/install.sh | bash

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Cleanup on exit
TMP_FILE=""
TMP_CHECKSUM=""
cleanup() {
    if [ -n "$TMP_FILE" ] && [ -f "$TMP_FILE" ]; then
        rm -f "$TMP_FILE"
    fi
    if [ -n "$TMP_CHECKSUM" ] && [ -f "$TMP_CHECKSUM" ]; then
        rm -f "$TMP_CHECKSUM"
    fi
}
trap cleanup EXIT

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)
        case "$ARCH" in
            x86_64) BINARY="mcp-cli-linux-x64" ;;
            aarch64) BINARY="mcp-cli-linux-arm64" ;;
            *) echo -e "${RED}Unsupported architecture: $ARCH${NC}"; exit 1 ;;
        esac
        ;;
    darwin)
        case "$ARCH" in
            x86_64) BINARY="mcp-cli-darwin-x64" ;;
            arm64) BINARY="mcp-cli-darwin-arm64" ;;
            *) echo -e "${RED}Unsupported architecture: $ARCH${NC}"; exit 1 ;;
        esac
        ;;
    *)
        echo -e "${RED}Unsupported OS: $OS${NC}"
        exit 1
        ;;
esac

# Installation directory
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
GITHUB_REPO="philschmid/mcp-cli"

echo -e "${GREEN}Installing mcp-cli...${NC}"
echo "Detected: $OS/$ARCH"
echo "Binary: $BINARY"

# Check for existing installation (non-interactive mode - just warn)
if command -v mcp-cli &> /dev/null; then
    EXISTING_PATH=$(which mcp-cli)
    EXISTING_VERSION=$(mcp-cli --version 2>/dev/null || echo "unknown")
    echo ""
    echo -e "${YELLOW}Note: Overwriting existing mcp-cli${NC}"
    echo "  Location: $EXISTING_PATH"
    echo "  Version: $EXISTING_VERSION"
    echo ""
fi

# Get latest release URL
DOWNLOAD_URL="https://github.com/$GITHUB_REPO/releases/latest/download/$BINARY"
CHECKSUM_URL="https://github.com/$GITHUB_REPO/releases/latest/download/checksums.txt"

# Download binary
echo "Downloading from $DOWNLOAD_URL..."
TMP_FILE=$(mktemp)
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"; then
    echo -e "${RED}Failed to download binary. Check if releases exist at:${NC}"
    echo "  https://github.com/$GITHUB_REPO/releases"
    exit 1
fi

# Verify checksum (if available)
TMP_CHECKSUM=$(mktemp)
echo "Verifying checksum..."
if curl -fsSL "$CHECKSUM_URL" -o "$TMP_CHECKSUM" 2>/dev/null; then
    # Extract checksum for our binary
    EXPECTED_CHECKSUM=$(grep "$BINARY" "$TMP_CHECKSUM" | awk '{print $1}')
    if [ -n "$EXPECTED_CHECKSUM" ]; then
        # Calculate actual checksum
        if command -v sha256sum &> /dev/null; then
            ACTUAL_CHECKSUM=$(sha256sum "$TMP_FILE" | awk '{print $1}')
        elif command -v shasum &> /dev/null; then
            ACTUAL_CHECKSUM=$(shasum -a 256 "$TMP_FILE" | awk '{print $1}')
        else
            echo -e "${YELLOW}Warning: Could not verify checksum (no sha256sum/shasum found)${NC}"
            ACTUAL_CHECKSUM=""
        fi

        if [ -n "$ACTUAL_CHECKSUM" ]; then
            if [ "$EXPECTED_CHECKSUM" != "$ACTUAL_CHECKSUM" ]; then
                echo -e "${RED}Checksum verification failed!${NC}"
                echo "Expected: $EXPECTED_CHECKSUM"
                echo "Actual: $ACTUAL_CHECKSUM"
                exit 1
            fi
            echo -e "${GREEN}Checksum verified.${NC}"
        fi
    else
        echo -e "${YELLOW}Warning: No checksum found for $BINARY${NC}"
    fi
else
    echo -e "${YELLOW}Warning: Could not download checksums file (skipping verification)${NC}"
fi

# Make executable
chmod +x "$TMP_FILE"

# Install (may require sudo)
echo "Installing to $INSTALL_DIR/mcp-cli..."
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_FILE" "$INSTALL_DIR/mcp-cli"
else
    echo -e "${YELLOW}Requires sudo to install to $INSTALL_DIR${NC}"
    sudo mv "$TMP_FILE" "$INSTALL_DIR/mcp-cli"
fi
TMP_FILE=""  # Clear so cleanup doesn't try to delete

# Verify installation
if command -v mcp-cli &> /dev/null; then
    echo ""
    echo -e "${GREEN}✓ mcp-cli installed successfully!${NC}"
    echo ""
    mcp-cli --version
    echo ""
    echo "Get started:"
    echo "  mcp-cli --help"
else
    echo -e "${YELLOW}Installation complete. Add $INSTALL_DIR to your PATH if needed.${NC}"
fi
