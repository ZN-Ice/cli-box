# Phase 5: 沙箱多实例管理

> 目标：实现多实例沙箱管理系统，支持 `start --cli/--app`、`list`、`close` 命令，每个沙箱拥有唯一 ID 和独立 HTTP API。

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P5-01 | 实例注册中心：`InstanceRegistry` + `SandboxInstance` + ID 生成 (`sandbox-core/src/instance.rs`) | Rust |
| P5-02 | 增强 Sandbox struct：添加 id/port/kind/start_time 字段，支持多实例 | Rust |
| P5-03 | 将 HTTP 服务器从 sandbox-cli 迁移到 sandbox-core：库化 server.rs，添加 PTY 端点 | Rust |
| P5-04 | HTTP 客户端模块：`SandboxClient` 封装 reqwest 调用 (`sandbox-cli/src/client.rs`) | Rust |
| P5-05 | 新增 CLI 命令：`start --cli/--app`、`list`、`close <id>` | Rust |
| P5-06 | 实例作用域操作：screenshot/click/type/key 支持 `--id` 参数 | Rust |
| P5-07 | Tauri 多实例支持：CLI 参数解析 + 内嵌 HTTP 服务器 + 关闭清理 | Rust |
| P5-08 | workspace Cargo.toml：添加 reqwest、uuid 依赖 | Rust |

## 架构决策

每个沙箱实例 = 一个独立的 Tauri 窗口进程，拥有：
- 唯一 ID (8 字符 hex)
- 内嵌 axum HTTP 服务器（随机端口）
- 文件系统注册：`~/.sandbox/instances/<id>.json`

CLI 通过注册中心发现实例，通过 HTTP 通信。

```
sandbox-cli start --cli "claude"
  ├─ 1. 生成 sandbox ID (generate_instance_id)
  ├─ 2. 分配可用端口 (bind 127.0.0.1:0)
  ├─ 3. 启动 Tauri: open -n -a "System Test Sandbox" --args --sandbox-id=<id> --port=<port> --mode=cli --cmd=claude
  ├─ 4. 轮询 http://127.0.0.1:<port>/health 等待就绪
  ├─ 5. 写入 ~/.sandbox/instances/<id>.json
  └─ 6. 打印 Sandbox ID
```

## 验收标准

- `sandbox-cli start --cli "echo hello"` 打开沙箱窗口并返回 ID
- `sandbox-cli list` 列出所有活跃沙箱及其状态
- `sandbox-cli screenshot <id> -o test.png` 截取指定沙箱截图
- `sandbox-cli click <id> 100 200` 在指定沙箱内模拟点击
- `sandbox-cli close <id>` 关闭沙箱，清理注册信息
- 所有现有测试仍通过
- 多个沙箱可同时运行，互不干扰
