# system-test-sandbox 任务管理系统

> 所有开发任务必须先记录后执行。任务文件：`docs/task/task_records.json`

## 任务记录格式

```json
{
  "task_id": "P{phase}{序号}",
  "task_type": "设计文档/功能开发/测试编写/重构优化",
  "phase": "Phase 0-4",
  "module": "sandbox/automation/capture/process/server/cli/ui",
  "layer": "rust/ts/both",
  "task_desc": "任务描述",
  "executor": "Claude Code/人工",
  "status": "待执行/进行中/已完成/驳回",
  "create_time": "2026-05-13 10:00:00",
  "finish_time": null,
  "check_result": null,
  "remark": "备注"
}
```

## 任务状态流转

| 当前状态 | 可转换状态 | 触发条件 |
|---------|-----------|---------|
| 待执行 | 进行中 | 创建特性分支并开始执行 |
| 进行中 | 已完成 | 任务成功且通过本地校验（推送前） |
| 进行中 | 驳回 | 任务执行失败或校验不通过 |

> **重要**：任务状态更新必须在推送远端之前完成，推送远端后不再修改任务记录。

## 分支命名规范

| 类型 | 命名格式 | 示例 |
|------|---------|------|
| Phase 开发 | `phase/{N}-{描述}` | `phase/1-automation` |
| 功能开发 | `feat/{模块}-{描述}` | `feat(capture-screencapturekit)` |
| Bug修复 | `fix/{描述}` | `fix/screenshot-permission` |
| 重构 | `refactor/{模块}-{描述}` | `refactor/input-simulator` |
| 文档 | `docs/{描述}` | `docs/architecture` |

## PR 合入规范

| 规则 | 说明 |
|------|------|
| CI 门禁全通过 | 必须（fmt + clippy + check + test + typecheck） |
| Squash Merge | 保持 main 历史整洁 |
| 自动删除分支 | 合并后自动删除特性分支 |
| 禁止 Force Push | 保护代码历史 |

## Phase 任务文档索引

| Phase | 文档 | 目标 |
|-------|------|------|
| Phase 0 | [phase-0-skeleton.md](./phase-0-skeleton.md) | 项目骨架 + 沙箱窗口 + 基础 CLI |
| Phase 1 | [phase-1-automation.md](./phase-1-automation.md) | CGEvent 输入模拟 + ScreenCaptureKit 截图 |
| Phase 2 | [phase-2-server.md](./phase-2-server.md) | HTTP API + MCP Server |
| Phase 3 | [phase-3-ui-inspect.md](./phase-3-ui-inspect.md) | AXUIElement UI 检查 |
| Phase 4 | [phase-4-advanced.md](./phase-4-advanced.md) | 高级特性：多窗口、录制回放、测试框架 |
