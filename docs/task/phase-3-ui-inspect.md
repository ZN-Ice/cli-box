# Phase 3: AXUIElement UI 检查

> 目标：实现 UI 元素树读取，让 Agent 能理解沙箱内应用的 UI 结构

## 任务清单

| 任务 ID | 描述 | 层 |
|---------|------|----|
| P3-01 | AXUIElement 窗口树遍历：inspect_window 返回 UiElement 树 | Rust |
| P3-02 | AXUIElement 搜索：find_elements 按 role/title 查找 | Rust |
| P3-03 | AXUIElement 值读取：get_element_value | Rust |
| P3-04 | HTTP 端点：/ui/inspect/:window_id, /ui/find | Rust |
| P3-05 | MCP Tools：inspect_ui, find_element, get_element_value | Rust |
| P3-06 | 单元测试：UI 树遍历 + 搜索 | Rust |

## 验收标准

- 能读取沙箱内任意窗口的 AX 元素树
- 能按 role（如 AXButton, AXTextField）搜索元素
- 能获取元素的 title/value/bounds
- HTTP 和 MCP 接口都能调用 UI 检查
- 单元测试覆盖核心功能
