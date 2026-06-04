#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# E2E Skill Installation Test
# ============================================================
# Verifies that cli-box skill installation works correctly
# in an isolated tmp directory. Tests both npm postinstall
# and install.sh (GitHub Release) paths.
#
# Usage: bash tests/e2e-skill-install.sh
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}➜${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }
err()   { echo -e "${RED}✗${NC} $*"; }
ok()    { echo -e "${GREEN}✓${NC} $*"; }

FAILED=0

# ==================== Setup: ensure platform package has binaries ====================
ensure_platform_binaries() {
  local PKG_BIN="$REPO_ROOT/packages/cli-box-darwin-arm64/bin"
  if [ -f "$PKG_BIN/cli-box" ] && [ -f "$PKG_BIN/cli-box-daemon" ]; then
    return
  fi

  info "Populating platform package bin/ with built binaries..."
  mkdir -p "$PKG_BIN"

  if [ -f "$REPO_ROOT/target/release/cli-box" ]; then
    ln -sf "$REPO_ROOT/target/release/cli-box" "$PKG_BIN/cli-box"
    ln -sf "$REPO_ROOT/target/release/cli-box-daemon" "$PKG_BIN/cli-box-daemon"
  elif [ -f "$REPO_ROOT/target/debug/cli-box" ]; then
    ln -sf "$REPO_ROOT/target/debug/cli-box" "$PKG_BIN/cli-box"
    ln -sf "$REPO_ROOT/target/debug/cli-box-daemon" "$PKG_BIN/cli-box-daemon"
  else
    err "No built binaries found. Run 'cargo build' first."
    exit 1
  fi

  ok "Platform package binaries linked"
}

# ==================== Test 1: npm postinstall.mjs ====================
test_postinstall() {
  info "Test 1: npm postinstall.mjs"

  local TMP_HOME
  TMP_HOME=$(mktemp -d)

  # postinstall.mjs uses createRequire(import.meta.url) which resolves from
  # its own directory. We need the platform package in its node_modules/.
  local SKILL_PKG_NM="$REPO_ROOT/packages/cli-box-skill/node_modules"
  local CREATED_NM=0
  cleanup_postinstall() {
    rm -rf "$TMP_HOME"
    if [ "$CREATED_NM" -eq 1 ]; then
      rm -rf "$SKILL_PKG_NM"
    fi
  }
  trap cleanup_postinstall RETURN

  # Create node_modules with platform package symlink next to postinstall.mjs
  if [ ! -d "$SKILL_PKG_NM/cli-box-darwin-arm64" ]; then
    mkdir -p "$SKILL_PKG_NM"
    ln -s "$REPO_ROOT/packages/cli-box-darwin-arm64" \
          "$SKILL_PKG_NM/cli-box-darwin-arm64"
    CREATED_NM=1
  fi

  # Create .claude/skills/ dir (simulating Claude Code installed)
  mkdir -p "$TMP_HOME/.claude/skills"
  # Create .config/opencode/skills/ dir (simulating OpenCode installed)
  mkdir -p "$TMP_HOME/.config/opencode/skills"

  # Run postinstall.mjs with HOME override
  info "  Running postinstall.mjs..."
  if ! HOME="$TMP_HOME" node "$REPO_ROOT/packages/cli-box-skill/postinstall.mjs" 2>&1; then
    err "  postinstall.mjs exited with non-zero status"
    FAILED=1
    return
  fi

  # Verify symlinks
  if [ -L "$TMP_HOME/.cli-box/bin/cli-box" ]; then
    ok "  cli-box symlink created"
  else
    err "  cli-box symlink NOT created"
    FAILED=1
  fi

  if [ -L "$TMP_HOME/.cli-box/bin/cli-box-daemon" ]; then
    ok "  cli-box-daemon symlink created"
  else
    err "  cli-box-daemon symlink NOT created"
    FAILED=1
  fi

  # Verify symlink targets are executable
  if [ -x "$TMP_HOME/.cli-box/bin/cli-box" ]; then
    ok "  cli-box symlink target is executable"
  else
    err "  cli-box symlink target is NOT executable"
    FAILED=1
  fi

  # Verify SKILL.md installed to Claude directory
  if [ -f "$TMP_HOME/.claude/skills/cli-box/SKILL.md" ]; then
    ok "  SKILL.md installed to .claude/skills/cli-box/"
  else
    err "  SKILL.md NOT found in .claude/skills/cli-box/"
    FAILED=1
  fi

  # Verify SKILL.md installed to OpenCode directory
  if [ -f "$TMP_HOME/.config/opencode/skills/cli-box/SKILL.md" ]; then
    ok "  SKILL.md installed to .config/opencode/skills/cli-box/"
  else
    err "  SKILL.md NOT found in .config/opencode/skills/cli-box/"
    FAILED=1
  fi

  # Verify SKILL.md frontmatter
  if head -1 "$TMP_HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^---$"; then
    ok "  SKILL.md has valid frontmatter delimiter"
  else
    err "  SKILL.md missing frontmatter delimiter"
    FAILED=1
  fi

  if head -5 "$TMP_HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^name: cli-box"; then
    ok "  SKILL.md frontmatter contains 'name: cli-box'"
  else
    err "  SKILL.md frontmatter missing 'name: cli-box'"
    FAILED=1
  fi

  if head -5 "$TMP_HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^description:"; then
    ok "  SKILL.md frontmatter contains 'description'"
  else
    err "  SKILL.md frontmatter missing 'description'"
    FAILED=1
  fi

  info "  Test 1 complete"
}

# ==================== Test 2: install.sh (GitHub Release path) ====================
test_install_sh() {
  info "Test 2: install.sh (GitHub Release path)"

  local TMP_HOME
  TMP_HOME=$(mktemp -d)
  local TMP_DIR
  TMP_DIR=$(mktemp -d)
  trap 'rm -rf "$TMP_HOME" "$TMP_DIR"' RETURN

  # Build local tarball from current repo state
  info "  Building local tarball..."
  local SKILL_PKG_DIR="$TMP_DIR/skill-pkg"
  mkdir -p "$SKILL_PKG_DIR/bin"

  cp "$REPO_ROOT/packages/cli-box-skill/skill/SKILL.md" "$SKILL_PKG_DIR/"

  # Use release binaries if available, fallback to debug
  if [ -f "$REPO_ROOT/target/release/cli-box" ]; then
    cp "$REPO_ROOT/target/release/cli-box" "$SKILL_PKG_DIR/bin/"
    cp "$REPO_ROOT/target/release/cli-box-daemon" "$SKILL_PKG_DIR/bin/"
  elif [ -f "$REPO_ROOT/target/debug/cli-box" ]; then
    cp "$REPO_ROOT/target/debug/cli-box" "$SKILL_PKG_DIR/bin/"
    cp "$REPO_ROOT/target/debug/cli-box-daemon" "$SKILL_PKG_DIR/bin/"
  else
    err "  No built binaries found. Run 'cargo build' first."
    FAILED=1
    return
  fi

  chmod +x "$SKILL_PKG_DIR/bin/"*
  (cd "$SKILL_PKG_DIR" && tar czf "$TMP_DIR/cli-box-skill.tar.gz" .)
  ok "  Local tarball built"

  # Create modified install.sh pointing to local tarball
  cp "$REPO_ROOT/packages/cli-box-skill/skill/install.sh" "$TMP_DIR/install-local.sh"

  # Replace version detection with fixed version
  sed -i '' 's/VERSION="${CLI_BOX_VERSION:-latest}"/VERSION="local"/' "$TMP_DIR/install-local.sh"

  # Replace the GitHub API version fetch block with a no-op
  sed -i '' '/Fetching latest release version/,/fi/c\
  info "Using local version"' "$TMP_DIR/install-local.sh"

  # Replace download URL with local file
  sed -i '' "s|DOWNLOAD_URL=\"https://github.com/\$REPO/releases/download/\$VERSION/cli-box-skill.tar.gz\"|DOWNLOAD_URL=\"file://$TMP_DIR/cli-box-skill.tar.gz\"|" "$TMP_DIR/install-local.sh"

  # Run install.sh with HOME override
  info "  Running install-local.sh..."
  if ! HOME="$TMP_HOME" bash "$TMP_DIR/install-local.sh" 2>&1; then
    err "  install.sh exited with non-zero status"
    FAILED=1
    return
  fi

  # Verify binaries
  if [ -f "$TMP_HOME/.cli-box/bin/cli-box" ] && [ -x "$TMP_HOME/.cli-box/bin/cli-box" ]; then
    ok "  cli-box binary installed and executable"
  else
    err "  cli-box binary NOT found or not executable"
    FAILED=1
  fi

  if [ -f "$TMP_HOME/.cli-box/bin/cli-box-daemon" ] && [ -x "$TMP_HOME/.cli-box/bin/cli-box-daemon" ]; then
    ok "  cli-box-daemon binary installed and executable"
  else
    err "  cli-box-daemon binary NOT found or not executable"
    FAILED=1
  fi

  # Verify SKILL.md
  if [ -f "$TMP_HOME/.claude/skills/cli-box/SKILL.md" ]; then
    ok "  SKILL.md installed to .claude/skills/cli-box/"
  else
    # install.sh only installs SKILL.md if .claude/ dir exists
    warn "  SKILL.md not installed (.claude/ dir may not exist in tmp HOME)"
  fi

  # Verify SKILL.md frontmatter
  if [ -f "$TMP_HOME/.claude/skills/cli-box/SKILL.md" ]; then
    if head -5 "$TMP_HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^name: cli-box"; then
      ok "  SKILL.md has valid frontmatter"
    else
      err "  SKILL.md frontmatter invalid"
      FAILED=1
    fi
  fi

  info "  Test 2 complete"
}

# ==================== Test 3: Post-install verification ====================
test_post_install_verify() {
  info "Test 3: Post-install verification"

  local TMP_HOME
  TMP_HOME=$(mktemp -d)
  local TMP_DIR
  TMP_DIR=$(mktemp -d)
  trap 'rm -rf "$TMP_HOME" "$TMP_DIR"' RETURN

  # Build local tarball (same as Test 2)
  local SKILL_PKG_DIR="$TMP_DIR/skill-pkg"
  mkdir -p "$SKILL_PKG_DIR/bin"
  cp "$REPO_ROOT/packages/cli-box-skill/skill/SKILL.md" "$SKILL_PKG_DIR/"
  if [ -f "$REPO_ROOT/target/release/cli-box" ]; then
    cp "$REPO_ROOT/target/release/cli-box" "$SKILL_PKG_DIR/bin/"
    cp "$REPO_ROOT/target/release/cli-box-daemon" "$SKILL_PKG_DIR/bin/"
  else
    cp "$REPO_ROOT/target/debug/cli-box" "$SKILL_PKG_DIR/bin/"
    cp "$REPO_ROOT/target/debug/cli-box-daemon" "$SKILL_PKG_DIR/bin/"
  fi
  chmod +x "$SKILL_PKG_DIR/bin/"*
  (cd "$SKILL_PKG_DIR" && tar czf "$TMP_DIR/cli-box-skill.tar.gz" .)

  # Install via install.sh
  cp "$REPO_ROOT/packages/cli-box-skill/skill/install.sh" "$TMP_DIR/install-local.sh"
  sed -i '' 's/VERSION="${CLI_BOX_VERSION:-latest}"/VERSION="local"/' "$TMP_DIR/install-local.sh"
  sed -i '' '/Fetching latest release version/,/fi/c\
  info "Using local version"' "$TMP_DIR/install-local.sh"
  sed -i '' "s|DOWNLOAD_URL=\"https://github.com/\$REPO/releases/download/\$VERSION/cli-box-skill.tar.gz\"|DOWNLOAD_URL=\"file://$TMP_DIR/cli-box-skill.tar.gz\"|" "$TMP_DIR/install-local.sh"

  # Create .claude/skills/ so install.sh installs SKILL.md
  mkdir -p "$TMP_HOME/.claude/skills"
  HOME="$TMP_HOME" bash "$TMP_DIR/install-local.sh" >/dev/null 2>&1

  # Verify cli-box binary responds to --help
  info "  Verifying cli-box --help..."
  if "$TMP_HOME/.cli-box/bin/cli-box" --help >/dev/null 2>&1; then
    ok "  cli-box --help works"
  else
    err "  cli-box --help failed"
    FAILED=1
  fi

  # Verify SKILL.md frontmatter is valid
  info "  Verifying SKILL.md frontmatter..."
  local SKILL_FILE="$TMP_HOME/.claude/skills/cli-box/SKILL.md"
  if [ -f "$SKILL_FILE" ]; then
    if head -1 "$SKILL_FILE" | grep -q "^---$" && \
       head -5 "$SKILL_FILE" | grep -q "^name: cli-box" && \
       head -5 "$SKILL_FILE" | grep -q "^description:"; then
      ok "  SKILL.md frontmatter valid (name + description present)"
    else
      err "  SKILL.md frontmatter invalid"
      FAILED=1
    fi
  else
    warn "  SKILL.md not found, skipping frontmatter check"
  fi

  # Functional-level test (local only, skipped in CI)
  if [ "$(uname)" = "Darwin" ] && [ -z "${CI:-}" ]; then
    info "  Running functional test (cli-box start zsh)..."
    local SANDBOX_ID
    SANDBOX_ID=$("$TMP_HOME/.cli-box/bin/cli-box" start zsh 2>&1 | grep -oE '[a-f0-9]{6}' | head -1 || true)
    if [ -n "$SANDBOX_ID" ]; then
      sleep 3
      if "$TMP_HOME/.cli-box/bin/cli-box" list 2>&1 | grep -q "$SANDBOX_ID"; then
        ok "  Sandbox $SANDBOX_ID is running"
      else
        warn "  Sandbox $SANDBOX_ID not found in list (may have exited)"
      fi
      "$TMP_HOME/.cli-box/bin/cli-box" close "$SANDBOX_ID" 2>/dev/null || true
    else
      warn "  Could not start sandbox (macOS permissions may be required)"
    fi
  else
    info "  Skipping functional test (CI or non-macOS)"
  fi

  info "  Test 3 complete"
}

# ==================== Main ====================
echo ""
echo "=============================================="
echo " E2E Skill Installation Tests"
echo "=============================================="
echo ""

ensure_platform_binaries
echo ""

test_postinstall
echo ""
test_install_sh
echo ""
test_post_install_verify

# ==================== Summary ====================
echo ""
echo "=============================================="
if [ "$FAILED" -eq 0 ]; then
  echo -e "${GREEN}All E2E skill installation tests passed!${NC}"
  exit 0
else
  echo -e "${RED}Some E2E skill installation tests failed.${NC}"
  exit 1
fi
