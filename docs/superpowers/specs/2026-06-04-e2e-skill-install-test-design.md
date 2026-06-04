# E2E Skill Installation Test Design

**Goal:** Verify that the cli-box skill installation process (both npm postinstall and install.sh paths) works correctly in an isolated tmp directory, and that after installation Claude can discover the skill and cli-box can execute.

**Approach:** Shell-based E2E test script that overrides HOME to a tmp directory, runs the actual installation scripts, and verifies the results at two levels: file-level (CI) and functional-level (local).

---

## Architecture

```
tests/e2e-skill-install.sh
│
├── Test 1: npm postinstall.mjs
│   ├── Create tmp HOME with mock project structure
│   ├── Symlink platform package into mock node_modules
│   ├── Run postinstall.mjs with HOME override
│   ├── Verify: symlinks created, SKILL.md installed
│   ├── Verify: SKILL.md format (frontmatter parseable by Claude)
│   └── Cleanup
│
├── Test 2: install.sh (GitHub Release path)
│   ├── Build local cli-box-skill.tar.gz from current binaries
│   ├── Create modified install.sh pointing to local tarball
│   ├── Run install.sh with HOME override
│   ├── Verify: binaries installed and executable
│   ├── Verify: SKILL.md installed
│   └── Cleanup
│
├── Test 3: Post-install verification
│   ├── Verify cli-box binary responds to --help
│   ├── Verify SKILL.md frontmatter is valid YAML with name + description
│   └── (Local only) Verify cli-box start zsh launches successfully
│
└── Summary + exit code
```

## Test 1: npm postinstall.mjs

### Setup

```bash
TMP_HOME=$(mktemp -d)
MOCK_PROJECT="$TMP_HOME/mock-project"

# Create mock project with platform package symlink
mkdir -p "$MOCK_PROJECT/node_modules"
ln -s "$REPO_ROOT/packages/cli-box-darwin-arm64" \
      "$MOCK_PROJECT/node_modules/cli-box-darwin-arm64"

# Create .claude/skills/ dir (simulating Claude Code being installed)
mkdir -p "$TMP_HOME/.claude/skills"
```

### Execution

```bash
HOME="$TMP_HOME" node "$REPO_ROOT/packages/cli-box-skill/postinstall.mjs"
```

### Verification

| Check | What it proves |
|-------|---------------|
| `$HOME/.cli-box/bin/cli-box` is a symlink | postinstall created the symlink |
| Symlink target exists and is executable | Platform package binary is valid |
| `$HOME/.claude/skills/cli-box/SKILL.md` exists | Skill installed to Claude directory |
| SKILL.md has `---\nname: cli-box` frontmatter | Claude can parse and load the skill |
| `$HOME/.config/opencode/skills/cli-box/SKILL.md` exists (if dir exists) | OpenCode skill installed |

## Test 2: install.sh (GitHub Release path)

### Setup

```bash
TMP_HOME=$(mktemp -d)
TMP_DIR=$(mktemp -d)

# Build local tarball from current repo state
SKILL_PKG_DIR="$TMP_DIR/skill-pkg"
mkdir -p "$SKILL_PKG_DIR/bin"
cp "$REPO_ROOT/packages/cli-box-skill/skill/SKILL.md" "$SKILL_PKG_DIR/"
cp "$REPO_ROOT/target/release/cli-box" "$SKILL_PKG_DIR/bin/" 2>/dev/null || \
  cp "$REPO_ROOT/target/debug/cli-box" "$SKILL_PKG_DIR/bin/"
cp "$REPO_ROOT/target/release/cli-box-daemon" "$SKILL_PKG_DIR/bin/" 2>/dev/null || \
  cp "$REPO_ROOT/target/debug/cli-box-daemon" "$SKILL_PKG_DIR/bin/"
chmod +x "$SKILL_PKG_DIR/bin/"*
cd "$SKILL_PKG_DIR" && tar czf "$TMP_DIR/cli-box-skill.tar.gz" . && cd "$REPO_ROOT"

# Create modified install.sh that uses local tarball
cp "$REPO_ROOT/packages/cli-box-skill/skill/install.sh" "$TMP_DIR/install-local.sh"
# Replace download URL with file:// path
sed -i '' "s|DOWNLOAD_URL=.*|DOWNLOAD_URL=\"file://$TMP_DIR/cli-box-skill.tar.gz\"|" "$TMP_DIR/install-local.sh"
# Replace version fetch (skip GitHub API call)
sed -i '' 's|VERSION=.*|VERSION="local"|' "$TMP_DIR/install-local.sh"
sed -i '' '/Fetching latest release version/,/fi/d' "$TMP_DIR/install-local.sh"
```

### Execution

```bash
HOME="$TMP_HOME" bash "$TMP_DIR/install-local.sh"
```

### Verification

| Check | What it proves |
|-------|---------------|
| `$HOME/.cli-box/bin/cli-box` exists and is executable | Binary installed correctly |
| `$HOME/.cli-box/bin/cli-box-daemon` exists and is executable | Daemon binary installed |
| `$HOME/.claude/skills/cli-box/SKILL.md` exists | Skill installed to Claude directory |
| SKILL.md has valid frontmatter | Claude can parse and load the skill |

## Test 3: Post-install verification

### File-level (CI)

```bash
# Verify cli-box binary works
"$HOME/.cli-box/bin/cli-box" --help

# Verify SKILL.md frontmatter is valid
head -5 "$HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^---$"
head -10 "$HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^name: cli-box"
head -10 "$HOME/.claude/skills/cli-box/SKILL.md" | grep -q "^description:"
```

### Functional-level (local only, skipped in CI)

```bash
# Only run on macOS with permissions
if [ "$(uname)" = "Darwin" ] && [ -z "${CI:-}" ]; then
  # Start a zsh sandbox
  SANDBOX_ID=$("$HOME/.cli-box/bin/cli-box" start zsh 2>&1 | grep -o '[a-f0-9]\{6\}')
  sleep 3

  # Verify sandbox is running
  "$HOME/.cli-box/bin/cli-box" list | grep -q "$SANDBOX_ID"

  # Cleanup
  "$HOME/.cli-box/bin/cli-box" close "$SANDBOX_ID"
fi
```

## Integration with test.sh

Add a new section to `test.sh`:

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

This section runs after the existing tests (Rust, frontend, Playwright) and before the summary.

## Cleanup Strategy

Every test uses `trap 'rm -rf "$TMP_HOME" "$TMP_DIR"' EXIT` to ensure cleanup even on failure. No tmp directories are left behind.

## CI Considerations

- Tests run on macOS CI runners (same as existing tests)
- The functional-level test (cli-box start) is skipped in CI via `[ -z "${CI:-}" ]`
- Tests require `node` to be available (already present in CI)
- Tests require built binaries (`cargo build` must have run before)
