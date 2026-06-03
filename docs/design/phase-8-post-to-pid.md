# Phase 8: CGEvent 定向投递 (post_to_pid)

## 问题

当前 `InputSimulator` 所有方法通过 `CGEvent::post(CGEventTapLocation::HID)` 发送事件。这是一个全局投递 API — 鼠标事件基于屏幕绝对坐标，键盘事件发送给当前焦点窗口。

如果 VS Code 覆盖了 Tauri 沙箱窗口：
- 鼠标点击落到 VS Code 而不是沙箱
- 键盘输入被 VS Code 接收
- 沙箱窗口不需要在前台（截图已经通过 ScreenCaptureKit 按 window_id 解决）

## 方案

使用 macOS `CGEvent::post_to_pid(pid)` API，将 CGEvent 直接投递到目标进程的事件队列。`core-graphics 0.25.0` 已暴露此方法。

### 核心变更

```
// 之前 (全局投递)
event.post(CGEventTapLocation::HID)

// 之后 (定向投递到 Tauri 进程)
event.post_to_pid(target_pid)
```

### API 设计

`InputSimulator` 所有公开方法添加 `target_pid: Option<u32>` 参数：

```rust
pub fn click(x: f64, y: f64, button: MouseButton, target_pid: Option<u32>) -> Result<()>
pub fn type_text(text: &str, target_pid: Option<u32>) -> Result<()>
pub fn press_key(key: &str, modifiers: &[&str], target_pid: Option<u32>) -> Result<()>
pub fn scroll(x: f64, y: f64, direction: &str, amount: i32, target_pid: Option<u32>) -> Result<()>
pub fn drag(from_x: f64, from_y: f64, to_x: f64, to_y: f64, target_pid: Option<u32>) -> Result<()>
pub fn double_click(x: f64, y: f64, target_pid: Option<u32>) -> Result<()>
```

- `Some(pid)`: 使用 `post_to_pid(pid)` — 事件直接投递到目标进程
- `None`: 使用 `post(HID)` — 保持当前行为（向后兼容 standalone 模式）

### 数据流

```
┌─────────────────────────────────────────────────────────┐
│ AppState {                                              │
│   target_pid: Option<u32>,  // Tauri 进程的 PID         │
│   window_id: Option<u32>,                               │
│   ...                                                   │
│ }                                                       │
└──────────────┬──────────────────────────────────────────┘
               │
    ┌──────────┼──────────────────────────────┐
    ▼          ▼                               ▼
click_handler  type_handler            playback_handler
    │              │                           │
    │  target_pid  │  target_pid    ActionPlayer { target_pid }
    ▼              ▼                    │
InputSimulator::click(x, y, btn, Some(pid))
    │
    ├─ Some(pid) → CGEvent::post_to_pid(pid)
    └─ None       → CGEvent::post(HID)
```

### 变更文件清单

| 文件 | 变更 |
|------|------|
| `crates/cli-box-core/src/automation/cg_event.rs` | 所有方法添加 `target_pid` 参数，内部 `post_event` helper |
| `crates/cli-box-core/src/server/mod.rs` | AppState 添加 `target_pid`，handlers 传递给 InputSimulator |
| `crates/cli-box-core/src/player.rs` | ActionPlayer 添加 `target_pid`，传递给 InputSimulator |
| `crates/cli-box-cli/src/mcp_server.rs` | 调用点传递 `None`（MCP 无 PID 上下文） |
| `crates/cli-box-cli/src/main.rs` | `start` 命令设置 `target_pid`，本地操作传 `None` |

### Standalone 模式保护

standalone 模式无 Tauri 窗口，`target_pid` 为 `None`，回退到全局 `post(HID)`。这是正确的——standalone 模式下输入操作本来就面向当前桌面。

### 向后兼容

- 所有现有调用点只需在末尾添加 `None` 参数
- 行为不变（`None` = 全局投递）
- 未来 Tauri 窗口启动后，`target_pid` 自动生效
