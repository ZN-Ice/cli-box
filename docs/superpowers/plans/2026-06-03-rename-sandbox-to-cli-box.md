# Rename sandbox → cli-box Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rename all user-facing "sandbox" references to "cli-box" across the entire codebase, including crate names, binary names, API routes, config paths, UI text, and documentation.

**Architecture:** This is a mechanical rename with semantic decisions:
- Crate names: `sandbox-core` → `cli-box-core`, `sandbox-cli` → `cli-box-cli`, `cli-box-daemon` → `cli-box-daemon`
- Binary names: `sandbox` → `cli-box`, `cli-box-daemon` → `cli-box-daemon`
- API routes: `/sandbox/` → `/box/`
- Config dir: `~/.sandbox/` → `~/.cli-box/`
- Internal identifiers (SandboxConfig, ManagedSandbox, etc.) stay as-is

---

## Task 1: Rename Cargo crate directories and package names

**Files:**
- Rename dir: `crates/sandbox-core/` → `crates/cli-box-core/`
- Rename dir: `crates/sandbox-cli/` → `crates/cli-box-cli/`
- Rename dir: `crates/cli-box-daemon/` → `crates/cli-box-daemon/`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/cli-box-core/Cargo.toml` (package name)
- Modify: `crates/cli-box-cli/Cargo.toml` (package name, dependency)
- Modify: `crates/cli-box-daemon/Cargo.toml` (package name, dependency)

- [ ] Rename directories
- [ ] Update workspace Cargo.toml members
- [ ] Update each crate's Cargo.toml package name
- [ ] Update dependency references between crates
- [ ] Verify: `cargo check --all-targets`

## Task 2: Update all Rust import paths

**Files:** All `.rs` files that `use sandbox_core::` or `use sandbox_cli::`

- [ ] Replace `sandbox_core` → `cli_box_core` in all Rust use statements
- [ ] Replace `sandbox_cli` → `cli_box_cli` in all Rust use statements
- [ ] Replace `sandbox_daemon` → `cli_box_daemon` in all Rust use statements
- [ ] Verify: `cargo check --all-targets`

## Task 3: Rename binary names

**Files:**
- Modify: `crates/cli-box-cli/Cargo.toml` — `[[bin]] name = "sandbox"` → `"cli-box"`
- Modify: `crates/cli-box-cli/src/main.rs` — clap `name = "sandbox"` → `"cli-box"`
- Modify: `crates/cli-box-daemon/Cargo.toml` — `[[bin]] name` if present

- [ ] Update binary names in Cargo.toml
- [ ] Update clap command name
- [ ] Verify: `cargo check -p cli-box-cli`

## Task 4: Update API routes /sandbox/ → /box/

**Files:**
- Modify: `crates/cli-box-core/src/daemon/mod.rs` (~20 route definitions)
- Modify: `crates/cli-box-core/src/server/mod.rs`
- Modify: `crates/cli-box-cli/src/client.rs` (~15 fetch calls)
- Modify: `electron-app/src/renderer/api.ts` (~6 fetch/WebSocket calls)
- Modify: All test files with route mocks

- [ ] Replace `/sandbox/` → `/box/` in daemon routes
- [ ] Replace `/sandbox/` → `/box/` in CLI client
- [ ] Replace `/sandbox/` → `/box/` in TS API layer
- [ ] Replace `/sandbox/` → `/box/` in e2e tests
- [ ] Replace `/sandbox/` → `/box/` in Rust tests
- [ ] Verify: `cargo test -p cli-box-core`

## Task 5: Update config dir ~/.sandbox/ → ~/.cli-box/

**Files:**
- Modify: `crates/cli-box-core/src/daemon/mod.rs`
- Modify: `crates/cli-box-cli/src/main.rs`
- Modify: `electron-app/src/main/index.ts`
- Modify: `electron-app/src/main/daemon-bridge.ts`
- Modify: `release.sh`

- [ ] Replace `.sandbox` → `.cli-box` in all files
- [ ] Verify: `cargo check --all-targets`

## Task 6: Update daemon binary name references

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs` — `find_daemon_binary()` references
- Modify: `electron-app/src/main/daemon-bridge.ts`
- Modify: `electron-app/electron-builder.config.cjs`
- Modify: `release.sh`

- [ ] Replace `cli-box-daemon` → `cli-box-daemon` in all files
- [ ] Verify: `cargo check --all-targets`

## Task 7: Update UI text in Electron app

**Files:**
- Modify: `electron-app/src/renderer/main.tsx` — "sandbox" in UI text

- [ ] Replace display text: "sandbox" → "CLI Box" where appropriate
- [ ] Verify: `cd electron-app && pnpm build`

## Task 8: Update CI/CD and build scripts

**Files:**
- Modify: `release.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `test.sh`

- [ ] Replace binary names, crate names, paths
- [ ] Verify: `sh test.sh`

## Task 9: Update documentation

**Files:** All `.md` files

- [ ] Replace CLI command examples: `cli-box start` → `cli-box start`
- [ ] Replace binary references: `./sandbox` → `./cli-box`
- [ ] Replace daemon references: `cli-box-daemon` → `cli-box-daemon`
- [ ] Replace crate names in docs
- [ ] Replace config paths: `~/.sandbox/` → `~/.cli-box/`

## Task 10: Update remaining test files

**Files:**
- Modify: `crates/cli-box-core/tests/*.rs`
- Modify: `electron-app/e2e/*.spec.ts`
- Modify: `electron-app/src/__tests__/*.test.ts`

- [ ] Update all test references
- [ ] Verify: `cargo test --all && cd electron-app && pnpm test`

## Task 11: Final verification and rebuild

- [ ] Full codebase grep for remaining "sandbox" references
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test --all`
- [ ] `cd electron-app && pnpm typecheck && pnpm test:unit`
- [ ] `sh release.sh`
- [ ] Commit
