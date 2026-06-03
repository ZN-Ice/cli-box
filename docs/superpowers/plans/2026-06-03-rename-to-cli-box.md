# Rename system-test-sandbox to cli-box Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename all occurrences of "system-test-sandbox" / "System Test Sandbox" to "cli-box" / "CLI Box" across source code, config, documentation, and build files.

**Architecture:** Pure string-literal rename — no Cargo package names, crate directories, or binary target names change. The rename covers two variants: hyphenated (`system-test-sandbox` → `cli-box`) and spaced (`System Test Sandbox` → `CLI Box`), plus the appId (`com.system-test-sandbox` → `com.cli-box`).

**Tech Stack:** Rust, TypeScript, Electron, GitHub Actions, Markdown

---

## Rename Mapping

| Old | New | Where |
|-----|-----|-------|
| `system-test-sandbox` | `cli-box` | binary paths, process names, appId suffix, URLs, comments |
| `System Test Sandbox` | `CLI Box` | app productName, window title, display names, pkill targets |
| `com.system-test-sandbox` | `com.cli-box` | Electron appId, bundle identifier |
| `System Test Sandbox.app` | `CLI Box.app` | macOS app bundle name |
| `/Users/zn-ice/2026/system-test-sandbox/` | `/Users/zn-ice/2026/cli-box/` | absolute paths in settings.local.json |

---

### Task 1: Rust Source — sandbox-cli/src/main.rs

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs`

This file has the most occurrences (11). Changes span binary paths, AppleScript process names, window title searches, and app bundle names.

- [ ] **Step 1: Replace all occurrences in main.rs**

Use `replace_all` for each variant:

1. `"System Test Sandbox"` → `"CLI Box"` (display name, window title, app name)
2. `"system-test-sandbox"` → `"cli-box"` (binary name in paths, AppleScript process name)

Specific lines affected:
- Line 306: `Contents/MacOS/system-test-sandbox` → `Contents/MacOS/cli-box`
- Line 997: `title.starts_with("System Test Sandbox")` → `title.starts_with("CLI Box")`
- Line 1002: `name is "system-test-sandbox"` → `name is "cli-box"`
- Line 1359: `"System Test Sandbox.app"` → `"CLI Box.app"`
- Line 1444: `"System Test Sandbox"` → `"CLI Box"`
- Line 1447: `"Contents/MacOS/System Test Sandbox"` → `"Contents/MacOS/CLI Box"`
- Line 1452: `"dist/electron/mac-arm64/System Test Sandbox.app"` → `"dist/electron/mac-arm64/CLI Box.app"`
- Line 1454: `"Contents/MacOS/system-test-sandbox"` → `"Contents/MacOS/cli-box"`
- Line 1458: `"dist/electron/mac/System Test Sandbox.app"` → `"dist/electron/mac/CLI Box.app"`
- Line 1460: `"Contents/MacOS/system-test-sandbox"` → `"Contents/MacOS/cli-box"`
- Line 1508: `title.starts_with("System Test Sandbox")` → `title.starts_with("CLI Box")`

- [ ] **Step 2: Verify no remaining occurrences**

Run: `grep -n "system-test-sandbox\|System Test Sandbox" crates/sandbox-cli/src/main.rs`
Expected: no output

- [ ] **Step 3: Verify Rust compiles**

Run: `cargo check -p sandbox-cli`
Expected: compiles without errors

---

### Task 2: Rust Source — sandbox-core

**Files:**
- Modify: `crates/sandbox-core/src/sandbox/mod.rs`
- Modify: `crates/sandbox-core/src/capture/mod.rs`
- Modify: `crates/sandbox-core/src/daemon/mod.rs`

- [ ] **Step 1: Replace in sandbox/mod.rs**

Line 30: `"System Test Sandbox"` → `"CLI Box"`

- [ ] **Step 2: Replace in capture/mod.rs**

- Line 125 (comment): `"System Test Sandbox"` → `"CLI Box"`
- Line 146: `"System Test Sandbox"` → `"CLI Box"`

- [ ] **Step 3: Replace in daemon/mod.rs**

4 occurrences, all `ScreenCapture::find_window_by_title("System Test Sandbox")`:
- Line 320, 489, 689, 1088: `"System Test Sandbox"` → `"CLI Box"`

- [ ] **Step 4: Verify no remaining occurrences**

Run: `grep -rn "system-test-sandbox\|System Test Sandbox" crates/sandbox-core/src/`
Expected: no output

- [ ] **Step 5: Verify Rust compiles**

Run: `cargo check -p sandbox-core`
Expected: compiles without errors

---

### Task 3: Rust Tests

**Files:**
- Modify: `crates/sandbox-core/tests/sandbox_integration.rs`
- Modify: `crates/sandbox-core/tests/config_integration.rs`

- [ ] **Step 1: Replace in sandbox_integration.rs**

- Line 8: `"System Test Sandbox"` → `"CLI Box"`
- Line 75: `"System Test Sandbox"` → `"CLI Box"`

- [ ] **Step 2: Replace in config_integration.rs**

- Line 8: `"System Test Sandbox"` → `"CLI Box"`
- Line 112: `"System Test Sandbox"` → `"CLI Box"`

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p sandbox-core`
Expected: all tests pass

---

### Task 4: Electron App Source

**Files:**
- Modify: `electron-app/electron-builder.config.cjs`
- Modify: `electron-app/src/main/index.ts`
- Modify: `electron-app/src/renderer/main.tsx`
- Modify: `electron-app/src/renderer/index.html`
- Modify: `electron-app/src/renderer/styles.css`

- [ ] **Step 1: Replace in electron-builder.config.cjs**

- Line 3: `appId: "com.system-test-sandbox"` → `appId: "com.cli-box"`
- Line 4: `productName: "System Test Sandbox"` → `productName: "CLI Box"`

- [ ] **Step 2: Replace in electron-app/src/main/index.ts**

- Line 74: `title: "System Test Sandbox"` → `title: "CLI Box"`

- [ ] **Step 3: Replace in electron-app/src/renderer/main.tsx**

- Line 218: `>System Test Sandbox<` → `>CLI Box<`

- [ ] **Step 4: Replace in electron-app/src/renderer/index.html**

- Line 6: `<title>System Test Sandbox</title>` → `<title>CLI Box</title>`

- [ ] **Step 5: Replace in electron-app/src/renderer/styles.css**

- Line 1: `/* System Test Sandbox` → `/* CLI Box`

- [ ] **Step 6: Verify Electron builds**

Run: `cd electron-app && pnpm build`
Expected: build succeeds

---

### Task 5: Build Scripts — release.sh

**Files:**
- Modify: `release.sh`

7 occurrences of the old name.

- [ ] **Step 1: Replace all in release.sh**

Use `replace_all` for both variants:
1. `"System Test Sandbox"` → `"CLI Box"` (APP_NAME, pkill target, README content)
2. `system-test-sandbox` → `cli-box` (comment header)

Specific lines:
- Line 5: `# system-test-sandbox —` → `# cli-box —`
- Line 21: `APP_NAME="System Test Sandbox"` → `APP_NAME="CLI Box"`
- Line 64: `pkill -x "System Test Sandbox"` → `pkill -x "CLI Box"`
- Line 145: `# System Test Sandbox —` → `# CLI Box —`
- Line 155: `System Test Sandbox.app/` → `CLI Box.app/`
- Line 175: `System Test Sandbox.app` → `CLI Box.app`
- Line 277: `System Test Sandbox.app` → `CLI Box.app`

- [ ] **Step 2: Verify no remaining occurrences**

Run: `grep -n "system-test-sandbox\|System Test Sandbox" release.sh`
Expected: no output

---

### Task 6: GitHub Actions CI/CD

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Replace in ci.yml**

- Line 1: `# system-test-sandbox - CI` → `# cli-box - CI`
- Line 449: `system-test-sandbox CI` → `cli-box CI`

- [ ] **Step 2: Replace in release.yml**

- Line 1: `# system-test-sandbox - Release` → `# cli-box - Release`
- Line 10: `System Test Sandbox.app.zip` → `CLI Box.app.zip`
- Line 11: `System Test Sandbox_*_aarch64.dmg` → `CLI Box_*_aarch64.dmg`
- Line 94: `"System Test Sandbox.app.zip"` → `"CLI Box.app.zip"`

- [ ] **Step 3: Verify no remaining occurrences**

Run: `grep -rn "system-test-sandbox\|System Test Sandbox" .github/`
Expected: no output

---

### Task 7: Documentation — Project Root

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `DEBUG.md`

- [ ] **Step 1: Replace in CLAUDE.md**

Use `replace_all` for both variants. Key lines:
- Line 1: title reference
- Line 58: `cargo build --release -p system-test-sandbox` → `cargo build --release -p sandbox-cli`
- Line 322: same cargo build reference
- Line 365: reference to the project

- [ ] **Step 2: Replace in README.md**

Use `replace_all` for both variants. Key lines:
- Line 1: `# system-test-sandbox` → `# cli-box`
- Line 37: git clone URL `system-test-sandbox.git` → `cli-box.git`
- Line 38: `cd system-test-sandbox` → `cd cli-box`
- Line 52: `"System Test Sandbox"` window name → `"CLI Box"`

- [ ] **Step 3: Replace in DEBUG.md**

Use `replace_all` for both variants.

- [ ] **Step 4: Verify no remaining occurrences**

Run: `grep -n "system-test-sandbox\|System Test Sandbox" CLAUDE.md README.md DEBUG.md`
Expected: no output

---

### Task 8: Documentation — docs/ directory

**Files:**
- Modify: `docs/design/rebuild-plan.md`
- Modify: `docs/design/electron-rust-architecture.md`
- Modify: `docs/design/phase-8-fixes.md`
- Modify: `docs/study/terminal-comparison.md` (16 occurrences)
- Modify: `docs/task/README.md`
- Modify: `docs/task/phase-5-multi-instance.md`
- Modify: `docs/question_analysis/xterm-wkwebview-write-stall.md`

- [ ] **Step 1: Replace in all docs/ files**

For each file, use `replace_all` for both variants:
- `system-test-sandbox` → `cli-box`
- `System Test Sandbox` → `CLI Box`

- [ ] **Step 2: Verify no remaining occurrences**

Run: `grep -rn "system-test-sandbox\|System Test Sandbox" docs/`
Expected: no output

---

### Task 9: Documentation — tests/ and release/

**Files:**
- Modify: `tests/MANUAL_TEST_GUIDE.md`
- Modify: `tests/REPORT.md`
- Modify: `tests/release_test/2026-05-31-11-32-23/test-report.md`
- Modify: `release/README.md`

- [ ] **Step 1: Replace in tests/ markdown files**

For each file, use `replace_all` for both variants.

- [ ] **Step 2: Replace in release/README.md**

Use `replace_all` for both variants.

- [ ] **Step 3: Verify no remaining occurrences**

Run: `grep -rn "system-test-sandbox\|System Test Sandbox" tests/ release/`
Expected: no output

---

### Task 10: Documentation — docs/superpowers/plans/

**Files:**
- Modify: `docs/superpowers/plans/2026-05-24-terminal-first-approach.md`
- Modify: `docs/superpowers/plans/2026-05-24-pty-reader-thread.md`
- Modify: `docs/superpowers/plans/2026-05-24-pty-websocket-streaming.md`
- Modify: `docs/superpowers/plans/2026-05-30-electron-shell.md`
- Modify: `docs/superpowers/plans/2026-05-30-electron-rust-daemon.md`
- Modify: `docs/superpowers/plans/2026-05-31-phase8-bugfixes.md`

- [ ] **Step 1: Replace in all plan files**

For each file, use `replace_all` for both variants.

- [ ] **Step 2: Verify no remaining occurrences**

Run: `grep -rn "system-test-sandbox\|System Test Sandbox" docs/superpowers/`
Expected: no output

---

### Task 11: Claude Settings — .claude/settings.local.json

**Files:**
- Modify: `.claude/settings.local.json`

This file has ~90+ occurrences, all in absolute paths like `/Users/zn-ice/2026/system-test-sandbox/...`.

- [ ] **Step 1: Replace path prefix**

Use `replace_all`:
- `/Users/zn-ice/2026/system-test-sandbox/` → `/Users/zn-ice/2026/cli-box/`

- [ ] **Step 2: Verify no remaining occurrences**

Run: `grep -c "system-test-sandbox" .claude/settings.local.json`
Expected: 0

---

### Task 12: Clean Build Artifacts & Rebuild

**Files:**
- Delete: `release/` directory (old .app bundles with old name)
- Delete: `electron-app/out/` (stale build output)
- Delete: `dist/` (old Electron dist output)

- [ ] **Step 1: Remove stale build artifacts**

Run:
```bash
rm -rf release/ electron-app/out/ dist/
```

- [ ] **Step 2: Rebuild Electron app**

Run:
```bash
cd electron-app && pnpm install && pnpm build && cd ..
```

- [ ] **Step 3: Rebuild Rust binaries**

Run:
```bash
cargo build --release -p sandbox-cli -p sandbox-daemon
```

- [ ] **Step 4: Run full test suite**

Run:
```bash
cargo test -p sandbox-core && cd electron-app && pnpm test && cd ..
```

Expected: all tests pass

---

### Task 13: Final Verification — Full Codebase Scan

- [ ] **Step 1: Scan entire codebase for any remaining occurrences**

Run:
```bash
grep -rn "system-test-sandbox\|System Test Sandbox" \
  --include="*.rs" --include="*.ts" --include="*.tsx" --include="*.js" \
  --include="*.json" --include="*.md" --include="*.yml" --include="*.yaml" \
  --include="*.sh" --include="*.cjs" --include="*.html" --include="*.css" \
  --exclude-dir="node_modules" --exclude-dir="target" --exclude-dir="dist" \
  --exclude-dir="out" --exclude-dir=".git" .
```

Expected: no output (or only this plan file itself)

- [ ] **Step 2: Run local check sequence**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --all-targets \
  && cargo check --all-targets && cargo test --all \
  && cd electron-app && pnpm typecheck && pnpm format:check && pnpm test:unit && cd ..
```

Expected: all checks pass

- [ ] **Step 3: Commit all changes**

```bash
git add -A
git commit -m "rename: system-test-sandbox → cli-box

Rename all occurrences of the old project name across source code,
configuration, documentation, and build files.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```
