#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# system-test-sandbox — Release Build Script
# ============================================================
# Builds both the CLI binary and the macOS app, then packages
# them into ./release/ for distribution.
#
# Prerequisites:
#   - Rust >= 1.88
#   - Node.js + pnpm
#   - macOS (Apple Silicon or Intel)
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

RELEASE_DIR="$SCRIPT_DIR/release"
VERSION="0.1.0"

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
echo " system-test-sandbox v${VERSION} — Release Build"
echo "=============================================="
echo ""

# --- step 1: check prerequisites ---
info "Checking prerequisites..."
check rustc
check cargo
check node
check pnpm
ok "All prerequisites met"

# --- step 2: build frontend ---
echo ""
info "Building frontend (sandbox-web)..."
cd "$SCRIPT_DIR/sandbox-web"

if [ ! -d "node_modules" ]; then
    pnpm install --frozen-lockfile
fi
pnpm build
ok "Frontend built -> sandbox-web/dist/"

# --- step 3: build CLI binary ---
echo ""
info "Building CLI binary (release)..."
cd "$SCRIPT_DIR"
cargo build --release -p sandbox-cli
CLI_BIN="$SCRIPT_DIR/target/release/sandbox"
ok "CLI binary built: $(du -h "$CLI_BIN" | cut -f1)"

# --- step 4: build Tauri app ---
echo ""
info "Building Tauri desktop app..."

cd "$SCRIPT_DIR"

# Try cargo-tauri if installed, otherwise install it
if ! cargo tauri --version &>/dev/null; then
    info "Installing tauri-cli (one-time) ..."
    cargo install tauri-cli --version "^2"
fi

APP_NAME="System Test Sandbox"
cargo tauri build --target universal-apple-darwin 2>/dev/null || cargo tauri build

TAURI_BUILD_DIR="$SCRIPT_DIR/target/release/bundle/macos"
APP_BUNDLE="$TAURI_BUILD_DIR/${APP_NAME}.app"
DMG_PATH="$SCRIPT_DIR/target/release/bundle/dmg"

if [ -d "$APP_BUNDLE" ]; then
    ok "Tauri app built: $APP_BUNDLE"
else
    # Fallback: manually assemble .app from cargo binary
    info "Manually assembling .app bundle..."
    cargo build --release -p system-test-sandbox
    APP_BUNDLE="$TAURI_BUILD_DIR/${APP_NAME}.app"
    mkdir -p "$APP_BUNDLE/Contents/MacOS"
    mkdir -p "$APP_BUNDLE/Contents/Resources"
    cp "$SCRIPT_DIR/target/release/system-test-sandbox" "$APP_BUNDLE/Contents/MacOS/"
    # Copy Info.plist if exists
    if [ -f "$SCRIPT_DIR/src-tauri/Info.plist" ]; then
        cp "$SCRIPT_DIR/src-tauri/Info.plist" "$APP_BUNDLE/Contents/"
    fi
    # Copy icons
    if [ -f "$SCRIPT_DIR/src-tauri/icons/icon.icns" ]; then
        cp "$SCRIPT_DIR/src-tauri/icons/icon.icns" "$APP_BUNDLE/Contents/Resources/"
    fi
    ok ".app bundle manually assembled"
fi

# --- step 5: assemble release folder ---
echo ""
info "Assembling release artifacts -> $RELEASE_DIR"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# CLI binary
cp "$CLI_BIN" "$RELEASE_DIR/sandbox"
chmod +x "$RELEASE_DIR/sandbox"

# Fix rpath for Swift Concurrency (required by ScreenCaptureKit)
install_name_tool -add_rpath /usr/lib/swift "$RELEASE_DIR/sandbox" 2>/dev/null || true
ok "sandbox CLI binary"

# Tauri .app
if [ -d "$APP_BUNDLE" ]; then
    cp -R "$APP_BUNDLE" "$RELEASE_DIR/"
    # Fix rpath for the app binary too
    APP_EXEC="$RELEASE_DIR/${APP_NAME}.app/Contents/MacOS/system-test-sandbox"
    if [ -f "$APP_EXEC" ]; then
        install_name_tool -add_rpath /usr/lib/swift "$APP_EXEC" 2>/dev/null || true
    fi
    ok "$APP_NAME.app"
fi

# DMG installer
DMG_FILE=$(ls "$DMG_PATH"/*.dmg 2>/dev/null | head -1)
if [ -n "$DMG_FILE" ]; then
    cp "$DMG_FILE" "$RELEASE_DIR/"
    ok "$(basename "$DMG_FILE")"
fi

# Generate README (inline, see step 6)
ok "Release artifacts ready"

# --- step 6: generate README ---
echo ""
info "Generating README.md..."

BUILD_DATE="$(date '+%Y-%m-%d %H:%M')"

cat > "$RELEASE_DIR/README.md" << 'RELEASEREADME'
# System Test Sandbox — Release v0.1.0

macOS 桌面自动化沙箱。模拟鼠标/键盘操作、截图、读取 UI 元素树，通过 CLI 或 MCP 协议供 Agent 工具调用。

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

## 二、CLI 使用方法

### 启动 HTTP + MCP 服务（最常用）

```bash
./sandbox serve --port 5801
```

启动后可用端点：

```
GET  http://127.0.0.1:5801/health              # 健康检查
GET  http://127.0.0.1:5801/screenshot           # 截取沙箱窗口 (PNG)
POST http://127.0.0.1:5801/input/click          # 鼠标点击
POST http://127.0.0.1:5801/input/type           # 键盘输入
POST http://127.0.0.1:5801/input/key            # 按键
POST http://127.0.0.1:5801/cli/spawn            # 启动 CLI 进程
POST http://127.0.0.1:5801/app/spawn            # 启动 macOS 应用
GET  http://127.0.0.1:5801/windows              # 列出窗口
GET  http://127.0.0.1:5801/processes            # 列出进程
GET  http://127.0.0.1:5801/ui/inspect/:window   # 读取 UI 树
```

### 启动 MCP 服务（供 Claude Code / Codex 调用）

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

### 一次性命令

```bash
# 截图
./sandbox screenshot -o result.png

# 列出所有窗口
./sandbox windows

# 模拟点击
./sandbox click 500 300

# 模拟打字
./sandbox type "Hello World"

# 模拟按键
./sandbox key Return
./sandbox key c --modifiers cmd

# 启动 App
./sandbox spawn-app /Applications/Calculator.app

# 启动 CLI
./sandbox spawn-cli ls -la

# 终止进程
./sandbox kill 12345
```

### curl 调用示例

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

# 按回车
curl -X POST http://127.0.0.1:5801/input/key \
  -H "Content-Type: application/json" \
  -d '{"key": "Return", "modifiers": []}'

# 启动 CLI
curl -X POST http://127.0.0.1:5801/cli/spawn \
  -H "Content-Type: application/json" \
  -d '{"command": "ls", "args": ["-la"]}'
```

## 三、桌面应用使用方法

1. 双击 `System Test Sandbox.app` 启动
2. 应用窗口内嵌 xterm.js 终端，可直接运行 CLI 命令
3. 顶部状态栏提供截图按钮

## 四、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: `serve` 端口被占用？**
A: 使用 `./sandbox serve --port 5802` 更换端口。

**Q: MCP 连接失败？**
A: 确认 `settings.json` 中的 `command` 路径是绝对路径。

---

**版本**: v0.1.0 | **构建时间**: __BUILD_DATE__
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
