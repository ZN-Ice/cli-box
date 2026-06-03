#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# cli-box — Release Build Script
# ============================================================
# Builds the Electron sandbox app + CLI binary and packages
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
APP_NAME="CLI Box"

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

# Kill Electron app by exact process name
pkill -x "CLI Box" 2>/dev/null || true

# Kill CLI processes by exact binary name
pkill -x "sandbox" 2>/dev/null || true
pkill -x "sandbox-daemon" 2>/dev/null || true

rm -f ~/.sandbox/instances/*.json 2>/dev/null || true
ok "Cleanup done"

# --- step 3: build CLI + daemon binaries (release) ---
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

# --- step 4: build Electron app ---
echo ""
info "Building Electron app..."
cd "$SCRIPT_DIR/electron-app"
pnpm install --silent 2>&1 | tail -1
pnpm build 2>&1 | tail -5

info "Packaging Electron app..."
ELECTRON_MIRROR="${ELECTRON_MIRROR:-https://npmmirror.com/mirrors/electron/}" pnpm run pack 2>&1 | tail -10

# Find the built .app bundle
ELECTRON_BUNDLE=""
for dir in \
    "$SCRIPT_DIR/electron-app/dist/electron/mac-arm64/${APP_NAME}.app" \
    "$SCRIPT_DIR/electron-app/dist/electron/mac/${APP_NAME}.app" \
    "$SCRIPT_DIR/dist/electron/mac-arm64/${APP_NAME}.app" \
    "$SCRIPT_DIR/dist/electron/mac/${APP_NAME}.app"; do
    if [ -d "$dir" ]; then
        ELECTRON_BUNDLE="$dir"
        break
    fi
done

if [ -z "$ELECTRON_BUNDLE" ]; then
    err "Electron app bundle not found. Check electron-builder output."
fi
ok "Electron app built: $(du -sh "$ELECTRON_BUNDLE" | cut -f1)"

# --- step 5: assemble release folder ---
echo ""
info "Assembling release artifacts -> $RELEASE_DIR"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# Copy CLI
cp "$CLI_BIN" "$RELEASE_DIR/sandbox"
chmod +x "$RELEASE_DIR/sandbox"
codesign --force --sign - "$RELEASE_DIR/sandbox" 2>/dev/null || true
ok "sandbox CLI binary"

# Copy daemon (standalone copy for CLI to discover)
cp "$DAEMON_BIN" "$RELEASE_DIR/sandbox-daemon"
chmod +x "$RELEASE_DIR/sandbox-daemon"
codesign --force --sign - "$RELEASE_DIR/sandbox-daemon" 2>/dev/null || true
ok "sandbox-daemon binary"

# Copy Electron app bundle
cp -R "$ELECTRON_BUNDLE" "$RELEASE_DIR/${APP_NAME}.app"
ok "${APP_NAME}.app bundle (Electron)"

# --- step 6: generate README ---
echo ""
info "Generating README.md..."

BUILD_DATE="$(date '+%Y-%m-%d %H:%M')"

cat > "$RELEASE_DIR/README.md" << 'RELEASEREADME'
# CLI Box — Release v${VERSION}

macOS 桌面自动化沙箱。通过 CLI 启动 Electron 沙箱窗口，内置 xterm.js 终端运行命令行工具（如 Claude Code），支持截图和输入模拟。

## 文件说明

```
release/
├── sandbox                     # CLI 工具（命令行入口）
├── sandbox-daemon              # 守护进程（CLI 自动管理）
├── CLI Box.app/    # Electron 沙箱 macOS 应用
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

将 \`sandbox\` 和 \`CLI Box.app\` 添加进去并勾选。

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
# 截取指定沙箱窗口
./sandbox screenshot --id <sandbox-id> -o screenshot.png
\`\`\`

### 其他命令

\`\`\`bash
# 列出所有沙箱
./sandbox list

# 查看沙箱详情
./sandbox inspect <sandbox-id>

# 关闭沙箱
./sandbox close <sandbox-id>
\`\`\`

### 示例工作流

\`\`\`bash
# 1. 启动 Claude Code（自动打开 Electron 窗口）
./sandbox start claude

# 2. 等待 Claude 启动（约 10 秒）
sleep 10

# 3. 截图查看状态
./sandbox screenshot --id <ID> -o screenshot.png

# 4. 启动另一个沙箱（自动创建新 Tab）
./sandbox start zsh

# 5. 列出所有沙箱
./sandbox list

# 6. 关闭指定沙箱
./sandbox close <ID>
\`\`\`

## 三、架构

\`\`\`
sandbox start claude
       │
       ▼
CLI (sandbox)
       │ 1. 启动 sandbox-daemon（如未运行）
       │ 2. 通过 HTTP 创建沙箱
       │ 3. 启动 Electron 窗口（如未运行）
       ▼
sandbox-daemon (HTTP :15801)
  - 管理 PTY 进程
  - 提供截图/输入 API
  - WebSocket PTY 终端
       │
       ▼
Electron 窗口 (Chromium)
  ┌────────────────────────────────────┐
  │  Tab: claude   Tab: zsh   Tab: ... │
  ├────────────────────────────────────┤
  │  xterm.js 终端                      │
  │  ← PTY WebSocket 连接              │
  │  标准 term.write() 渲染             │
  └────────────────────────────────────┘
\`\`\`

## 四、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: 无法启动沙箱？**
A: 确保 \`CLI Box.app\` 与 \`sandbox\` 在同一目录下。

**Q: 沙箱窗口内终端空白？**
A: 等待几秒让 CLI 工具启动，终端会自动连接 PTY 输出。

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
