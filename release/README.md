# System Test Sandbox — Release v0.2.0

macOS 桌面自动化沙箱，支持多实例管理。通过一条命令启动沙箱并在其中运行 CLI 或 macOS 应用，模拟鼠标/键盘操作并获取截图反馈。

## 文件说明

```
release/
├── sandbox                        # CLI 工具（命令行）
├── System Test Sandbox.app        # macOS 桌面应用（Tauri）
└── README.md                      # 本文件
```

## 一、前置条件

| 依赖 | 版本要求 |
|------|---------|
| macOS | 14.0+ (Sonoma) |
| 芯片 | Apple Silicon (M1–M4)，Intel 也支持 |

### 必须授予的权限

> **没有这两个权限，sandbox 无法工作。**

1. **辅助功能 (Accessibility)**：用于 CGEvent 输入模拟 + AXUIElement UI 读取
2. **屏幕录制 (Screen Recording)**：用于 ScreenCaptureKit 截图

授予方式：`系统设置 → 隐私与安全性 → 辅助功能 / 屏幕录制`，将 `sandbox` 或 `System Test Sandbox.app` 添加进去并勾选。

## 二、多实例沙箱管理（核心功能）

### 启动沙箱

```bash
# 启动沙箱，运行 Claude Code 终端 → 返回沙箱 ID
./sandbox start --cli "claude"
# 输出: Starting sandbox: a1b2c3d4 (claude) on port 15801
#       Sandbox started: a1b2c3d4

# 启动沙箱，运行任意 CLI 命令
./sandbox start --cli "vim" --args "test.txt"
./sandbox start --cli "python3" --args "-i"

# 启动沙箱，运行 macOS 应用
./sandbox start --app "/System/Applications/TextEdit.app"
./sandbox start --app "/Applications/cc-switch.app"
```

### 查看和管理沙箱

```bash
# 列出所有活跃沙箱
./sandbox list
# ID       TITLE                    KIND   STATUS     PORT   CREATED
# a1b2c3d4 claude                   CLI    Running    15801  2026-05-17 10:30:00
# e5f6g7h8 TextEdit                 APP    Running    15802  2026-05-17 10:31:00

# 查看沙箱详情
./sandbox inspect a1b2c3d4

# 关闭沙箱（自动清理注册 + 终止进程）
./sandbox close a1b2c3d4
```

### 操作沙箱内目标

```bash
# 截取指定沙箱截图
./sandbox screenshot --id a1b2c3d4 -o result.png

# 在指定沙箱内模拟点击
./sandbox click --id a1b2c3d4 500 300

# 在指定沙箱内模拟输入
./sandbox type --id a1b2c3d4 "hello world"

# 在指定沙箱内模拟按键
./sandbox key --id a1b2c3d4 Return
./sandbox key --id a1b2c3d4 c --modifiers cmd

# 查看沙箱内窗口
./sandbox windows --id a1b2c3d4

# 查看沙箱内进程
./sandbox processes --id a1b2c3d4

# 在沙箱内启动新的 CLI
./sandbox spawn-cli --id a1b2c3d4 "ls" -la

# 终止沙箱内进程
./sandbox kill --id a1b2c3d4 12345
```

## 三、独立 HTTP 服务模式（向后兼容）

```bash
./sandbox serve --port 5801
```

启动后可用端点：

```
GET  http://127.0.0.1:5801/health              # 健康检查
GET  http://127.0.0.1:5801/sandbox/info         # 沙箱信息
POST http://127.0.0.1:5801/shutdown             # 关闭沙箱
GET  http://127.0.0.1:5801/screenshot           # 截图 (PNG)
POST http://127.0.0.1:5801/input/click          # 鼠标点击
POST http://127.0.0.1:5801/input/type           # 键盘输入
POST http://127.0.0.1:5801/input/key            # 按键
POST http://127.0.0.1:5801/cli/spawn            # 启动 CLI 进程
POST http://127.0.0.1:5801/app/spawn            # 启动 macOS 应用
GET  http://127.0.0.1:5801/windows              # 列出窗口
GET  http://127.0.0.1:5801/processes            # 列出进程
POST http://127.0.0.1:5801/pty/write            # 写入 PTY
GET  http://127.0.0.1:5801/pty/output/:pid      # 读取 PTY 输出
GET  http://127.0.0.1:5801/ui/inspect/:window   # 读取 UI 树
```

## 四、MCP 服务（供 Claude Code / OpenCode 调用）

```bash
./sandbox mcp-serve
```

在 `.claude/settings.json` 中配置：

```json
{
  "mcpServers": {
    "sandbox": {
      "command": "/absolute/path/to/release/sandbox",
      "args": ["mcp-serve"]
    }
  }
}
```

MCP 工具列表：screenshot, click, double_click, type_text, press_key, scroll, spawn_cli, spawn_app, kill_process, list_processes, list_windows, inspect_ui, find_element, get_element_value, record_start, record_stop, play_actions, run_scenario, diff_screenshot

## 五、curl 调用示例

```bash
# 截图
curl http://127.0.0.1:5801/screenshot -o screenshot.png

# 点击
curl -X POST http://127.0.0.1:5801/input/click \
  -H "Content-Type: application/json" \
  -d '{"x": 100, "y": 200, "button": "left"}'

# 输入文字
curl -X POST http://127.0.0.1:5801/input/type \
  -H "Content-Type: application/json" \
  -d '{"text": "hello"}'

# 启动 CLI
curl -X POST http://127.0.0.1:5801/cli/spawn \
  -H "Content-Type: application/json" \
  -d '{"command": "ls", "args": ["-la"]}'
```

## 六、桌面应用

1. 双击 `System Test Sandbox.app` 启动
2. 可通过 CLI `./sandbox start --cli "xxx"` 自动启动并关联

## 七、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: `start` 启动后没有打开窗口？**
A: 如果 Tauri 桌面应用不在同目录，CLI 会自动 fallback 到 standalone HTTP 模式。通过返回的端口可直接 curl 调用。

**Q: `serve` 端口被占用？**
A: 使用 `./sandbox serve --port 5802` 更换端口。`start` 命令会自动分配端口。

**Q: MCP 连接失败？**
A: 确认 `settings.json` 中的 `command` 路径是绝对路径。

**Q: 如何清理残留的沙箱注册？**
A: 删除 `~/.sandbox/instances/` 目录下的 `.json` 文件。

---

**版本**: v0.2.0 | **构建时间**: 2026-05-17
