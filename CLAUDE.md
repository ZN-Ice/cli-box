# cli-box — 开发工作流指南

> **使用方式**：用户发送需求后，Claude 读取本文档，按照预设工作流自动执行完整开发周期。
>
> **核心原则**：Superpowers 驱动 · 测试先行 · 代码检视 · 不自动合入主分支

---

## 一、项目概览

**cli-box** 是一个 macOS 桌面自动化沙箱，支持多实例管理。通过 CLI 命令启动独立沙箱窗口，在其中运行任意 CLI 或 macOS 应用，并通过模拟鼠标/键盘操作与截图反馈进行自动化控制。

**架构**：Rust daemon（PTY + 自动化引擎）+ Electron GUI（xterm.js + React）+ CLI 工具

```
Agent / 用户 (CLI / MCP / HTTP)
        │ HTTP (localhost:15801)
        ▼
cli-box-daemon (Rust, 单实例)
        │ WebSocket (PTY 流 + 截图)
        ▼
Electron App (单实例, Chromium)
```

**技术栈**：Rust (tokio + axum) · Electron (React + xterm.js) · macOS API (CGEvent + AXUIElement + ScreenCaptureKit)

---

## 二、开发工作流（完整周期）

当用户发送新需求时，按以下阶段顺序执行：

### 阶段 0：分支准备

```bash
git checkout main && git pull
git checkout -b feat/<scope>-<short-description>
```

- 从 main 新切特性分支
- 分支命名：`feat/`、`fix/`、`test/`、`docs/` 前缀

### 阶段 1：需求分析与方案设计

**使用技能**：`superpowers:brainstorming`

1. **探索项目上下文** — 读取相关代码、文档、最近提交
2. **需求澄清** — 一次一个问题，与用户对齐目标和约束
3. **方案探讨** — 提出 2-3 个方案，分析 trade-offs，给出推荐
4. **设计呈现** — 分段呈现架构、组件、数据流、错误处理、测试策略
5. **设计文档** — 写入 `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
6. **自检** — 检查占位符、内部一致性、范围、歧义
7. **用户确认** — 等用户 review spec 后再进入下一阶段

### 阶段 2：测试设计

在写实现计划之前，先设计测试策略：

| 测试层级 | 工具 | 覆盖范围 |
|---------|------|---------|
| **UT (单元测试)** | `cargo test` (Rust) · `vitest` (TS) | 单个函数/模块，mock 外部依赖 |
| **IT (集成测试)** | `cargo test --test daemon_integration` | Daemon HTTP API 端点，tower::ServiceExt::oneshot |
| **E2E (端到端)** | `test.sh` · `tests/e2e-*.sh` | 完整用户场景，CLI 命令驱动 |

测试设计原则：
- **TDD**：先写失败测试 → 实现 → 通过 → 重构
- **场景覆盖**：正常路径 + 边界条件 + 错误处理
- **回归看护**：发现 bug 时，先补充能复现问题的测试，再修复

### 阶段 3：实现计划

**使用技能**：`superpowers:writing-plans`

- 写入 `docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md`
- 每个 Task 包含：Files（精确路径）、Steps（完整代码）、Verification（命令 + 预期输出）、Commit
- 遵循 DRY · YAGNI · TDD · 频繁提交

### 阶段 4：计划执行

**使用技能**：`superpowers:subagent-driven-development`

每个 Task：
1. 派发实现子代理（提供完整上下文，不共享会话历史）
2. 实现 → 测试 → 提交
3. **Spec 合规检视** — 实现是否匹配 spec
4. **代码质量检视** — Rust 惯用法、一致性、测试质量
5. 修复检视问题 → 重新检视 → 通过后进入下一 Task

### 阶段 5：代码检视

**使用技能**：`superpowers:requesting-code-review`

完成所有 Task 后，派发最终代码检视：
- Base SHA → HEAD SHA 完整 diff
- 关注：架构一致性、测试覆盖、安全问题、命名规范
- Critical 立即修复，Important 合入前修复，Minor 记录

### 阶段 6：测试执行与回归

#### 6.1 本地质量门禁

```bash
sh test.sh
```

执行内容：
- `cargo test -p cli-box-core -p cli-box-cli` — Rust 测试
- `cargo clippy --all-targets -- -D warnings` — 静态分析
- `cargo fmt --all -- --check` — 格式检查
- `pnpm typecheck` — TypeScript 类型检查
- `pnpm vitest run` — 前端单元测试
- Playwright E2E 测试
- E2E skill 安装测试
- "sandbox" 残留检查

**如果失败**：使用 `superpowers:systematic-debugging` 分析根因，修复后回归。

#### 6.2 Release 构建

```bash
sh release.sh
```

产出：`release/cli-box` + `release/cli-box-daemon` + `release/CLI Box.app`

#### 6.3 Release 测试

按照 `tests/release_test.md` 执行手动场景测试：
- 只使用 CLI 命令测试，不直接调用 REST API
- 每步操作都截图保存到 `release_test/YYYY-MM-DD-HH-MM-SS/`
- 检查截图是否符合预期
- 生成 markdown 测试报告

**如果测试出问题**：
1. 分析错误根因
2. 判断能否通过 UT/IT/E2E 复现
3. 如果可以，补充测试用例覆盖问题场景
4. 修复问题后回归全部测试

### 阶段 7：提交与 CI

```bash
git push -u origin <branch-name>
gh pr create --title "<title>" --body "<body>"
```

- **提交远端**，等待 CI 执行结果
- **不合入主分支** — PR 保持 open 状态
- 分析 CI 结果，如有失败则修复后重新推送

#### 7.1 PR 描述规范

PR 描述必须包含 **Problem** 和 **Solution** 两部分：

```markdown
## Problem
描述这个 PR 解决了什么问题（用户视角或系统视角）

## Solution
描述解决方案（每个问题对应一个子节，包含关键 commit 引用）

## Test Plan
- [x] 测试项...
```

- 使用 `gh pr edit <number> --body "..."` 更新描述
- 在阶段 6.3 Release 测试完成后更新，确保测试结果反映在描述中

---

## 三、Git 规范

### Commit 格式

```
<type>(<scope>): <description>

[optional body]
[optional footer]
```

| type | 用途 |
|------|------|
| `feat` | 新功能 |
| `fix` | Bug 修复 |
| `test` | 测试相关 |
| `docs` | 文档更新 |
| `refactor` | 重构（不改变行为） |
| `chore` | 构建/工具链变更 |

### Scope

`sandbox` · `automation` · `capture` · `process` · `server` · `cli` · `ui` · `daemon` · `client` · `electron`

### 提交策略

- 小粒度提交，每个提交可独立编译通过
- 实现和测试在同一个或相邻提交中
- `cargo fmt` 和 `clippy` 问题在提交前修复

---

## 四、目录速查

| 内容 | 路径 |
|------|------|
| 核心库 | `crates/cli-box-core/src/` |
| Daemon | `crates/cli-box-core/src/daemon/mod.rs` |
| 实例管理 | `crates/cli-box-core/src/instance/` |
| HTTP 服务器 | `crates/cli-box-core/src/server/` |
| CLI 入口 | `crates/cli-box-cli/src/main.rs` |
| HTTP 客户端 | `crates/cli-box-cli/src/client.rs` |
| Electron 前端 | `electron-app/src/` |
| 前端 API 层 | `electron-app/src/api.ts` |
| 设计文档 | `docs/superpowers/specs/` |
| 实现计划 | `docs/superpowers/plans/` |
| Release 测试脚本 | `test.sh` · `release.sh` |
| Release 测试场景 | `tests/release_test.md` |
| Release 测试报告 | `release_test/YYYY-MM-DD-HH-MM-SS/` |
| 本文件 | `CLAUDE.md` |

---

## 五、测试层级说明

### UT (单元测试)

- **Rust**：`#[cfg(test)] mod tests` 在每个文件内，`cargo test` 运行
- **TypeScript**：`*.test.ts` 文件，`vitest` 运行
- 测试单个函数/模块，mock 外部依赖（HTTP、文件系统、macOS API）

### IT (集成测试)

- **位置**：`crates/cli-box-core/tests/daemon_integration.rs`
- **方式**：`tower::ServiceExt::oneshot` 测试 daemon 路由，不绑定真实 TCP 端口
- **覆盖**：每个 HTTP 端点的正常/错误路径

### E2E (端到端测试)

- **脚本**：`test.sh` 编排所有测试，`tests/e2e-*.sh` 执行具体场景
- **场景**：CLI 命令 → daemon 响应 → 验证结果
- **覆盖**：sandbox 生命周期、截图、输入模拟、skill 安装

### Release 测试

- **触发**：`sh release.sh` 构建后手动执行
- **场景**：按照 `tests/release_test.md` 中的完整用户流程
- **产出**：截图 + markdown 测试报告

---

## 六、Superpowers 技能使用

| 阶段 | 技能 | 触发时机 |
|------|------|---------|
| 需求分析 | `superpowers:brainstorming` | 新需求开始时 |
| 计划编写 | `superpowers:writing-plans` | 设计确认后 |
| 计划执行 | `superpowers:subagent-driven-development` | 实现阶段 |
| 代码检视 | `superpowers:requesting-code-review` | 每个 Task 完成后 + 最终检视 |
| Bug 调试 | `superpowers:systematic-debugging` | 测试失败或异常行为时 |
| 分支收尾 | `superpowers:finishing-a-development-branch` | 所有任务完成时 |

**调试原则**（systematic-debugging）：
- **NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST**
- Phase 1: 根因调查 → Phase 2: 模式分析 → Phase 3: 假设验证 → Phase 4: 修复实现
- 3 次修复失败 → 质疑架构，与用户讨论

---

## 七、关键约束

- **语言**：用户使用中文交流，代码和注释使用英文
- **不自动合入**：PR 创建后保持 open，不执行 merge
- **测试驱动**：先写测试，再写实现
- **CLI 优先**：Release 测试只使用 CLI 命令，不直接调用 REST API
- **截图验证**：Release 测试每步操作都截图，检查图片是否符合预期
- **Superpowers 驱动**：使用 Superpowers 技能完成各阶段工作，不跳过
- **零侵入**：目标应用不需要任何适配，所有操作在 OS 层面完成

---

**版本**：v0.2.0 | **创建**：2026-05-13 | **更新**：2026-06-06 | **维护者**：cli-box 项目
