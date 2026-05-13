# Phase 0: 项目骨架 + 沙箱窗口 + 基础 CLI

> 目标：建立可用的项目骨架，能启动沙箱窗口并运行基础 CLI

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P0-01 | Cargo Workspace 初始化：sandbox-core (lib) + sandbox-cli (bin) + src-tauri (Tauri) | Rust |
| P0-02 | sandbox-core 骨架：lib.rs + error.rs + 各模块 mod.rs | Rust |
| P0-03 | sandbox-cli 骨架：clap 参数解析，serve/screenshot/windows/spawn/click/type/key 命令 | Rust |
| P0-04 | Tauri 2 骨架：src-tauri 初始化，sandbox 窗口配置 | Rust |
| P0-05 | sandbox-web 骨架：React + xterm.js 终端组件 | TS |
| P0-06 | 沙箱窗口管理：Sandbox struct，init/screenshot/state 方法 | Rust |
| P0-07 | PTY 进程管理：spawn_cli/send_input/list_processes | Rust |

## 验收标准

- `cargo build --workspace` 编译通过
- `cargo test --workspace` 测试通过
- `sandbox-cli serve` 能启动服务（即使功能是 stub）
- `sandbox-cli screenshot` 能执行（即使返回占位图）
- `pnpm dev` 能启动前端
- Tauri 窗口能打开并显示 xterm.js 终端
