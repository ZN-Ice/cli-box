# Phase 1: CGEvent 输入模拟 + ScreenCaptureKit 截图

> 目标：实现核心自动化能力——模拟鼠标/键盘输入，截取沙箱窗口截图

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P1-01 | CGEvent 输入模拟：click/double_click/type_text/press_key/scroll/drag | Rust |
| P1-02 | ScreenCaptureKit 窗口截图：capture_window 返回 base64 PNG | Rust |
| P1-03 | ScreenCaptureKit 区域截图：capture_region | Rust |
| P1-04 | 沙箱窗口截图集成：Sandbox.screenshot() 调用 SCContentFilter | Rust |
| P1-05 | 进程管理完善：spawn_app (NSWorkspace.open) + kill_process | Rust |
| P1-06 | 单元测试：输入模拟 + 截图 + 进程管理 | Rust |

## 验收标准

- CGEvent 能在沙箱窗口内模拟鼠标点击和键盘输入
- ScreenCaptureKit 能只截取沙箱窗口（不截全屏）
- 截图不需要沙箱窗口在前台
- `sandbox-cli screenshot` 能输出有效 PNG
- `sandbox-cli click 100 200` 能在沙箱内点击
- `sandbox-cli type "hello"` 能在沙箱内输入文本
- 单元测试覆盖核心功能
