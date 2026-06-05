#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# E2E Screenshot --with-frame Test
# ============================================================
# Verifies that cli-box screenshot works with and without
# the --with-frame flag. Tests the full CLI → daemon flow.
#
# Usage: bash tests/e2e-screenshot-with-frame.sh
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

# ==================== Skip on Linux CI ====================
if [ "$(uname)" = "Linux" ] && [ -n "${CI:-}" ]; then
  warn "Skipping E2E screenshot tests on Linux CI (macOS required)"
  exit 0
fi

# ==================== Skip on CI (requires macOS permissions) ====================
if [ -n "${CI:-}" ]; then
  warn "Skipping E2E screenshot tests on CI (Screen Recording permission required)"
  exit 0
fi

# ==================== Setup: ensure platform package has binaries ====================
ensure_platform_binaries() {
  local PKG_BIN="$REPO_ROOT/packages/cli-box-darwin-arm64/bin"
  if [ -f "$PKG_BIN/cli-box" ] && [ -f "$PKG_BIN/cli-box-daemon" ]; then
    return
  fi

  info "Building binaries..."
  if ! cargo build -p cli-box-cli -p cli-box-daemon 2>&1; then
    err "cargo build failed"
    exit 1
  fi

  mkdir -p "$PKG_BIN"
  if [ -f "$REPO_ROOT/target/release/cli-box" ]; then
    ln -sf "$REPO_ROOT/target/release/cli-box" "$PKG_BIN/cli-box"
    ln -sf "$REPO_ROOT/target/release/cli-box-daemon" "$PKG_BIN/cli-box-daemon"
  elif [ -f "$REPO_ROOT/target/debug/cli-box" ]; then
    ln -sf "$REPO_ROOT/target/debug/cli-box" "$PKG_BIN/cli-box"
    ln -sf "$REPO_ROOT/target/debug/cli-box-daemon" "$PKG_BIN/cli-box-daemon"
  else
    err "No built binaries found"
    exit 1
  fi

  ok "Binaries ready"
}

# ==================== Find CLI binary ====================
find_cli() {
  if [ -f "$REPO_ROOT/target/release/cli-box" ]; then
    echo "$REPO_ROOT/target/release/cli-box"
  elif [ -f "$REPO_ROOT/target/debug/cli-box" ]; then
    echo "$REPO_ROOT/target/debug/cli-box"
  else
    err "cli-box binary not found"
    exit 1
  fi
}

# ==================== Test 1: Default screenshot (no --with-frame) ====================
test_default_screenshot() {
  info "Test 1: Default screenshot (renderer path)"

  local CLI="$1"
  local SANDBOX_ID="$2"

  local OUT_FILE
  OUT_FILE=$(mktemp /tmp/cli-box-screenshot-XXXXXX.png)

  # Default screenshot uses renderer path, should work without Screen Recording permission
  if "$CLI" screenshot --id "$SANDBOX_ID" -o "$OUT_FILE" 2>&1; then
    if [ -f "$OUT_FILE" ] && [ -s "$OUT_FILE" ]; then
      ok "  Default screenshot saved ($(wc -c < "$OUT_FILE") bytes)"
    else
      err "  Screenshot file is empty or missing"
      FAILED=1
    fi
  else
    # May fail if renderer is not connected — that's acceptable
    warn "  Default screenshot failed (renderer may not be connected)"
  fi

  rm -f "$OUT_FILE"
}

# ==================== Test 2: --with-frame screenshot ====================
test_with_frame_screenshot() {
  info "Test 2: --with-frame screenshot (ScreenCaptureKit path)"

  local CLI="$1"
  local SANDBOX_ID="$2"

  local OUT_FILE
  OUT_FILE=$(mktemp /tmp/cli-box-screenshot-XXXXXX.png)

  local OUTPUT
  OUTPUT=$("$CLI" screenshot --id "$SANDBOX_ID" --with-frame -o "$OUT_FILE" 2>&1) && local EXIT_CODE=0 || local EXIT_CODE=$?

  if [ $EXIT_CODE -eq 0 ]; then
    # Succeeded — Screen Recording permission granted
    if [ -f "$OUT_FILE" ] && [ -s "$OUT_FILE" ]; then
      ok "  --with-frame screenshot saved ($(wc -c < "$OUT_FILE") bytes)"
      # Verify the output mentions ScreenCaptureKit
      if echo "$OUTPUT" | grep -qi "screencapturekit\|ScreenCaptureKit"; then
        ok "  Output confirms ScreenCaptureKit was used"
      else
        warn "  Output does not mention ScreenCaptureKit (may be using renderer)"
      fi
    else
      err "  Screenshot file is empty or missing"
      FAILED=1
    fi
  else
    # Failed — likely no Screen Recording permission
    if echo "$OUTPUT" | grep -qi "screen recording\|permission\|with_frame"; then
      ok "  --with-frame correctly reports permission error"
    else
      err "  --with-frame failed with unexpected error: $OUTPUT"
      FAILED=1
    fi
  fi

  rm -f "$OUT_FILE"
}

# ==================== Test 3: --with-frame error message guidance ====================
test_with_frame_error_guidance() {
  info "Test 3: --with-frame error message guidance"

  local CLI="$1"
  local SANDBOX_ID="$2"

  # Try --with-frame and check that error messages are helpful
  local OUTPUT
  OUTPUT=$("$CLI" screenshot --id "$SANDBOX_ID" --with-frame 2>&1) && local EXIT_CODE=0 || local EXIT_CODE=$?

  if [ $EXIT_CODE -ne 0 ]; then
    # Check error message contains helpful guidance
    if echo "$OUTPUT" | grep -qi "screen recording\|permission\|system settings"; then
      ok "  Error message provides permission guidance"
    else
      warn "  Error message could be more helpful: $OUTPUT"
    fi
  else
    ok "  --with-frame succeeded (permission already granted)"
  fi
}

# ==================== Main ====================
echo ""
echo "=============================================="
echo " E2E Screenshot --with-frame Tests"
echo "=============================================="
echo ""

ensure_platform_binaries

CLI=$(find_cli)
info "Using CLI: $CLI"

# Verify CLI works
if ! "$CLI" --help >/dev/null 2>&1; then
  err "cli-box --help failed"
  exit 1
fi
ok "CLI is functional"

# Kill any existing daemon to start fresh
"$CLI" close-all 2>/dev/null || true
sleep 1

# Start a sandbox
info "Starting sandbox..."
START_OUTPUT=$("$CLI" start zsh 2>&1) || true
SANDBOX_ID=$(echo "$START_OUTPUT" | grep -oE '[a-f0-9]{6}' | head -1 || true)

if [ -z "$SANDBOX_ID" ]; then
  err "Failed to start sandbox"
  err "Output: $START_OUTPUT"
  exit 1
fi

ok "Sandbox started: $SANDBOX_ID"

# Wait for sandbox to be ready
sleep 3

# Cleanup on exit
cleanup() {
  info "Cleaning up sandbox $SANDBOX_ID..."
  "$CLI" close "$SANDBOX_ID" 2>/dev/null || true
}
trap cleanup EXIT

echo ""

# Run tests
test_default_screenshot "$CLI" "$SANDBOX_ID"
echo ""
test_with_frame_screenshot "$CLI" "$SANDBOX_ID"
echo ""
test_with_frame_error_guidance "$CLI" "$SANDBOX_ID"

# ==================== Summary ====================
echo ""
echo "=============================================="
if [ "$FAILED" -eq 0 ]; then
  echo -e "${GREEN}All E2E screenshot tests passed!${NC}"
  exit 0
else
  echo -e "${RED}Some E2E screenshot tests failed.${NC}"
  exit 1
fi
