# Screenshot --with-frame E2E Tests Design

## Goal

Add E2E test coverage for the `--with-frame` screenshot feature, covering both daemon routing logic (CI-runnable) and full CLI flow (local macOS only).

## Approach

Two test layers:

1. **Rust integration tests** — `oneshot` HTTP tests against daemon router, verifying `with_frame` query param routing. CI-runnable, no macOS dependencies.
2. **Shell E2E tests** — Build CLI binary, start daemon, run `cli-box screenshot` commands, verify output and exit codes. Local macOS only (skip CI).

## Rust Integration Tests

File: `crates/cli-box-core/tests/daemon_integration.rs`

| Test | Request | Assertion |
|------|---------|-----------|
| `screenshot_with_frame_nonexistent` | `GET /box/no-such-id/screenshot?with_frame=true` | 404 |
| `screenshot_default_nonexistent` | `GET /box/no-such-id/screenshot` | 404 |
| `screenshot_with_frame_query_parsed` | `GET /box/test-sb/screenshot?with_frame=true` | Not 400 (param parsed correctly) |

## Shell E2E Tests

File: `tests/e2e-screenshot-with-frame.sh`

Flow:
1. Build CLI binary (reuse `ensure_platform_binaries` pattern)
2. Start daemon: `cli-box start zsh`
3. Wait for sandbox ready
4. Test: `cli-box screenshot --id <id>` — verify exit code, file output
5. Test: `cli-box screenshot --id <id> --with-frame` — verify exit code, error message guidance
6. Test: `cli-box screenshot --id <id> --with-frame -o /tmp/test.png` — verify file
7. Cleanup: `cli-box close <id>`

CI skip: `uname == Darwin && CI` → skip (same pattern as `e2e-skill-install.sh`)
