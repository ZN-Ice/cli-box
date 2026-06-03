#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}➜${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }
err()   { echo -e "${RED}✗${NC} $*"; }
ok()    { echo -e "${GREEN}✓${NC} $*"; }

FAILED=0

# ==================== Rust Tests ====================
# Rust tests require macOS frameworks (CGEvent, AXUIElement, ScreenCaptureKit).
# Skip on Linux CI where these are unavailable.
if [ "$(uname)" = "Linux" ] && [ -n "${CI:-}" ]; then
  warn "Skipping Rust tests on Linux CI (macOS frameworks required)"
else
  info "Running Rust tests..."
  if cargo test -p cli-box-core -p cli-box-cli 2>&1; then
    ok "Rust tests passed"
  else
    err "Rust tests FAILED"
    FAILED=1
  fi
fi

# ==================== Rust Clippy ====================
if [ "$(uname)" = "Linux" ] && [ -n "${CI:-}" ]; then
  warn "Skipping Rust clippy on Linux CI (macOS frameworks required)"
else
  info "Running Rust clippy..."
  if cargo clippy --all-targets -- -D warnings 2>&1; then
    ok "Rust clippy passed"
  else
    err "Rust clippy FAILED"
    FAILED=1
  fi
fi

# ==================== Rust Format Check ====================
if [ "$(uname)" = "Linux" ] && [ -n "${CI:-}" ]; then
  warn "Skipping Rust fmt on Linux CI (handled by separate CI job)"
else
  info "Running cargo fmt check..."
  if cargo fmt --all -- --check 2>&1; then
    ok "Rust format check passed"
  else
    err "Rust format check FAILED — run: cargo fmt --all"
    FAILED=1
  fi
fi

# ==================== Frontend Type Check ====================
info "Running TypeScript type check..."
if (cd electron-app && pnpm typecheck) 2>&1; then
  ok "TypeScript type check passed"
else
  err "TypeScript type check FAILED"
  FAILED=1
fi

# ==================== Frontend Unit Tests ====================
info "Running frontend unit tests..."
if (cd electron-app && pnpm vitest run --reporter=verbose) 2>&1; then
  ok "Frontend unit tests passed"
else
  err "Frontend unit tests FAILED"
  FAILED=1
fi

# ==================== Playwright E2E Tests ====================
info "Checking Playwright E2E conditions..."

# Check if Playwright is installed
if ! (cd electron-app && npx playwright --version) &>/dev/null; then
  warn "Playwright not installed — installing..."
  (cd electron-app && pnpm add -D @playwright/test)
  (cd electron-app && npx playwright install chromium)
fi

# Check if display is available (needed for headed mode on Linux)
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ] && [ "$(uname)" = "Linux" ]; then
  warn "No display detected on Linux — using headless mode"
  export PLAYWRIGHT_CHROMIUM_ARGS="--no-sandbox"
fi

info "Running Playwright E2E tests..."
if (cd electron-app && npx playwright test --config e2e/playwright.config.ts) 2>&1; then
  ok "Playwright E2E tests passed"
else
  err "Playwright E2E tests FAILED"
  FAILED=1
fi

# ==================== Rename Remnant Check ====================
info "Checking for 'sandbox' remnants in user-facing strings..."
# Check specific files that were renamed for leftover "sandbox" references
# in user-visible strings (help text, error messages, log prefixes)
SANDBOX_REMNANTS=$(grep -rn '"sandbox' crates/cli-box-cli/src/ crates/cli-box-core/src/daemon/mod.rs electron-app/src/renderer/ --include='*.rs' --include='*.ts' --include='*.tsx' 2>/dev/null \
  | grep -v '//\|/\*\|\*\|#\|test\|Test\|TEST\|sandbox_state\|sandbox_config\|SandboxConfig\|SandboxState\|ManagedSandbox\|SandboxInstance\|sandbox_id\|sandbox/\|/sandbox\|sandbox-daemon\|sandbox-cli\|sandbox-core\|sandbox-electron\|SANDBOX_\|sandbox_daemon\|sandbox_cli\|sandbox_core\|Sandbox\|\.sandbox\|sandbox\.json\|instances/\|pty_store\|SandboxRegion\|SandboxInfo\|SandboxKind\|SandboxStatus' || true)
if [ -n "$SANDBOX_REMNANTS" ]; then
  echo "$SANDBOX_REMNANTS"
  warn "Found potential 'sandbox' remnants in user-facing strings (review above)"
else
  ok "No 'sandbox' remnants found in user-facing strings"
fi

# ==================== Summary ====================
echo ""
echo "============================================"
if [ "$FAILED" -eq 0 ]; then
  echo -e "${GREEN}All tests passed!${NC}"
  exit 0
else
  echo -e "${RED}Some tests failed.${NC}"
  exit 1
fi
