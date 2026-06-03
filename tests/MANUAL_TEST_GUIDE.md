# macOS 手动测试指南

## 前置准备

### 1. 环境要求

- macOS (Apple Silicon 或 Intel)
- Rust >= 1.88 (`rustup show`)
- Xcode Command Line Tools (`xcode-select --install`)
- curl
- Python 3 (用于部分格式化输出)

### 2. 授予系统权限（必须）

运行截图和输入模拟功能需要两个权限：

| 权限 | 路径 |
|------|------|
| **Accessibility** | 系统设置 → 隐私与安全 → 辅助功能 |
| **Screen Recording** | 系统设置 → 隐私与安全 → 屏幕录制 |

将你的**终端应用**（Terminal.app / iTerm.app / VS Code）加入这两个权限的白名单。这两个权限必须手动授予，无法通过代码绕过。

### 3. Swift dylib 修复（必须）

`screencapturekit` crate 存在 rpath 问题，需要手动复制 Swift 运行时库：

```bash
# 确认 dylib 存在
ls /Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx/libswift_Concurrency.dylib

# 先编译一次项目（让 build 目录创建）
cargo build -p cli-box-cli

# 复制 dylib
cp /Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx/libswift_Concurrency.dylib \
   target/debug/
```

> 每次 `cargo clean` 后需要重新复制。这会在终端输出 Class duplicate 警告，不影响功能。

---

## 测试 1: 自动化集成测试 (CI 等同)

这些测试在任何平台都能跑，不需要 macOS API。

```bash
# 运行全部沙箱核心测试 (67 个)
cargo test -p cli-box-core

# 分别运行各个集成测试
cargo test -p cli-box-core --test diff_integration      # 图片对比 (10 tests)
cargo test -p cli-box-core --test scenario_integration   # 场景引擎 (14 tests)
cargo test -p cli-box-core --test sandbox_integration    # 沙箱管理 (15 tests)
cargo test -p cli-box-core --test recorder_integration   # 录制回放 (10 tests)
cargo test -p cli-box-core --test error_integration      # 错误类型 (5 tests)
```

**预期结果**：67 passed, 0 failed

---

## 测试 2: CLI 子命令 (macOS)

### 2.1 列出所有窗口

```bash
cargo run -p cli-box-cli -- windows
```

**预期**：列出现有所有窗口 ID 和标题。终端输出示例：

```
Window ID=34:
Window ID=38166: V2RayX
Window ID=120: 豆包
Total: 147 windows
```

### 2.2 CLI 进程管理（注：跨进程无状态）

```bash
# 启动 CLI 进程
cargo run -p cli-box-cli -- spawn-cli -- echo "hello sandbox"
```

**预期输出**：
```
CLI spawned: PID=1000, name=echo
Use 'cli-box-cli kill 1000' to terminate
```

```bash
# 查看进程
cargo run -p cli-box-cli -- processes
```

**预期输出**：`Total: 0 processes` — 这是正常的，因为每个 CLI 命令是独立进程，SESSIONS 不共享。需要持久化管理请使用 HTTP Server 模式（测试 3）。

### 2.3 截图

```bash
# 通过 title 搜索沙箱窗口截图（首先需要启动 Tauri app 才有效）
cargo run -p cli-box-cli -- screenshot

# 指定输出路径
cargo run -p cli-box-cli -- screenshot -o /tmp/my_screenshot.png
```

**如果没有 Tauri 窗口运行**，会返回错误 `Sandbox window not found`，这是预期的。

---

## 测试 3: HTTP API (macOS)

### 3.1 启动服务器

```bash
cargo run -p cli-box-cli -- serve --port 5801
```

看到以下输出表示启动成功：
```
Sandbox HTTP API server started on http://127.0.0.1:5801
  GET  http://127.0.0.1:5801/health
  GET  http://127.0.0.1:5801/screenshot
  POST http://127.0.0.1:5801/input/click
  POST http://127.0.0.1:5801/cli/spawn
```

⚠️ **保持此终端运行**，后续命令在新终端窗口中执行。

### 3.2 健康检查

```bash
curl -s http://127.0.0.1:5801/health | python3 -m json.tool
```

**预期**：
```json
{
    "status": "ok",
    "version": "0.1.0",
    "uptime_secs": 0
}
```

### 3.3 窗口列表

```bash
curl -s http://127.0.0.1:5801/windows | python3 -c "
import sys,json
data=json.load(sys.stdin)
print(f'Total: {len(data)} windows')
for w in data[:5]:
    print(f'  ID={w[0]}: {w[1][:50] if w[1] else \"(no title)\"}')
"
```

### 3.4 区域截图

```bash
# 截取屏幕 (100,100) 位置 200x200 区域
curl -s -o /tmp/sandbox_region.png "http://127.0.0.1:5801/screenshot/region?x=100&y=100&width=200&height=200"

# 验证结果
file /tmp/sandbox_region.png
# 预期: PNG image data, 200 x 200, 8-bit/color RGBA, non-interlaced
```

### 3.5 进程管理

```bash
# 启动 CLI
curl -s -X POST http://127.0.0.1:5801/cli/spawn \
  -H 'Content-Type: application/json' \
  -d '{"command":"echo","args":["hello sandbox"]}'
# 预期: {"pid":1000,"name":"echo","path":null,"is_running":true}

# 列出进程
curl -s http://127.0.0.1:5801/processes | python3 -m json.tool
# 预期: [{"pid":1000,"name":"echo","path":null,"is_running":true}]

# 终止进程
curl -s -X POST http://127.0.0.1:5801/process/kill \
  -H 'Content-Type: application/json' \
  -d '{"pid":1000}'
# 预期: {"killed":1000}

# 验证已终止
curl -s http://127.0.0.1:5801/processes
# 预期: []
```

### 3.6 输入模拟

```bash
# ⚠️ 以下命令会实际操控鼠标/键盘！

# 鼠标点击 (100, 100)
curl -s -X POST http://127.0.0.1:5801/input/click \
  -H 'Content-Type: application/json' \
  -d '{"x":100,"y":100,"button":"left"}'
# 预期: {"clicked":{"button":"left","x":100.0,"y":100.0}}

# 键盘输入
curl -s -X POST http://127.0.0.1:5801/input/type \
  -H 'Content-Type: application/json' \
  -d '{"text":"hello"}'
# 预期: {"typed":"hello"}

# 按键
curl -s -X POST http://127.0.0.1:5801/input/key \
  -H 'Content-Type: application/json' \
  -d '{"key":"tab"}'
# 预期: {"pressed":{"key":"tab","modifiers":[]}}

# 组合键 (Cmd+Return)
curl -s -X POST http://127.0.0.1:5801/input/key \
  -H 'Content-Type: application/json' \
  -d '{"key":"return","modifiers":["cmd"]}'
# 预期: {"pressed":{"key":"return","modifiers":["cmd"]}}

# 滚动
curl -s -X POST http://127.0.0.1:5801/input/scroll \
  -H 'Content-Type: application/json' \
  -d '{"x":100,"y":100,"direction":"down","amount":3}'
# 预期: {"scrolled":true}

# 拖拽
curl -s -X POST http://127.0.0.1:5801/input/drag \
  -H 'Content-Type: application/json' \
  -d '{"from_x":100,"from_y":100,"to_x":200,"to_y":200}'
# 预期: {"dragged":true}
```

### 3.7 图片对比 (Diff)

```bash
python3 << 'PYEOF'
import json, base64, urllib.request

# 读取刚才截的图
with open('/tmp/sandbox_region.png', 'rb') as f:
    png_data = f.read()
b64 = base64.standard_b64encode(png_data).decode()

# 发送 diff 请求（同一张图对比自身）
req = urllib.request.Request(
    'http://127.0.0.1:5801/diff',
    data=json.dumps({
        'expected': b64,
        'actual': b64,
        'max_diff_percentage': 0.0
    }).encode(),
    headers={'Content-Type': 'application/json'}
)
result = json.loads(urllib.request.urlopen(req).read())
print(json.dumps(result, indent=2))
PYEOF

# 预期: {"identical": true, "diff_percentage": 0.0, "total_pixels": 40000, "changed_pixels": 0}
```

### 3.8 场景执行

```bash
python3 << 'PYEOF'
import json, urllib.request

scenario_yaml = """
name: "HTTP integration test"
steps:
  - type: wait
    duration_ms: 100
  - type: click
    x: 50
    y: 50
    button: left
  - type: wait
    duration_ms: 100
"""

req = urllib.request.Request(
    'http://127.0.0.1:5801/scenario/run',
    data=json.dumps({'yaml': scenario_yaml, 'speed': 10.0}).encode(),
    headers={'Content-Type': 'application/json'}
)
result = json.loads(urllib.request.urlopen(req).read())
print(f"Name: {result['name']}")
print(f"Status: {result['status']}")
print(f"Steps: {result['passed']}/{result['total']} passed, {result['failed']} failed")
print(f"Duration: {result['duration_ms']}ms")
PYEOF

# 预期: 3/3 passed
```

### 3.9 录制

```bash
# 开始录制
curl -s -X POST http://127.0.0.1:5801/record/start \
  -H 'Content-Type: application/json' \
  -d '{}'
# 预期: {"recording":true}

# 停止录制
curl -s -X POST http://127.0.0.1:5801/record/stop \
  -H 'Content-Type: application/json' \
  -d '{}'
# 预期: {"recording":false,"actions_count":0,"actions":[]}
```

### 3.10 停止服务器

```bash
# 回到运行服务器的终端，按 Ctrl+C
# 或直接 kill
pkill -f "sandbox serve"
```

---

## 测试 4: MCP Server (macOS)

```bash
# 发送 initialize + tools/list + tools/call
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_windows","arguments":{}}}\n' | cargo run -p cli-box-cli -- mcp-serve 2>/dev/null | head -3
```

**预期输出**：三行 JSON-RPC 响应：
1. `initialize` 返回 `{"serverInfo":{"name":"cli-box","version":"0.1.0"}}`
2. `tools/list` 返回 18 个 MCP 工具定义
3. `tools/call` 返回 `list_windows` 的结果

---

## 测试 5: 全功能场景 (需要 Tauri 窗口)

此测试需要先启动 Tauri 沙箱宿主应用才能完整运行。

### 5.1 启动 Tauri 宿主

前提条件：安装 pnpm 依赖。

```bash
pnpm install
pnpm tauri dev
```

这会打开一个 1280x800 的 "CLI Box" 窗口。

### 5.2 使用预置 YAML 场景

```bash
cargo run -p cli-box-cli -- serve --port 5801 &
sleep 2

# 用完整的 11 步 YAML 场景测试（需要 macOS，因为涉及 click/type 等）
python3 << 'PYEOF'
import json, urllib.request

with open('tests/fixtures/full_scenario.yaml', 'r') as f:
    yaml_content = f.read()

req = urllib.request.Request(
    'http://127.0.0.1:5801/scenario/run',
    data=json.dumps({'yaml': yaml_content, 'speed': 5.0}).encode(),
    headers={'Content-Type': 'application/json'}
)
result = json.loads(urllib.request.urlopen(req).read())
print(f"Name: {result['name']}")
print(f"Status: {result['status']}")
print(f"Steps: {result['passed']}/{result['total']} passed")
if result['failed'] > 0:
    print(f"  FAILED: {result['failed']} steps")
PYEOF
```

---

## 测试结果速查表

| 测试 | 命令 | 不需要 macOS |
|------|------|-------------|
| 集成测试 | `cargo test -p cli-box-core` | ✅ |
| CLI windows | `cargo run -p cli-box-cli -- windows` | ❌ |
| CLI spawn-cli | `cargo run -p cli-box-cli -- spawn-cli -- echo hi` | ❌ |
| HTTP /health | `curl http://127.0.0.1:5801/health` | ❌ |
| HTTP /windows | `curl http://127.0.0.1:5801/windows` | ❌ |
| HTTP screenshot/region | `curl "http://127.0.0.1:5801/screenshot/region?x=0&y=0&width=100&height=100"` | ❌ |
| HTTP /cli/spawn | `curl -X POST .../cli/spawn -d '{...}'` | ❌ |
| HTTP /input/* | `curl -X POST .../input/click -d '{...}'` | ❌ |
| HTTP /diff | `curl -X POST .../diff -d '{...}'` | ✅ |
| HTTP /scenario/run | `curl -X POST .../scenario/run -d '{...}'` | ❌ |
| MCP    | `echo '...' \| cargo run -p cli-box-cli -- mcp-serve` | ❌ |

---

## 常见问题

### Q: `Library not loaded: @rpath/libswift_Concurrency.dylib`

**原因**：screencapturekit 的 Swift 构建产物依赖 rpath。

**解决**：执行前置准备第 3 步的 dylib 复制。

### Q: `Class _TtCs... is implemented in both`

**现象**：终端出现大量 Class duplicate 警告。

**解决**：无害警告，不影响功能。由 dylib 修复导致。

### Q: UI inspect 返回空

**原因**：当前 CLI 没有 Accessibility entitlements 签名。

**解决**：需要用 entitlements plist 进行代码签名。这是已知限制。

### Q: Sandbox window not found

**原因**：Tauri 宿主应用没有运行。

**解决**：运行 `pnpm tauri dev` 启动沙箱窗口。
