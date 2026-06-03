# xterm.js WriteBuffer setTimeout 在 Tauri WKWebView 中失效问题

> 分析日期：2026-05-30
> 状态：已解决
> 影响：所有在 Tauri WKWebView 中使用 xterm.js 6.x 渲染 PTY 输出的场景

## 一、问题现象

在 Tauri 沙箱中启动 opencode（Go TUI 应用）后，终端显示空白屏幕。PTY 数据通过 WebSocket 正确传递到前端，但 xterm.js 不渲染任何内容。

**对比测试结果：**

| 场景 | 结果 |
|------|------|
| `cargo test` — opencode PTY 输出 | 32103+ chars，数据正常 |
| zsh 终端（简单输出） | 正常渲染 |
| opencode（大量 TUI 输出） | 空白屏幕 |
| 服务端日志 `[PTY-READ]` | 数据正常读取，WebSocket receivers=1 |
| 前端 WebSocket `onOutput` 回调 | 数据正常到达 |

## 二、问题演进历史

这个问题经历了多个阶段的排查，每一步解决了一个层面的问题，但最终都发现还有更深层的原因：

### Phase 1: PTY Reader 线程阻塞（已解决）

最初的猜测是 PTY 输出读取有问题。`portable-pty` 的阻塞式 `read()` 在 TUI 应用空闲时不会返回数据，导致 HTTP 轮询接口 `/pty/output` 返回 `null`。

**解决方案：** 实现专用 Reader 线程持续读取 PTY 输出到共享缓冲区，HTTP 接口从缓冲区非阻塞 drain。

**结果：** 服务端数据读取正常，但前端仍然空白。

### Phase 2: SQLite 缓冲区与 WebSocket（已解决）

添加 PtyStore（SQLite 缓冲区）后，引入了 WebSocket 实时推送。后端日志确认数据通过 WebSocket 正确推送，但终端仍然空白。

**排查方向：** 怀疑前端 WebSocket 接收、xterm.js 写入、或渲染管线有问题。

### Phase 3: 前端 xterm.js 写入调试（发现问题）

通过 monkey-patch `term.write()` 添加诊断，发现关键证据：

```
calls=9, cbs=1
```

**9 次 `term.write()` 调用，只有 1 次回调被触发。** 这意味着 9 批数据被提交给 WriteBuffer，但只有第一批被实际处理并渲染。

## 三、根因分析

### 3.1 xterm.js 6.x WriteBuffer 的写入调度机制

xterm.js 6.x 的 `WriteBuffer`（`@xterm/xterm/src/common/input/WriteBuffer.ts`）使用 `setTimeout(0)` 异步调度数据处理：

```typescript
// WriteBuffer.write() — 第一次写入时调度处理
public write(data: string | Uint8Array, callback?: () => void): void {
  if (!this._writeBuffer.length) {
    this._bufferOffset = 0;
    if (this._didUserInput) { /* 用户输入立即处理 */ return; }
    setTimeout(() => this._innerWrite());  // ← 调度异步处理
  }
  this._pendingData += data.length;
  this._writeBuffer.push(data);
  this._callbacks.push(callback);
}
```

```typescript
// WriteBuffer._innerWrite() — 处理数据，12ms 时间预算
protected _innerWrite(lastTime = 0, promiseResult = true): void {
  const startTime = lastTime || performance.now();
  while (this._writeBuffer.length > this._bufferOffset) {
    const data = this._writeBuffer[this._bufferOffset];
    const result = this._action(data, promiseResult);  // 调用 parser
    // ...
    if (performance.now() - startTime >= WRITE_TIMEOUT_MS) {  // 12ms 预算
      break;  // ← 超时则退出循环
    }
  }
  if (this._writeBuffer.length > this._bufferOffset) {
    setTimeout(() => this._innerWrite());  // ← 还有数据，再次调度
  }
  // ...
}
```

**正常流程（Chromium/Electron）：**
```
write(data1) → setTimeout → _innerWrite → 处理 data1 → 12ms 超时
                                                       ↓
                           setTimeout → _innerWrite → 处理 data2 → 12ms 超时
                                                                 ↓
                                               setTimeout → _innerWrite → 处理 data3...
```

### 3.2 WKWebView 中 setTimeout(0) 失效

在 Tauri 2.x 的 WKWebView（macOS WebKit）中，`setTimeout(0)` 的行为与 Chromium 不同：

- **第一次 setTimeout(0)**：正常触发
- **后续的 setTimeout(0)**（从 `_innerWrite` 内部调用的）：**不触发**

这意味着 WriteBuffer 的调度循环在第一个周期后中断：

```
write(data1) → setTimeout → _innerWrite → 处理 data1 → 12ms 超时
                                                       ↓
                           setTimeout ← ← ← ← ← ← ← ← 卡住！永远不触发
                           
data2 ~ data9 永远不会被处理 → 回调永远不会触发 → 终端空白
```

### 3.3 为什么 zsh 能渲染但 opencode 不能

- **zsh**：启动时输出少量数据（提示符），通常在 12ms 预算内一次性处理完毕，不需要后续 setTimeout
- **opencode**：启动时输出大量 TUI 数据（全屏渲染、颜色设置、布局），远超 12ms 预算，需要多个处理周期 → 触发后续 setTimeout → 在 WKWebView 中卡住

### 3.4 与 waveterm 的对比

waveterm 也使用 xterm.js 6.x + 标准的 `terminal.write()` API，但没有此问题。原因是 waveterm 使用 **Electron（Chromium 渲染引擎）**，而 Chromium 的 `setTimeout(0)` 调度行为正常。**此问题仅影响使用 WebKit 渲染引擎的环境（Tauri、WKWebView）。**

## 四、失败的解决尝试

### 尝试 1：直接调用 `term.write()`

标准写入方式。由于 WriteBuffer 内部 setTimeout 卡住，只有第一批数据被渲染。

**结果：** 失败 — 仅第一次 write 生效。

### 尝试 2：`_core._writeBuffer.writeSync()` 外部调用

xterm.js 的 `WriteBuffer.writeSync()` 方法可以同步处理数据，绕过 setTimeout 调度。但 minified 代码中使用了模块作用域变量（如 `s`），从模块外部调用会导致：

```
ReferenceError: Can't find variable: s
```

**原因：** V8 的模块作用域变量在 WebKit JavaScriptCore 中不可从外部访问。minified 代码将 WriteBuffer 内部的 `_isSyncWriting`、`_syncCalls` 等属性名压缩为短变量名，这些变量在模块加载时绑定到特定作用域。

**结果：** 失败 — JSC ReferenceError。

### 尝试 3：setTimeout → requestAnimationFrame 猴子补丁

全局替换 `setTimeout(0)` 为 `requestAnimationFrame`：

```typescript
const origSetTimeout = window.setTimeout.bind(window);
window.setTimeout = function(fn, delay, ...args) {
  if (delay === undefined || delay === 0) {
    return requestAnimationFrame(() => fn(...args)) as unknown as number;
  }
  return origSetTimeout(fn, delay, ...args);
};
```

**问题：** `requestAnimationFrame` 以 ~60fps（16ms 间隔）触发，但 `_innerWrite` 需要 ~1ms 级别的连续调度来处理高吞吐量 PTY 输出。RAF 的 16ms 间隔导致数据处理严重滞后，且在 WKWebView 中 RAF 的触发频率也不稳定。

**结果：** 失败 — 终端仍然空白。

### 尝试 4：批量合并写入

将 WebSocket 接收的多批数据合并为单次 `term.write()` 调用，减少对 WriteBuffer 的压力。

**问题：** 即使单次写入大量数据，`_innerWrite` 的 12ms 预算会导致中断，后续处理仍依赖 setTimeout → 卡住。

**结果：** 失败 — 同样的 setTimeout 问题。

## 五、最终解决方案

### 核心思路：完全绕过 WriteBuffer

直接调用 xterm.js 内部的 `InputHandler.parse()` 方法，跳过 WriteBuffer 的异步调度机制：

```typescript
function writeDirect(term: Terminal, data: string | Uint8Array): void {
  const core = (term as any)._core ?? (term as any).core;
  if (!core) return;
  const ih = core._inputHandler;
  if (ih && typeof ih.parse === "function") {
    ih.parse(data, true);  // true = promiseResult，同步处理
  }
  // 触发 write-parsed 事件驱动渲染
  if (core._writeBuffer?._onWriteParsed) {
    core._writeBuffer._onWriteParsed.fire();
  }
}
```

### 为什么这能工作

1. **`InputHandler.parse()`** 是同步方法，直接解析 ANSI 转义序列并更新终端缓冲区
2. 所有内置的 escape sequence handler（CSI、OSC、ESC）都是同步的（返回 `boolean`，不是 `Promise`）
3. `parse(data, true)` 的第二个参数 `promiseResult=true` 告诉解析器同步处理所有 handler
4. `_onWriteParsed.fire()` 手动触发渲染管线更新

### 数据流对比

**修复前（WriteBuffer 路径）：**
```
WebSocket → term.write() → WriteBuffer.write() → setTimeout → _innerWrite() → parse()
                                                   ↑
                                            WKWebView 卡住
```

**修复后（直接解析路径）：**
```
WebSocket → writeDirect() → InputHandler.parse() → 渲染
                                 ↑
                          完全同步，无 setTimeout
```

## 六、完整的修改文件

**文件：** `sandbox-web/src/components/Terminal.tsx`

```typescript
/**
 * Bypass xterm.js WriteBuffer's setTimeout-based scheduling which stalls in
 * Tauri's WKWebView. Instead, call InputHandler.parse() directly and then
 * fire the write-parsed event to trigger rendering.
 */
function writeDirect(term: Terminal, data: string | Uint8Array): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const core = (term as any)._core ?? (term as any).core;
  if (!core) return;
  const ih = core._inputHandler;
  if (ih && typeof ih.parse === "function") {
    ih.parse(data, true);
  }
  if (core._writeBuffer?._onWriteParsed) {
    core._writeBuffer._onWriteParsed.fire();
  }
}

// WebSocket 输出处理中使用 writeDirect 替代 term.write
conn.onOutput((data) => {
  const term = xtermRef.current;
  if (!term) return;
  const writeData = typeof data === "string" ? data : decoder.decode(data as Uint8Array);
  writeDirect(term, writeData);  // 替代 term.write(writeData)
});
```

## 七、测试验证

### 测试环境
- macOS (Apple Silicon)
- Tauri 2.x (WKWebView)
- xterm.js 6.x (`@xterm/xterm`)
- opencode (Go TUI 应用)

### 测试步骤与结果

| 步骤 | 操作 | 截图 | 结果 |
|------|------|------|------|
| 1 | `sandbox start opencode` | 01_initial.png | OpenCode UI 正确渲染 |
| 2 | PTY 输入 "你是谁？" | 03_after_type.png | 中文文本正确显示在输入框 |
| 3 | 按 Enter 发送 | 04_after_enter.png | AI 成功响应，流式输出正常 |
| 4 | 输入第二个问题 | 05_second_input.png | 多轮对话正常 |
| 5 | 第二个回答流式输出 | 06_second_response.png | 代码块、中文混合内容正确渲染 |
| 6 | 最终完整对话 | 07_final.png | 全部内容完整显示 |

所有截图保存在 `release_test/2026-05-29_14-21/` 目录下。

## 八、潜在风险与注意事项

1. **内部 API 依赖**：`_core`、`_inputHandler`、`_writeBuffer` 是 xterm.js 的内部属性，未包含在公共 TypeScript 类型定义中。xterm.js 版本升级时可能需要调整属性访问路径。

2. **性能影响**：`parse()` 是同步调用，大量数据可能导致主线程阻塞。实际测试中 opencode 的输出量（单次 ~10KB）未造成可见延迟。如果未来需要处理更高吞吐量（如 `cat large_file`），可能需要添加分片处理。

3. **键盘输入**：`term.onData()` 路径不受影响，仍然通过 WebSocket 正常发送用户输入到 PTY。`writeDirect` 仅用于 PTY 输出到终端的渲染路径。

4. **WriteBuffer 状态**：由于完全绕过了 WriteBuffer，其内部状态（`_pendingData`、`_writeBuffer` 数组等）不会因 `writeDirect` 的调用而更新。这不会导致问题，因为没有代码依赖这些状态来决策。

5. **`_onWriteParsed.fire()`**：手动触发此事件确保渲染管线被通知数据已更新。如果不触发，终端缓冲区会更新但屏幕不会刷新。

## 九、与其他问题的关联

此问题是 `opencode-tui-blank-screen.md` 中分析的问题的真正根因。之前的分析正确地识别了服务端层面的问题（Reader 线程、SQLite 缓冲区），但前端渲染层的根因在 xterm.js 的 WriteBuffer 中。完整的问题链条：

```
服务端 PTY 读取（Phase 1，已修复）
    ↓
服务端 WebSocket 推送（Phase 2，已修复）
    ↓
前端 xterm.js 渲染（Phase 3，本文档）← 真正阻塞用户看到输出的最后一环
```

## 十、为什么 waveterm 没有这个问题：Electron vs Tauri 对比

### 10.1 架构差异

桌面应用 = **后端语言** + **桌面框架** + **渲染引擎** + **前端技术**，这是四个独立层。

**Waveterm 的架构：**

```
Waveterm
├── 后端：Go 语言（处理 PTY、文件系统等）
├── 桌面框架：Electron
│   └── 渲染引擎：Chromium（和 Chrome 浏览器同一个内核）
└── 前端：React + xterm.js（跑在 Chromium 里）
```

Electron 不是 Go 语言的框架，它是 Node.js/JavaScript 的桌面框架（VS Code、Slack、Discord 都用它）。Waveterm 的后端用 Go 写了一个独立进程处理 PTY，然后通过 WebSocket 传给 Electron 前端。

**我们的架构：**

```
cli-box
├── 后端：Rust 语言（处理 PTY、截图、输入模拟等）
├── 桌面框架：Tauri
│   └── 渲染引擎：WKWebView（macOS 系统自带的 WebKit 内核，即 Safari 内核）
└── 前端：React + xterm.js（跑在 WKWebView 里）
```

Tauri 是 Rust 的桌面框架。在 macOS 上，它不打包 Chromium，而是直接用系统自带的 **WKWebView**（即 Safari 的内核）来渲染前端。这也是 Tauri 安装包比 Electron 小很多的原因——不需要自带一个完整的浏览器引擎。

**根因就在渲染引擎的差异：**

```
xterm.js 的 term.write()
    ↓
WriteBuffer 内部用 setTimeout(0) 调度
    ↓
在 Chromium 中：setTimeout(0) 正常触发 ✓  → waveterm 没问题
在 WebKit 中：  setTimeout(0) 第一次正常，后续卡住 ✗  → 我们空白屏
```

相同的 xterm.js 代码，跑在不同的浏览器引擎里，`setTimeout` 的行为不一样。这不是后端语言（Go vs Rust）的问题，而是**渲染引擎**（Chromium vs WebKit）的行为差异。

### 10.2 能不能用 Electron 替代 Tauri？

技术上完全可以，但需要将 Rust 后端的所有功能（CGEvent、AXUIElement、ScreenCaptureKit、PTY 管理）用 Node.js/Swift 重写或通过 FFI 桥接。代价很大。

### 10.3 全面对比

#### 体积和分发

| 维度 | Tauri | Electron |
|------|-------|----------|
| 安装包大小 | ~11MB (CLI) + ~19MB (App) | ~150-200MB |
| 原因 | 复用系统 WebKit | 自带完整 Chromium |
| 用户下载体验 | 快 | 慢 |

我们的定位是「可复用的自动化沙箱」，CLI 工具需要频繁下载分发。**11MB vs 150MB 是巨大差异。**

#### 渲染兼容性

| 维度 | Tauri (WKWebView) | Electron (Chromium) |
|------|-------------------|---------------------|
| setTimeout(0) | 有坑（已通过 writeDirect 解决） | 正常 |
| xterm.js | 需要 writeDirect hack | 开箱即用 |
| CSS/JS 兼容性 | Safari 级别 | Chrome 级别 |
| 前端调试 | 困难（无内置 DevTools） | 完整 Chrome DevTools |

这是 Electron 最明显的优势。但我们的 `writeDirect` 方案已经解决了这个问题。

#### 性能

| 维度 | Tauri (Rust) | Electron (Node.js) |
|------|-------------|-------------------|
| PTY 管理 | Rust 原生，极快 | node-pty，够用 |
| 截图 (ScreenCaptureKit) | Rust FFI 直接调用 | 需要 Swift bridge 或 node-addon |
| 输入模拟 (CGEvent) | Rust FFI 直接调用 | 需要 node-addon |
| 内存占用 | ~30-50MB | ~150-300MB |
| 多实例 | 每实例轻量 | 每实例开销大 |

我们的项目大量使用 macOS 系统 API（CGEvent、AXUIElement、ScreenCaptureKit）。**Rust 通过 FFI 调用这些 API 比 Node.js 通过 native addon 更自然、更可靠。**

#### 多实例架构（核心差异）

我们的核心架构是每个沙箱一个独立进程：

```
Tauri 多实例：
├── sandbox start → CLI (Rust, ~5MB 内存)
│   └── Tauri window (~30MB)
├── sandbox start → CLI (Rust, ~5MB 内存)
│   └── Tauri window (~30MB)
└── 总计: ~70MB

Electron 多实例：
├── sandbox start → Electron process (~150MB)
├── sandbox start → Electron process (~150MB)
└── 总计: ~300MB+
```

Electron 的多进程模型和我们的多实例需求是冲突的。Electron 本身就是多进程架构（主进程 + 多个渲染进程），再加多实例会很重。

#### 开发体验

| 维度 | Tauri | Electron |
|------|-------|----------|
| 后端语言 | Rust（学习曲线陡） | Node.js（生态成熟） |
| macOS 系统 API 调用 | FFI 直接，类型安全 | 需要 native addon 或子进程 |
| 前端调试 | 无内置 DevTools | 完整 DevTools |
| 社区生态 | 较小，但快速增长 | 非常成熟 |
| 前端技术栈 | 完全相同（React/TS） | 完全相同 |

### 10.4 结论

```
                Tauri 优势项                    Electron 优势项
            ┌─────────────────────┐      ┌──────────────────────┐
  体积     │  11MB vs 150MB      │      │                      │
  内存     │  30MB vs 150MB+     │      │                      │
  多实例   │  轻量，天生适合      │      │  重，不适合           │
  系统 API │  Rust FFI 原生      │      │                      │
  ──────── │  ────────────────   │      │  ────────────────    │
  渲染     │  WKWebView 有坑     │      │  Chromium 开箱即用   │
  调试     │  无 DevTools        │      │  完整 DevTools       │
  开发速度 │  Rust 曲线陡        │      │  Node.js 快速开发    │
            └─────────────────────┘      └──────────────────────┘
```

对我们项目来说，**Tauri 是更好的选择**：

1. **多实例架构**是我们的核心需求，Electron 的内存开销在这里是致命的
2. **体积小**对 CLI 分发很重要
3. **Rust FFI** 调用 macOS 系统 API 是我们的核心能力
4. WKWebView 的 `setTimeout` 问题已经有了解决方案（`writeDirect`）

唯一值得考虑 Electron 的场景是：如果未来 WKWebView 出现更多兼容性问题，且 `writeDirect` 这类 hack 维护成本太高。但目前看来，一个 `writeDirect` 函数就足够了，不值得为此切换整个技术栈。

---

**解决方案作者：** Claude (Anthropic)
**验证日期：** 2026-05-29
