#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# system-test-sandbox — Release Build Script
# ============================================================
# Builds the Tauri sandbox app + CLI binary and packages
# them into ./release/.
#
# Prerequisites:
#   - Rust >= 1.88
#   - Node.js >= 20 + pnpm
#   - macOS (Apple Silicon or Intel)
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

RELEASE_DIR="$SCRIPT_DIR/release"
VERSION="0.1.0"
APP_NAME="System Test Sandbox"

# --- helpers ---
info()  { echo "  ➜  $*"; }
ok()    { echo "  ✓  $*"; }
err()   { echo "  ✗  $*" >&2; exit 1; }
check() {
    if command -v "$1" &>/dev/null; then
        ok "$1 found: $(command -v "$1")"
    else
        err "$1 not found — please install $1"
    fi
}

echo ""
echo "=============================================="
echo " ${APP_NAME} v${VERSION} — Release Build"
echo "=============================================="
echo ""

# --- step 1: check prerequisites ---
info "Checking prerequisites..."
check rustc
check cargo
check pnpm
check node
ok "All prerequisites met"

# --- step 2: clean up old processes & registries ---
echo ""
info "Cleaning up old sandbox processes..."

# Kill daemon by PID from daemon.json (avoid pkill -f which matches Electron apps)
if [ -f ~/.sandbox/daemon.json ]; then
    DAEMON_PID=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1])).get('pid',''))" ~/.sandbox/daemon.json 2>/dev/null)
    if [ -n "$DAEMON_PID" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null || true
        info "Stopped daemon (PID $DAEMON_PID)"
    fi
    rm -f ~/.sandbox/daemon.json
fi

# Kill legacy Tauri app by exact process name (not -f, which is too broad)
pkill -x "System Test Sandbox" 2>/dev/null || true

# Kill CLI processes by exact binary name
pkill -x "sandbox" 2>/dev/null || true
pkill -x "sandbox-daemon" 2>/dev/null || true

rm -f ~/.sandbox/instances/*.json 2>/dev/null || true
ok "Cleanup done"

# --- step 3: build frontend ---
echo ""
info "Building frontend (sandbox-web)..."
cd "$SCRIPT_DIR/sandbox-web"
pnpm install --silent 2>&1 | tail -1
pnpm build 2>&1 | tail -5
ok "Frontend built"

# --- step 4: build Tauri app (includes Rust build) ---
echo ""
info "Building Tauri sandbox app..."
cd "$SCRIPT_DIR"
cargo tauri build 2>&1 | tail -10

TAURI_BUNDLE="$SCRIPT_DIR/target/release/bundle/macos/${APP_NAME}.app"
if [ ! -d "$TAURI_BUNDLE" ]; then
    err "Tauri app bundle not found at $TAURI_BUNDLE"
fi
ok "Tauri app built: $(du -sh "$TAURI_BUNDLE" | cut -f1)"

# --- step 5: build CLI + daemon binaries (release) ---
echo ""
info "Building CLI + daemon binaries (release)..."
cargo build --release -p sandbox-cli -p sandbox-daemon
CLI_BIN="$SCRIPT_DIR/target/release/sandbox"
DAEMON_BIN="$SCRIPT_DIR/target/release/sandbox-daemon"
if [ ! -f "$CLI_BIN" ]; then
    err "CLI binary not found at $CLI_BIN"
fi
if [ ! -f "$DAEMON_BIN" ]; then
    err "Daemon binary not found at $DAEMON_BIN"
fi
ok "CLI binary built: $(du -h "$CLI_BIN" | cut -f1)"
ok "Daemon binary built: $(du -h "$DAEMON_BIN" | cut -f1)"

# --- step 6: assemble release folder ---
echo ""
info "Assembling release artifacts -> $RELEASE_DIR"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# Copy CLI
cp "$CLI_BIN" "$RELEASE_DIR/sandbox"
chmod +x "$RELEASE_DIR/sandbox"
ok "sandbox CLI binary"

# Copy daemon
cp "$DAEMON_BIN" "$RELEASE_DIR/sandbox-daemon"
chmod +x "$RELEASE_DIR/sandbox-daemon"
ok "sandbox-daemon binary"

# Copy Tauri app bundle
cp -R "$TAURI_BUNDLE" "$RELEASE_DIR/${APP_NAME}.app"
ok "${APP_NAME}.app bundle"

# --- step 7: generate README ---
echo ""
info "Generating README.md..."

BUILD_DATE="$(date '+%Y-%m-%d %H:%M')"

cat > "$RELEASE_DIR/README.md" << 'RELEASEREADME'
# System Test Sandbox — Release v${VERSION}

macOS 桌面自动化沙箱。通过 CLI 启动 Tauri 沙箱窗口，内置 xterm.js 终端运行命令行工具（如 Claude Code），支持截图和输入模拟。

## 文件说明

```
release/
├── sandbox                     # CLI 工具（命令行入口）
├── System Test Sandbox.app/    # Tauri 沙箱 macOS 应用
└── README.md                   # 本文件
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

授予方式：\`系统设置 → 隐私与安全性 → 辅助功能 / 屏幕录制\`。

将 \`sandbox\` 和 \`System Test Sandbox.app\` 添加进去并勾选。

## 二、使用方法

### 启动沙箱

\`\`\`bash
# 在沙箱中启动 Claude Code（交互模式）
./sandbox start claude

# 非交互式：直接向 Claude 提问（约 30 秒响应）
./sandbox start claude -- -p "你的问题"

# 启动交互式 Shell
./sandbox start zsh
./sandbox start bash

# 启动其他 CLI 工具
./sandbox start node
./sandbox start npm -- test
\`\`\`

> **注意**：命令与参数之间用 \`--\` 分隔，如 \`./sandbox start <command> -- <args>\`。

### 截图

\`\`\`bash
# 自动发现沙箱窗口并截图（保存为 PNG）
./sandbox screenshot -o screenshot.png

# 指定窗口 ID 截图
./sandbox screenshot --window-id 12345 -o screenshot.png
\`\`\`

### 其他命令

\`\`\`bash
# 列出所有可见窗口
./sandbox windows

# 关闭沙箱
./sandbox shutdown
\`\`\`

### 示例工作流

\`\`\`bash
# 1. 启动 Claude Code
./sandbox start claude

# 2. 等待 Claude 启动（约 10 秒）
sleep 10

# 3. 截图查看状态
./sandbox screenshot -o screenshot.png

# 4. 关闭沙箱
./sandbox shutdown
\`\`\`

\`\`\`bash
# 非交互式快速提问
./sandbox start claude -- -p "用 Python 写一个 hello world"
sleep 30
./sandbox screenshot -o claude_response.png
./sandbox shutdown
\`\`\`

## 三、架构

\`\`\`
sandbox start claude
       │
       ▼
CLI (sandbox)
       │ spawn System Test Sandbox.app --mode=cli --cmd=claude
       ▼
Tauri 沙箱窗口
  ┌────────────────────────────────────────────┐
  │  终端面板 (xterm.js)    │  Screenshot Preview │
  │  ← Claude 运行在这里     │                     │
  ├────────────────────────────────────────────┤
  │  Control Panel: Screenshot / Spawn / Click  │
  ├────────────────────────────────────────────┤
  │  Status: Server :5801 | Processes: X | ...  │
  └────────────────────────────────────────────┘
       │ HTTP :5801
       ▼
  内嵌 HTTP API (axum)
  - /screenshot, /input/click, /pty/write, ...
\`\`\`

## 四、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: 无法启动沙箱？**
A: 确保 \`System Test Sandbox.app\` 与 \`sandbox\` 在同一目录下。

**Q: 沙箱窗口内终端空白？**
A: 等待几秒让 Claude 启动，终端会自动连接 PTY 输出。

---

**版本**: v${VERSION} | **构建时间**: __BUILD_DATE__
RELEASEREADME

# Inject build date
sed -i '' "s/__BUILD_DATE__/${BUILD_DATE}/" "$RELEASE_DIR/README.md"

ok "README.md generated"

# --- done ---
echo ""
echo "=============================================="
echo " Release v${VERSION} built successfully!"
echo " Artifacts -> $RELEASE_DIR"
echo "=============================================="
ls -lh "$RELEASE_DIR"
echo ""
echo "  $(du -sh "$RELEASE_DIR" | cut -f1) total"
echo ""
