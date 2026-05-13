# Phase 2: HTTP API + MCP Server

> 目标：暴露 HTTP 和 MCP 双协议接口，让 Agent CLI 可以远程控制沙箱

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P2-01 | HTTP API Server：基于 axum 实现 REST API | Rust |
| P2-02 | HTTP 端点：/health, /windows, /processes, /app/spawn, /cli/spawn | Rust |
| P2-03 | HTTP 端点：/input/click, /input/type, /input/key | Rust |
| P2-04 | HTTP 端点：/screenshot, /screenshot/:window_id | Rust |
| P2-05 | MCP Server：基于 rmcp 实现 MCP stdio 协议 | Rust |
| P2-06 | MCP Tools：screenshot, click, type_text, press_key, spawn_cli, list_windows | Rust |
| P2-07 | 集成测试：HTTP API 端到端测试 | Rust |

## 验收标准

- `sandbox-cli serve` 启动 HTTP + MCP 服务
- `curl http://127.0.0.1:5801/health` 返回健康状态
- `curl http://127.0.0.1:5801/screenshot` 返回 PNG 截图
- `curl -X POST http://127.0.0.1:5801/input/click` 能触发点击
- MCP server 能通过 stdio 被 Claude Code 调用
- 所有 MCP tools 返回正确结果
