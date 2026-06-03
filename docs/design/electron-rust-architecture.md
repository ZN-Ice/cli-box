# 方案 A：Electron + Rust Daemon 架构设计

> 日期：2026-05-30
> 分支：`design/electron-rust-architecture`
> 状态：草案

## 一、背景与动机

### 1.1 当前架构

```
cli-box (Tauri 2.11.2)
├── Rust 后端：直接在 Tauri 进程内调用 macOS API
├── WKWebView 渲染：macOS 系统 WebKit（Safari 内核）
└── 多实例：每个 sandbox start 启动一个独立 Tauri 进程
```

每个沙箱 = 一个 Tauri 进程 (~30MB)，进程级强隔离。

### 1.2 核心问题

| 问题 | 表现 | 影响 |
|------|------|------|
| **WKWebView setTimeout 失效** | xterm.js WriteBuffer 的后续 setTimeout(0) 不触发 | TUI 应用空白屏（已通过 writeDirect 绕过） |
| **终端渲染残留** | Claude Code 中旧内容未被正确擦除，出现鬼影文字 | 影响可用性，无法通过代码完全修复 |
| **渲染流畅度** | 与 waveterm (Electron/Chromium) 和 VS Code 对比有差距 | 核心体验问题 |
| **无 DevTools** | 调试前端困难 | 开发效率低 |

**根因：WebKit 的 Canvas 渲染管线对高频终端输出的处理不如 Chromium 成熟。** `writeDirect` 解决了 setTimeout 卡死，但 WebKit 的 Canvas 合成器在处理 ANSI 光标移动/擦除指令时仍然会产生渲染残留。

### 1.3 为什么不直接换 WebView2

WebView2 是 Windows 专属的。Tauri 在各平台的渲染引擎：

| 平台 | Tauri 渲染引擎 | 终端渲染质量 |
|------|---------------|-------------|
| macOS | WKWebView (WebKit) | 有问题 |
| Windows | WebView2 (Chromium) | 正常 |
| Linux | WebKitGTK | 可能有类似问题 |

macOS 上只能用 WKWebView 或换 Electron/CEF。

## 二、目标架构

### 2.1 架构图

```
┌──────────────────────────────────────────────────────────────────┐
│                     CLI / Agent / 用户                            │
│  sandbox start          sandbox screenshot --id abc              │
│  sandbox list           sandbox click --id abc 100 200           │
└───────────────────────────────┬──────────────────────────────────┘
                                │ HTTP (localhost:port)
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  Electron 主进程 (单实例, requestSingleInstanceLock)              │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Tab Manager (Tabs/Workspaces)                              │  │
│  │                                                            │  │
│  │ ┌──────────────┐ ┌──────────────┐ ┌──────────────────┐    │  │
│  │ │ Tab A        │ │ Tab B        │ │ Tab C            │    │  │
│  │ │ CLI mode     │ │ CLI mode     │ │ APP mode         │    │  │
│  │ │ xterm.js     │ │ xterm.js     │ │ 截图预览+控制面板  │    │  │
│  │ │ (Chromium)   │ │ (Chromium)   │ │ (Chromium)       │    │  │
│  │ └──────┬───────┘ └──────┬───────┘ └────────┬─────────┘    │  │
│  └────────┼────────────────┼──────────────────┼───────────────┘  │
│           │                │                  │                   │
│           ↕ WS (直连 daemon，不经 Electron 主进程中转)          │
│  ┌────────────────────────┬───────────────────────────────────┐  │
│  │              IPC Bridge (Electron 主进程)                    │  │
│  │   仅用于：Tab 创建/销毁通知，daemon 状态监控                   │  │
│  └────────────────────────┬───────────────────────────────────┘  │
│                           │ HTTP (控制请求) + WS (PTY 输出)       │
│  ┌────────────────────────▼───────────────────────────────────┐  │
│  │         sandbox-daemon (Rust 子进程, 单实例)                 │  │
│  │                                                            │  │
│  │  ┌─────────────┐ ┌──────────────┐ ┌───────────────────┐   │  │
│  │  │ PTY Manager │ │ App Manager  │ │ Automation Engine │   │  │
│  │  │             │ │              │ │                   │   │  │
│  │  │ 每个 CLI    │ │ NSWorkspace  │ │ ScreenCaptureKit  │   │  │
│  │  │ 沙箱独立    │ │ launch +     │ │ CGEvent           │   │  │
│  │  │ PTY 进程    │ │ 进程追踪     │ │ AXUIElement       │   │  │
│  │  └─────────────┘ └──────────────┘ └───────────────────┘   │  │
│  │                                                            │  │
│  │  ┌──────────────────────────────────────────────────────┐   │  │
│  │  │ HTTP Server + WebSocket Server                        │   │  │
│  │  │ :15801 (默认，占用时自动递增)                          │   │  │
│  │  └──────────────────────────────────────────────────────┘   │  │
│  │                                                            │  │
│  │  ┌──────────────────────────────────────────────────────┐   │  │
│  │  │ Instance Registry (~/.sandbox/instances/)             │   │  │
│  │  └──────────────────────────────────────────────────────┘   │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 设计原则

1. **Rust daemon 做所有系统级工作** — PTY、截图、输入模拟、UI 检查、APP 启动
2. **Electron 只做 UI** — Tab 管理、xterm.js 渲染、控制面板、DevTools
3. **CLI 直连 daemon** — `sandbox screenshot --id abc` 不经过 Electron，直接 HTTP 到 daemon
4. **单 Electron 实例 + 单 daemon 实例** — 类似 waveterm 的 `requestSingleInstanceLock` 模式
5. **守护进程保活** — 任一组件崩溃可恢复

## 三、组件设计

### 3.1 sandbox-daemon (Rust 子进程)

**职责：** 所有 macOS 系统 API 调用 + PTY 管理 + HTTP API 服务

这是当前 `sandbox-core` 的能力提取为一个独立长期运行的 daemon 进程。

```
sandbox-daemon
├── HTTP API Server (axum, 与当前 server/mod.rs 接口兼容)
│   ├── GET  /health
│   ├── GET  /sandbox/list                    # 列出所有沙箱
│   ├── POST /sandbox/create                  # 创建新沙箱 (CLI 或 APP 模式)
│   ├── POST /sandbox/:id/close               # 关闭沙箱
│   ├── GET  /sandbox/:id/screenshot          # 截图
│   ├── POST /sandbox/:id/input/click         # 点击
│   ├── POST /sandbox/:id/input/type          # 输入
│   ├── POST /sandbox/:id/input/key           # 按键
│   ├── GET  /sandbox/:id/pty/ws/:pid         # PTY WebSocket
│   ├── POST /sandbox/:id/app/spawn           # 启动 .app
│   └── ...
├── PTY Manager
│   ├── 每个 CLI 沙箱拥有独立 PTY 进程
│   ├── PTY Reader 线程 (当前架构已有)
│   └── WebSocket 推送 PTY 输出到 Electron
├── App Manager
│   ├── NSWorkspace launch (当前 spawn_app 已实现)
│   ├── 进程状态追踪
│   └── SCWindow ID 发现
├── Automation Engine
│   ├── CGEvent 输入模拟 (当前 cg_event.rs)
│   ├── AXUIElement UI 检查 (当前 ax_ui.rs)
│   └── ScreenCaptureKit 截图 (当前 capture/mod.rs)
└── Instance Registry
    └── ~/.sandbox/instances/ (当前 instance/mod.rs)
```

**关键设计：** daemon 的 HTTP API 接口与当前 `/crates/sandbox-core/src/server/mod.rs` 基本一致，只是从"每实例一个 server"变为"一个 daemon 服务所有沙箱"。

**可复用的现有代码（约 2,500 行 Rust）：**
- `automation/cg_event.rs` — 373 行，直接复用
- `automation/ax_ui.rs` — 497 行，直接复用
- `capture/mod.rs` — 441 行，直接复用
- `process/mod.rs` — 509 行，直接复用
- `instance/mod.rs` — 343 行，直接复用
- `server/mod.rs` — 1,709 行，需重构为多沙箱路由

### 3.2 Electron 主进程

**职责：** Tab 管理、UI 渲染、与 daemon 通信

```
electron-app/
├── main.ts                    # 入口：requestSingleInstanceLock, spawn daemon
├── window.ts                  # 窗口管理（单 BrowserWindow + Tab 切换）
├── tab-manager.ts             # Tab 创建/切换/销毁
├── daemon-bridge.ts           # 与 sandbox-daemon 的 IPC 通信
├── preload.ts                 # 安全的 IPC 桥接
├── tray.ts                    # 系统托盘（daemon 后台运行）
└── platform/
    ├── darwin.ts              # macOS 特定逻辑
    └── win32.ts               # Windows 特定逻辑
```

**Tab 管理策略（参考 waveterm 的 WaveTabView）：**

```typescript
// 每个 Tab 对应一个沙箱
interface SandboxTab {
  id: string;           // sandbox-id
  kind: "cli" | "app";
  title: string;
  webContentsView: WebContentsView;  // Electron 内嵌视图
  daemonConn: DaemonConnection;      // 与 daemon 的 WebSocket 连接
}

// Tab 切换：把目标 Tab 的 webContentsView 移到屏幕内
// 非活跃 Tab 移到屏幕外 (x: -15000)，与 waveterm 策略一致
function switchTab(targetId: string) {
  for (const tab of tabs) {
    if (tab.id === targetId) {
      tab.webContentsView.setBounds({ x: 0, y: 0, width, height });
    } else {
      tab.webContentsView.setBounds({ x: -15000, y: -15000, width, height });
    }
  }
}
```

**Tab 内渲染内容：**

- **CLI 模式：** xterm.js 终端，WebSocket 直连 daemon（不经 Electron 主进程中转），获得 PTY 输出。**标准 `term.write()` 即可，无需 writeDirect。**
- **APP 模式：** 控制面板 + 定时截图预览。显示已启动 APP 的状态、截图缩略图、操作按钮。

**Daemon 端口发现：**

daemon 固定监听 `:15801`（单实例无需动态分配）。启动时将端口信息写入 `~/.sandbox/daemon.json`：

```json
{ "port": 15801, "pid": 12345, "started_at": "2026-05-30T10:00:00Z" }
```

Electron 启动时读取此文件发现 daemon 端口。CLI 也通过此文件定位 daemon。

### 3.3 通信协议

**参考 waveterm 和 VSCode 的通信架构：**

| 项目 | PTY host 进程 | 前端通信方式 | 特点 |
|------|--------------|-------------|------|
| waveterm | Go wavesrv（子进程） | 单一 WebSocket（RPC + 数据流复用） | 简单，所有通信走一个连接 |
| VSCode | Node.js UtilityProcess | Electron MessagePort IPC | 最原生 Electron 方式，零网络开销 |
| **我们** | Rust daemon（子进程） | Electron IPC + 本地 HTTP（见下文） | 结合两者优势 |

**我们的方案：双层通信**

```
┌─────────────────────────────────────────────────────────┐
│ Electron renderer (Tab/xterm.js)                        │
│                                                         │
│  ① 控制请求（创建沙箱、截图、输入模拟）                    │
│     → Electron main process → spawn daemon 子进程        │
│     → daemon HTTP API                                    │
│                                                         │
│  ② PTY 输出流（高频，延迟敏感）                           │
│     → WebSocket 直连 daemon（不经 Electron main process） │
│     → xterm.js 标准 term.write()                        │
│                                                         │
│  ③ 事件通知（沙箱退出、进程状态变化）                      │
│     → 同一个 WebSocket 连接上的 JSON 事件消息              │
└─────────────────────────────────────────────────────────┘

CLI → daemon：
  → 直接 HTTP 到 daemon（不经过 Electron）
```

**为什么选择 WebSocket 而非纯 RPC：**

- waveterm 的方案是单一 WebSocket 复用 RPC 和数据流，简洁实用
- VSCode 用 MessagePort 是因为 PTY host 是 Node.js 进程，天然支持 Electron IPC
- 我们的 daemon 是 Rust 进程，不支持 Electron MessagePort，WebSocket 是最自然的桥接方式
- WebSocket 的二进制帧传输 PTY 输出效率很高（无需 base64）
- RPC 语义（请求-响应）可以在 WebSocket 上层实现（JSON 消息带 request_id）

**WebSocket 消息格式（复用单一连接）：**

```
PTY 输出（daemon → 前端，二进制帧）：
  直接发送原始 PTY bytes，不编码

控制命令（前端 → daemon，JSON 帧）：
  { "type": "rpc", "id": "req-123", "command": "resize", "sandbox_id": "abc", "cols": 80, "rows": 24 }
  { "type": "rpc", "id": "req-124", "command": "input", "sandbox_id": "abc", "data": "ls\n" }

控制响应（daemon → 前端，JSON 帧）：
  { "type": "rpc_response", "id": "req-123", "success": true }
  { "type": "rpc_response", "id": "req-124", "success": true }

事件通知（daemon → 前端，JSON 帧）：
  { "type": "event", "event": "sandbox_exit", "sandbox_id": "abc", "exit_code": 0 }
  { "type": "event", "event": "app_window_changed", "sandbox_id": "def" }
```

**HTTP API（供 CLI 直接调用，与当前接口兼容）：**

```
GET  /health
GET  /sandbox/list
POST /sandbox/create                  { mode, command, args }
POST /sandbox/:id/close
GET  /sandbox/:id/screenshot
POST /sandbox/:id/input/click         { x, y, button }
POST /sandbox/:id/input/type          { text }
POST /sandbox/:id/input/key           { key, modifiers }
POST /sandbox/:id/app/spawn           { app_path }
```

### 3.4 Daemon 端口发现

**端口分配策略：**

- 默认端口 `15801`
- 如果被占用，依次尝试 `15802`, `15803`, ... `15899`
- daemon 启动时打印端口信息：`Sandbox daemon started on port 15801`

**CLI 发现 daemon 端口：**

```
1. 读取 ~/.sandbox/daemon.json
   { "port": 15802, "pid": 12345, "started_at": "2026-05-30T10:00:00Z" }

2. 验证 pid 是否存活（kill(pid, 0)）
   - 存活 → 使用该端口
   - 不存在 → 清理 daemon.json，视为 daemon 未运行

3. daemon 未运行 → CLI 负责启动 daemon，等待 daemon.json 写入后读取端口
```

Electron 启动时也通过同样机制发现 daemon 端口。

### 3.5 CLI 集成

**当前架构：**
```bash
sandbox start claude
  → CLI 解析参数
  → spawn 一个新的 Tauri 进程 (cli-box --mode=cli --cmd=claude)
  → Tauri 进程内嵌 HTTP server
  → 注册实例到 ~/.sandbox/instances/
```

**新架构：**
```bash
sandbox start claude
  → CLI 解析参数
  → 检查 daemon 是否运行（读 ~/.sandbox/daemon.json + 验证 pid）
  → 如果 daemon 未运行：
      spawn sandbox-daemon
      等待 ~/.sandbox/daemon.json 出现
      打印 "Sandbox daemon started on port 15801"
  → 如果 daemon 已运行：
      打印 "Sandbox daemon running on port 15802"
  → 检查 Electron 是否运行（同上机制，~/.sandbox/electron.json）
  → 如果 Electron 未运行：spawn electron-app
  → HTTP POST http://localhost:{port}/sandbox/create { mode: "cli", command: "claude" }
  → daemon 创建 PTY 进程，返回 { sandbox_id: "abc123", pid: 12345 }
  → daemon 通过 WebSocket 通知 Electron 创建新 Tab
  → 打印 "Sandbox abc123 created (port 15801)"
```

**CLI 操作命令（不变）：**
```bash
sandbox screenshot --id abc    → HTTP GET daemon/sandbox/abc/screenshot
sandbox click --id abc 100 200 → HTTP POST daemon/sandbox/abc/input/click
sandbox list                   → HTTP GET daemon/sandbox/list
sandbox close abc              → HTTP POST daemon/sandbox/abc/close → Electron 关闭对应 Tab
```

CLI 操作不经过 Electron，直接与 daemon 通信。对于需要切换 Tab 的操作（如截图前切换），daemon 通过 WebSocket 通知 Electron 切换 Tab。

## 四、操作流程

### 4.1 CLI 模式沙箱

```
用户: sandbox start claude

1. CLI 检查 daemon → 未运行 → spawn sandbox-daemon → 打印端口
2. CLI 检查 Electron → 未运行 → spawn electron-app
3. CLI → HTTP POST http://localhost:{port}/sandbox/create { mode: "cli", command: "claude" }
4. Daemon: 创建 PTY 进程 (zsh → claude)
5. Daemon: 返回 { sandbox_id: "abc123", pid: 12345 }
6. Daemon: 通过 WebSocket 事件通知 Electron "新沙箱已创建"
7. Electron: 创建新 Tab，建立 WebSocket 连接到 daemon
8. Electron: xterm.js 使用标准 term.write() 渲染（Chromium，无 WKWebView 问题）
```

### 4.2 APP 模式沙箱

```
用户: sandbox start /Applications/cc-switch.app

1-3. 同上
4. Daemon: NSWorkspace.open("cc-switch.app")
5. Daemon: 等待窗口出现，获取 SCWindow ID
6. Daemon: 返回 { sandbox_id: "def456", window_id: 789 }
7. Electron: 创建新 Tab（APP 控制面板模式）
8. cc-switch 作为独立 macOS 窗口运行，不在 Electron 内
9. Daemon: ScreenCaptureKit 按 window_id 截图 → 返回给 CLI
```

**APP 不在 Electron 里运行。** Electron 的 Tab 只是控制面板。cc-switch 的窗口在 macOS 桌面上独立存在。

### 4.3 沙箱作用域操作

```
用户: sandbox screenshot --id abc -o result.png

1. CLI → HTTP GET daemon/sandbox/abc/screenshot
2. Daemon: 通知 Electron 切换到 Tab abc（如果不在前台）
3. Electron: 切换 Tab
4. Daemon: ScreenCaptureKit 截取沙箱窗口
5. Daemon: 返回 PNG 数据给 CLI
6. CLI: 写入 result.png
7. Electron 窗口保持在桌面后台（不需要在最前面）
```

## 五、强隔离策略

### 5.1 隔离边界

| 层面 | 隔离机制 | 崩溃影响 |
|------|----------|----------|
| **PTY 进程** | 每个 CLI 沙箱独立 OS 进程 | 只影响该沙箱的终端 |
| **APP 进程** | macOS 独立进程 | 只影响该 APP |
| **Electron renderer** | 每个 Tab 独立 WebContentsView（Chromium 沙箱） | 只影响该 Tab 的 UI |
| **Electron 主进程** | 单点 | 所有 Tab UI 丢失 |
| **Rust daemon** | 单点 | 所有截图/输入/PTY 能力丢失 |

### 5.2 崩溃恢复

**Electron 主进程崩溃：**
```
守护进程检测到 Electron 退出
  → 从 daemon 获取活跃沙箱列表
  → 重启 Electron
  → 为每个沙箱重新创建 Tab
  → CLI 沙箱：PTY 进程还活着，WebSocket 重连即可恢复终端
  → APP 沙箱：APP 窗口还在，重新关联即可
```

**Rust daemon 崩溃：**
```
Electron 检测到 daemon 连接断开
  → 重启 daemon
  → PTY 进程已随 daemon 退出而终止（PTY fd 关闭）
  → APP 进程还在（独立进程），但需要重新注册
  → 标记 CLI 沙箱为 "disconnected"，通知用户
```

**单个 PTY 进程崩溃：**
```
只影响该沙箱
  → daemon 通知 Electron 该沙箱退出
  → Tab 显示 "进程已退出" 提示
  → 其他沙箱不受影响
```

### 5.3 与当前 Tauri 架构的隔离对比

| 场景 | Tauri 多实例 | Electron 单进程 |
|------|-------------|----------------|
| 单个 PTY 崩溃 | 只影响该实例 | 只影响该 Tab |
| UI 渲染器崩溃 | 只影响该实例 | 只影响该 Tab（Chromium renderer 隔离） |
| 主进程崩溃 | 只影响该实例（其他实例完全独立） | **所有 Tab 丢失**，需守护进程恢复 |
| daemon 崩溃 | N/A（无 daemon） | **所有能力丢失**，需重启 daemon |

**结论：** 日常场景（PTY 崩溃、渲染器崩溃）隔离效果等同。极端场景（主进程/daemon 崩溃）需要守护进程恢复，但这是罕见事件。

## 六、跨平台路径（未来）

当前阶段仅聚焦 macOS。未来扩展到 Windows 时：

- **Electron (Chromium)** 跨平台渲染行为一致，前端代码无需修改
- **sandbox-daemon (Rust)** 需要实现 Windows 系统API：SendInput（输入模拟）、DXGI Desktop Duplication（截图）、conpty（PTY）
- 通过 Rust trait 抽象平台差异：

```rust
trait AutomationEngine {
    fn click(&self, x: f64, y: f64, button: MouseButton) -> Result<()>;
    fn type_text(&self, text: &str) -> Result<()>;
    fn press_key(&self, key: &str, modifiers: &[&str]) -> Result<()>;
    fn capture_window(&self, window_id: u64) -> Result<Vec<u8>>;
}

#[cfg(target_os = "macos")]
struct MacOsEngine { /* CGEvent + AXUIElement + ScreenCaptureKit */ }
```

**Electron 选择的额外好处：** 未来 Windows 版的渲染层无需任何额外适配工作。

## 七、目录结构变更

```
cli-box/
├── Cargo.toml                     # Workspace 根
├── crates/
│   ├── sandbox-core/              # 核心库（大部分复用）
│   │   └── src/
│   │       ├── lib.rs, error.rs
│   │       ├── automation/        # ✅ 直接复用
│   │       │   ├── cg_event.rs
│   │       │   └── ax_ui.rs
│   │       ├── capture/           # ✅ 直接复用
│   │       │   └── mod.rs
│   │       ├── process/           # ✅ 直接复用
│   │       │   └── mod.rs
│   │       ├── instance/          # ✅ 直接复用
│   │       │   └── mod.rs
│   │       └── server/            # 🔧 重构为多沙箱路由
│   │           └── mod.rs
│   ├── sandbox-daemon/            # 🆕 Daemon 二进制 (从 sandbox-core 构建)
│   │   └── src/
│   │       └── main.rs            # daemon 入口：启动 HTTP server + PTY 管理
│   └── sandbox-cli/               # 🔧 修改：spawn daemon+electron 而非 Tauri
│       └── src/
│           ├── main.rs
│           ├── client.rs
│           └── mcp_server.rs
├── electron-app/                  # 🆕 Electron 应用（替代 src-tauri）
│   ├── package.json
│   ├── electron-builder.config.cjs
│   ├── main.ts                    # Electron 主进程
│   ├── preload.ts
│   └── src/
│       ├── window.ts              # 窗口管理
│       ├── tab-manager.ts         # Tab 创建/切换
│       ├── daemon-bridge.ts       # Daemon IPC
│       └── tray.ts                # 系统托盘
├── sandbox-web/                   # 🔧 修改：去掉 writeDirect，用标准 term.write()
│   └── src/
│       ├── main.tsx
│       ├── api.ts                 # 修改为连接 daemon 而非内嵌 HTTP
│       └── components/
│           ├── Terminal.tsx        # 去掉 writeDirect
│           ├── Dashboard.tsx
│           ├── Sidebar.tsx
│           └── AppControlPanel.tsx # 🆕 APP 模式控制面板
├── src-tauri/                     # ❌ 删除
└── docs/
    └── design/
        └── electron-rust-architecture.md  # 本文件
```

## 八、可复用代码评估

| 模块 | 当前行数 | 改动量 | 说明 |
|------|---------|--------|------|
| `automation/cg_event.rs` | 373 | 无 | 直接复用，daemon 内调用 |
| `automation/ax_ui.rs` | 497 | 无 | 直接复用 |
| `capture/mod.rs` | 441 | 小改 | 复用，可能需要调整窗口发现逻辑 |
| `process/mod.rs` | 509 | 小改 | 复用 PTY spawn 和 APP launch |
| `instance/mod.rs` | 343 | 中改 | 从每实例注册改为 daemon 统一管理 |
| `server/mod.rs` | 1,709 | 重构 | 从每实例 server 改为 daemon 多沙箱路由 |
| `sandbox/mod.rs` | 262 | 重构 | Sandbox 结构体适配 daemon 模式 |
| `sandbox-cli/main.rs` | 758 | 重构 | 改为 spawn daemon + Electron |
| `sandbox-cli/client.rs` | 700 | 小改 | HTTP client 基本不变 |
| **Rust 合计** | **~5,592** | **~40% 可直接复用** | |
| | | | |
| `Terminal.tsx` | 180 | 删 writeDirect | 标准化 |
| `api.ts` | 318 | 修改 | 连接 daemon |
| `Dashboard.tsx` | 194 | 修改 | 多 Tab 布局 |
| `Sidebar.tsx` | 138 | 修改 | Tab 列表 |
| `src-tauri/main.rs` | 340 | **删除** | 被 Electron main.ts 替代 |

## 九、实施阶段

### Phase 1：sandbox-daemon（Rust 侧）

**目标：** 让 sandbox-daemon 成为独立可运行进程，管理多个沙箱。

1. 创建 `crates/sandbox-daemon/` binary crate
2. 重构 `server/mod.rs` 为多沙箱路由（`/sandbox/:id/...`）
3. 添加 daemon 生命周期管理（pid 文件、信号处理、优雅关闭）
4. 修改 CLI：`sandbox start` → spawn daemon + 发送创建请求
5. 验证：CLI 通过 daemon 的 HTTP API 完成 PTY 启动、截图、输入模拟

**验证标准：** `sandbox start claude` 通过 daemon 启动 PTY，`sandbox screenshot` 通过 daemon 截图，无 Electron 参与。

### Phase 2：Electron 壳

**目标：** 用 Electron 替代 Tauri 窗口。

1. 搭建 `electron-app/` 项目（electron-vite 或 electron-forge）
2. 实现 `main.ts`：`requestSingleInstanceLock`，spawn daemon，创建 BrowserWindow
3. 实现 Tab Manager：WebContentsView 管理，Tab 切换
4. 修改前端：`api.ts` 连接 daemon，`Terminal.tsx` 去掉 writeDirect
5. 实现 APP 模式控制面板 Tab
6. 实现 `sandbox start` 时的 second-instance 处理（已有实例时创建新 Tab）

**验证标准：** `sandbox start claude` 打开 Electron 窗口，xterm.js 使用标准 `term.write()` 正常渲染 Claude Code。

### Phase 3：守护与恢复

**目标：** 处理崩溃恢复，提升可靠性。

1. Electron → daemon 心跳检测
2. daemon → Electron 状态同步
3. 崩溃恢复：守护进程自动重启 + 状态恢复
4. 系统托盘：daemon 后台运行，Electron 关闭窗口不退出

### Phase 4：完善与优化

**目标：** 生产级稳定性。

1. 完善崩溃恢复逻辑
2. 性能优化（大输出场景的 PTY 数据流）
3. Electron 打包配置（macOS .dmg）
4. 日志系统对接（daemon 日志与 Electron 日志统一）

## 十、风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| Electron 主进程崩溃导致所有 Tab 丢失 | 低 | 高 | 守护进程 + PTY 进程不丢失（可重连） |
| Rust daemon 崩溃导致所有能力丢失 | 低 | 高 | Electron 监控 + 自动重启 daemon |
| 包体积从 ~30MB 增至 ~150MB+ | 确定 | 中 | CLI 分发用轻量安装器；daemon 独立分发 |
| 内存从 ~30MB/实例 增至 ~180MB 固定 + ~30MB/Tab | 确定 | 中 | 相比 N 个 Tauri 实例，3+ 沙箱时反而更省内存 |
| IPC 延迟影响 PTY 输出流畅度 | 中 | 中 | WebSocket 直连 daemon，不经 Electron 主进程中转 |
| Electron 版本升级维护成本 | 中 | 低 | 使用 Electron LTS 版本 |
| xterm.js 内部 API 变更无需再跟踪 | 确定 | 正向 | 不再需要 writeDirect hack |

## 十一、与当前架构的对比总结

```
                   当前 (Tauri)              方案 A (Electron+Rust)
─────────────  ──────────────────         ──────────────────────
渲染引擎        WKWebView (WebKit)          Chromium
终端渲染质量    有残留/鬼影                  与 VS Code 一致
writeDirect     需要（hack）                 不需要（标准 term.write）
DevTools        需手动开启                   原生支持
包体积          ~30MB                        ~150MB+
内存 (3沙箱)    ~90MB                        ~210MB
隔离性          进程级强隔离                  Tab 级隔离 + 守护进程恢复
系统 API 调用   Rust FFI 直接（进程内）       Rust FFI 直接（daemon 内）
IPC 复杂度      无（同进程）                  Electron ↔ daemon (HTTP/WS)
多实例 UX       多窗口                       单窗口多 Tab
Rust 代码复用   100%                         ~40% 直接复用，~30% 重构
重写工作量      0                            大（~2-3 周）
```
