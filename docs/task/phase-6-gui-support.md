# Phase 6: GUI 应用支持与前端集成

> 目标：完善沙箱对 macOS GUI 应用的支持，并将前端所有 stub handler 替换为真实 API 调用。

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P6-01 | Tauri --cli 模式：启动时在 PTY 中运行 CLI，输出流式传输到前端 | Rust + TS |
| P6-02 | Tauri --app 模式：启动 macOS 应用，发现窗口，关联到沙箱 | Rust |
| P6-03 | 前端 API 客户端层：fetch 封装所有沙箱操作 (`sandbox-web/src/api.ts`) | TS |
| P6-04 | 连接 main.tsx：将所有 stub handler 替换为真实 API 调用 | TS |
| P6-05 | 连接 Terminal 组件：PTY 读写通过 API 轮询 | TS |
| P6-06 | 更新 StatusBar 组件：显示沙箱 ID、端口、进程信息 | TS |

## GUI 应用支持说明

真实的窗口嵌入（将外部 app 的 NSWindow 嵌入沙箱窗口）需要私有 macOS API，不可行。替代方案：

- 通过 NSWorkspace 启动应用
- 通过 ScreenCaptureKit 发现应用窗口
- 将应用窗口定位在沙箱窗口附近
- 通过 CGEvent + AXUIElement 提供完整的自动化交互
- 沙箱关闭时自动终止关联应用

## 验收标准

- `sandbox-cli start --cli "claude"` 在 xterm.js 中显示 Claude Code 交互界面
- `sandbox-cli start --app "/Applications/TextEdit.app"` 启动应用并关联
- 前端可实时查看沙箱状态（ID、运行进程、截图）
- 前端 Terminal 支持 PTY 输入输出
- ControlPanel 所有按钮产生真实的沙箱操作
- RecordControls 录制/回放功能可用
