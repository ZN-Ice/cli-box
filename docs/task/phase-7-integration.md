# Phase 7: 集成测试与发布

> 目标：完善测试覆盖，更新 MCP 服务器，完成端到端验证和文档更新。

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P7-01 | 实例注册中心单元测试：CRUD、并发访问、过期清理 | Rust |
| P7-02 | CLI 集成测试：SandboxClient + mock HTTP server | Rust |
| P7-03 | MCP 服务器更新：添加 list_sandboxes、start_sandbox、close_sandbox 工具 | Rust |
| P7-04 | 端到端冒烟测试：start --cli echo → screenshot → close | Manual + CI |
| P7-05 | 更新文档：CLAUDE.md、README.md、docs/task/* | Docs |

## 验收标准

- `cargo test --all` 全部通过（含新增测试）
- `cargo fmt --all -- --check` + `cargo clippy --all-targets` 无警告
- `cargo check --all-targets` 通过
- `pnpm typecheck` + `pnpm format:check` + `pnpm test:unit` 通过
- MCP 工具 `list_sandboxes`、`start_sandbox`、`close_sandbox` 可用
- 端到端流程：start → list → screenshot → close 全程正常
- CLAUDE.md、README.md、docs/task/* 文档已更新
