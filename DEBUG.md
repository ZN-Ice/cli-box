# 沙箱调试指南

> 记录项目中排查问题时使用过的调试方法，方便后续复用。

## 一、前端调试

### 1.1 xterm.js 写入诊断（Monkey-patch term.write）

用于判断 xterm.js WriteBuffer 是否正确处理了写入数据。

```typescript
// 在 Terminal.tsx 中，初始化 term 后添加
let writeCalls = 0;
let writeCbs = 0;
const origWrite = term.write.bind(term);
(term as any).write = function (data: any, callback?: () => void) {
  writeCalls++;
  origWrite(data, () => {
    writeCbs++;
    console.log(`[DIAG-WRITE] calls=${writeCalls} cbs=${writeCbs}`);
    callback?.();
  });
};
```

**判断标准：**
- `calls === cbs`：所有写入都被处理，问题不在 WriteBuffer
- `calls > cbs`：部分写入丢失，WriteBuffer 调度有问题
- `calls=9, cbs=1`：只有第一次写入生效，后续 setTimeout 卡住

### 1.2 页面覆盖诊断面板

在 Tauri 中无法打开 DevTools，通过 DOM 覆盖层显示运行时状态。

```typescript
// 在 useEffect 中创建诊断元素
const diagEl = document.createElement("div");
diagEl.id = "diag-overlay";
diagEl.style.cssText =
  "position:fixed;top:10px;right:10px;z-index:9999;background:#222;" +
  "color:#0f0;padding:8px;font-size:12px;font-family:monospace;border-radius:4px;";
document.body.appendChild(diagEl);

// 定时更新内容
const interval = setInterval(() => {
  const term = xtermRef.current;
  const buf = term?.buffer.active;
  const line0 = buf?.getLine(0)?.translateToString(true)?.substring(0, 40) || "";
  diagEl.textContent = `状态信息: ${line0}`;
}, 2000);

// 清理
return () => {
  clearInterval(interval);
  document.getElementById("diag-overlay")?.remove();
};
```

**用途：** 查看终端缓冲区第一行内容、WebSocket 接收字节数、parse 调用次数等。

### 1.3 直接检查 xterm.js 内部对象

```typescript
const core = (term as any)._core ?? (term as any).core;
const inputHandler = core?._inputHandler;
const writeBuffer = core?._writeBuffer;

// 检查解析器是否存在
console.log("parse fn:", typeof inputHandler?.parse);  // "function"

// 检查缓冲区状态
console.log("buffer length:", writeBuffer?._writeBuffer?.length);
console.log("pending data:", writeBuffer?._pendingData);
```

### 1.4 WKWebView 缓存清理

Tauri 构建后前端资源可能被 WKWebView 缓存，修改前端代码后必须清理：

```bash
rm -rf ~/Library/Caches/com.cli-box*
```

### 1.5 强制 Tauri 重新嵌入前端资源

只运行 `cargo build` 不会更新嵌入的前端资源。必须满足以下任一条件：

```bash
# 方法 1：使用完整构建脚本
bash release.sh

# 方法 2：修改 build.rs 时间戳触发重构建
touch src-tauri/build.rs && cargo build --release -p cli-box
```

## 二、服务端调试

### 2.1 查看沙箱日志

每个沙箱实例启动时会打印日志路径。日志按日期和 sandbox_id 组织：

```
~/.cli-box/logs/<date>/<sandbox_id>.log.<date>
```

```bash
# 查看最新实例的日志
ls -lt ~/.cli-box/logs/$(date +%Y-%m-%d)/ | head -5

# 实时跟踪日志
tail -f ~/.cli-box/logs/2026-05-29/0dbeaf79.log.2026-05-29
```

**关键日志标签：**
- `[PTY-READ]` — PTY 输出读取（字节数、预览、接收者数量）
- `[setup]` — 沙箱启动流程
- `broadcast sent, receivers=N` — WebSocket 推送状态（N=0 说明没有前端连接）

### 2.2 通过 HTTP API 验证数据流

不依赖前端，直接用 curl 验证服务端各环节是否正常：

```bash
# 健康检查
curl http://127.0.0.1:5801/health

# 查看进程列表（确认 PTY 已 spawn）
curl http://127.0.0.1:5801/processes

# 就绪检查
curl http://127.0.0.1:5801/readyz

# 手动向 PTY 写入数据
curl -X POST http://127.0.0.1:5801/pty/write/1000 \
  -H "Content-Type: application/json" \
  -d '{"data": "echo hello\n"}'
```

### 2.3 检查实例注册状态

```bash
# 列出所有注册的沙箱实例
./release/cli-box list

# 查看实例注册文件
cat ~/.cli-box/instances/*.json
```

## 三、构建与发布调试

### 3.1 完整构建流程

```bash
# 标准发布构建（包含前端 + Tauri + CLI）
bash release.sh
```

构建顺序：`pnpm install → pnpm build → cargo tauri build → cargo build CLI → 复制到 release/`

### 3.2 验证构建产物

```bash
# 检查 CLI 版本
./release/cli-box --version

# 检查 Tauri app 大小
ls -lh release/cli-box 2>/dev/null || ls -lh "release/CLI Box.app"
```

## 四、端到端测试流程

### 4.1 标准测试步骤

```bash
# 1. 清理环境
pkill -f "cli-box" 2>/dev/null
rm -rf ~/Library/Caches/com.cli-box* 2>/dev/null

# 2. 构建并启动
bash release.sh
./release/cli-box start opencode

# 3. 等待启动，截图验证
sleep 10
SANDBOX_ID=$(./release/cli-box list | grep -o '^\S*' | head -1)
./release/cli-box screenshot --id $SANDBOX_ID -o test_screenshot.png

# 4. PTY 交互测试
./release/cli-box type --id $SANDBOX_ID "测试文本"
./release/cli-box key --id $SANDBOX_ID Return

# 5. 再次截图验证
sleep 3
./release/cli-box screenshot --id $SANDBOX_ID -o test_result.png

# 6. 清理
./release/cli-box close $SANDBOX_ID
```

### 4.2 输入路由说明

输入路由根据沙箱类型自动选择：
- **CLI 沙箱**（Claude Code、zsh 等）：自动使用 PTY 写入
- **App 沙箱**（GUI 应用）：自动使用 CGEvent 模拟

```bash
# CLI 沙箱中自动走 PTY 路径
./release/cli-box type --id xxx "你好"
./release/cli-box key --id xxx Return
```

## 五、常见问题排查

### 5.1 终端空白

1. 检查服务端日志 `receivers=N`：N=0 说明 WebSocket 未连接，N≥1 说明数据已推送
2. 添加 monkey-patch 诊断：检查 `calls` 和 `cbs` 是否匹配
3. 检查 `writeDirect` 是否生效：添加诊断覆盖层显示 parse 调用次数

### 5.2 前端代码修改不生效

1. 是否执行了 `release.sh`（而不是单独 `cargo build`）
2. 是否清理了 WKWebView 缓存
3. 确认 `electron-app/dist/` 目录的构建时间是最新的

### 5.3 WebSocket 连接失败

1. 检查沙箱实例是否在运行：`./release/cli-box list`
2. 检查端口是否正确：实例注册文件中的 port 字段
3. 检查服务端日志是否有错误

### 5.4 进程启动失败

1. 检查命令是否在 PATH 中：`which opencode`
2. 检查服务端日志中 `spawned cli` 或错误信息
3. 检查 PTY 环境变量是否正确设置（TERM=xterm-256color）
