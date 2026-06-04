#!/usr/bin/env bash
set -euo pipefail

# cli-box skill installer
# Downloads binaries from GitHub Release and sets up skill files

REPO="ZN-Ice/cli-box"
VERSION="${CLI_BOX_VERSION:-latest}"
INSTALL_DIR="$HOME/.cli-box/bin"
SKILL_CLAUDE_DIR="$HOME/.claude/skills/cli-box"
SKILL_OPENCODE_DIR="$HOME/.config/opencode/skills/cli-box"

info()  { echo "  ➜  $*"; }
ok()    { echo "  ✓  $*"; }
err()   { echo "  ✗  $*" >&2; exit 1; }

echo ""
echo "=============================================="
echo " cli-box — Skill Installer"
echo "=============================================="
echo ""

# Check prerequisites
if ! command -v curl &>/dev/null; then
    err "curl not found — please install curl"
fi

if [[ "$(uname)" != "Darwin" ]]; then
    err "cli-box only supports macOS"
fi

# Determine version
if [ "$VERSION" = "latest" ]; then
    info "Fetching latest release version..."
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"//' | sed 's/".*//')
    if [ -z "$VERSION" ]; then
        err "Failed to fetch latest version"
    fi
fi
ok "Version: $VERSION"

# Download skill tarball
info "Downloading cli-box-skill.tar.gz..."
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/cli-box-skill.tar.gz"
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMPDIR/cli-box-skill.tar.gz"; then
    err "Failed to download from $DOWNLOAD_URL"
fi
ok "Downloaded"

# Extract
info "Extracting..."
tar xzf "$TMPDIR/cli-box-skill.tar.gz" -C "$TMPDIR"

# Install binaries
info "Installing binaries to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"
cp "$TMPDIR/bin/cli-box" "$INSTALL_DIR/"
cp "$TMPDIR/bin/cli-box-daemon" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/cli-box" "$INSTALL_DIR/cli-box-daemon"
ok "Binaries installed"

# Install skill to Claude Code
if [ -d "$(dirname "$SKILL_CLAUDE_DIR")" ]; then
    info "Installing skill to Claude Code..."
    mkdir -p "$SKILL_CLAUDE_DIR"
    cp "$TMPDIR/SKILL.md" "$SKILL_CLAUDE_DIR/"
    ok "Skill installed to $SKILL_CLAUDE_DIR"
fi

# Install skill to OpenCode
if [ -d "$(dirname "$SKILL_OPENCODE_DIR")" ]; then
    info "Installing skill to OpenCode..."
    mkdir -p "$SKILL_OPENCODE_DIR"
    cp "$TMPDIR/SKILL.md" "$SKILL_OPENCODE_DIR/"
    ok "Skill installed to $SKILL_OPENCODE_DIR"
fi

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    info "Add to your PATH:"
    echo "  export PATH=\"\$HOME/.cli-box/bin:\$PATH\""
    echo ""
    info "Add to ~/.zshrc or ~/.bashrc for persistence."
fi

# Verify
echo ""
info "Verifying installation..."
if "$INSTALL_DIR/cli-box" --version &>/dev/null; then
    ok "cli-box installed: $($INSTALL_DIR/cli-box --version 2>&1 || echo 'ok')"
else
    ok "cli-box binary installed (version check requires daemon)"
fi

echo ""
echo "=============================================="
echo " Installation complete!"
echo ""
echo " Quick start:"
echo "   cli-box start claude    # Start Claude Code sandbox"
echo "   cli-box start zsh       # Start zsh sandbox"
echo "   cli-box list            # List active sandboxes"
echo ""
echo " Permissions required:"
echo "   System Settings → Privacy & Security → Accessibility"
echo "   System Settings → Privacy & Security → Screen Recording"
echo "=============================================="
echo ""
