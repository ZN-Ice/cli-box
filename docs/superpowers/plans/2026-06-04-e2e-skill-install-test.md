# E2E Skill Installation Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a shell-based E2E test that verifies skill installation works correctly in an isolated tmp directory, with two-level verification (file-level for CI, functional-level for local).

**Architecture:** A single bash script `tests/e2e-skill-install.sh` that overrides HOME to tmp, runs both installation paths (npm postinstall.mjs and install.sh), and verifies results. Integrated into `test.sh` as a new CI gate step.

**Tech Stack:** Bash, Node.js (for postinstall.mjs), mktemp, tar

**Design doc:** `docs/superpowers/specs/2026-06-04-e2e-skill-install-test-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `tests/e2e-skill-install.sh` | E2E test script: Test 1 (npm), Test 2 (install.sh), Test 3 (verification) |
| `test.sh` | Add integration section to invoke the E2E test |

---

### Task 1: Create tests/e2e-skill-install.sh

**Files:**
- Create: `tests/e2e-skill-install.sh`

- [ ] **Step 1: Create the E2E test script**

Create `tests/e2e-skill-install.sh` with the following content:

```bash
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

# ==================== Test 1: npm postinstall.mjs ====================
test_postinstall() {
  info "Test 1: npm postinstall.mjs"

  local TMP_HOME
  TMP_HOME=$(mktemp -d)
  trap 'rm -rf "$TMP_HOME"' RETURN

  # Setup: mock project with platform package symlink
  local MOCK_PROJECT="$TMP_HOME/mock-project"
  mkdir -p "$MOCK_PROJECT/node_modules"
  ln -s "$REPO_ROOT/packages/cli-box-darwin-arm64" \
        "$MOCK_PROJECT/node_modules/cli-box-darwin-arm64"

  # Create .claude/skills/ dir (simulating Claude Code installed)
  mkdir -p "$TMP_HOME/.claude/skills"
  # Create .config/opencode/ dir (simulating OpenCode installed)
  mkdir -p "$TMP_HOME/.config/opencode"

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
```

- [ ] **Step 2: Make the script executable**

```bash
chmod +x tests/e2e-skill-install.sh
```

- [ ] **Step 3: Run the test locally to verify it works**

Run: `bash tests/e2e-skill-install.sh`

Expected output: All 3 tests pass with green checkmarks.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e-skill-install.sh
git commit -m "test: add E2E skill installation test (postinstall + install.sh)"
```

---

### Task 2: Integrate into test.sh

**Files:**
- Modify: `test.sh:113-124` (before the summary section)

- [ ] **Step 1: Add E2E skill test section to test.sh**

In `test.sh`, add the following section **before** the `# ==================== Rename Remnant Check ====================` section (line 103):

```bash
# ==================== E2E Skill Installation Tests ====================
info "Running E2E skill installation tests..."
if bash tests/e2e-skill-install.sh 2>&1; then
  ok "E2E skill installation tests passed"
else
  err "E2E skill installation tests FAILED"
  FAILED=1
fi
```

- [ ] **Step 2: Run test.sh to verify integration**

Run: `bash test.sh`

Expected: The new E2E section runs after Playwright E2E and before the rename remnant check. All tests pass.

- [ ] **Step 3: Commit**

```bash
git add test.sh
git commit -m "test: integrate E2E skill installation tests into test.sh"
```

---

### Task 3: Verify end-to-end

- [ ] **Step 1: Run the full test suite**

Run: `bash test.sh`

Expected: All sections pass including the new "E2E skill installation tests" section.

- [ ] **Step 2: Verify the E2E script cleans up tmp directories**

Run: `bash tests/e2e-skill-install.sh && ls /tmp/ | grep cli-box || echo "No tmp dirs left behind"`

Expected: "No tmp dirs left behind"

- [ ] **Step 3: Push and verify CI**

```bash
git push origin feat/release-pipeline
```

Wait for CI to pass, specifically the "统一测试 (test.sh)" check.
