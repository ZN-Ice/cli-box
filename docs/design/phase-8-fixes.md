# Phase 8 — Release Test Bug 修复方案

**日期**: 2026-05-17  
**状态**: Draft  

---

## B1: `capture_region` 按 x/y 裁剪

### 问题

```rust
// capture/mod.rs:48
pub fn capture_region(_x: i32, _y: i32, width: u32, height: u32) -> Result<Vec<u8>> {
    // _x, _y 从未使用
    // 创建全显示器 filter，width/height 只缩放不裁剪
}
```

### 方案

使用 `image` crate（已是依赖）做软件裁剪：

1. 获取第一个显示器的完整尺寸
2. 以显示器原生分辨率截图
3. 用 `image::imageops::crop` 裁剪到 `(x, y, width, height)`
4. 编码为 PNG

```
capture_region(x, y, w, h):
  1. SCShareableContent::get() → 取第一个 display
  2. SCContentFilter 基于 display
  3. SCStreamConfiguration 使用 display 原生宽高
  4. SCScreenshotManager::capture_image → RGBA 全屏数据
  5. image::RgbaImage::from_raw(display_w, display_h, rgba)
  6. image::imageops::crop(x, y, w, h) 
  7. 编码裁剪后的图像为 PNG
```

**边界处理**：若 `x + width > display_width` 或 `y + height > display_height`，裁剪到显示器边界。

---

## B2: `window_id` 传播

### 问题

```
Tauri SandboxState.window_id  ← init_sandbox() 可设置（但前端不调用）
HTTP  AppState.window_id      ← 初始化 None，永不更新
```

两个 state 隔离，HTTP `/screenshot` 永远拿不到 window_id。

### 方案

#### Step 1: 在 Tauri setup 中自动发现窗口 ID

```
src-tauri/src/main.rs setup():
  // 当 Tauri 窗口创建后，延迟 1s 等待窗口渲染
  tauri::async_runtime::spawn(async {
    sleep(1s).await;
    
    // 用 ScreenCaptureKit 按标题查找窗口
    let window_id = ScreenCapture::find_window_by_title("CLI Box");
    
    // 设置到 HTTP AppState
    if let Some(id) = window_id {
      http_state.lock().await.window_id = Some(id);
    }
    
    // 也设置到 Sandbox state（通过 Tauri 命令或直接访问）
  });
```

#### Step 2: 添加 `set_window_id` HTTP 端点（可选）

```
POST /window/set  { "window_id": 12345 }
→ 更新 AppState.window_id
```

这允许前端或外部工具主动注入窗口 ID。

#### Step 3: 让 `sandbox-cli screenshot --id` 传递 `?window_id=N`

当 CLI 客户端请求沙箱截图时，自动带上 `window_id` 查询参数：

```rust
// client.rs screenshot()
let url = format!("http://127.0.0.1:{port}/screenshot?window_id={wid}");
```

但这需要 CLI 先知道 window_id。可以在 `start` 命令中，等 Tauri 窗口启动后自动获取 window_id 并写入 instance registry。

---

## B3+B5: 前端 API 连接

### 当前状态

```
main.tsx handlers:
  handleScreenshot   → setScreenshotCount(c => c+1)  // 假操作
  handleSpawnApp     → setProcesses([...fake...])      // 假进程
  handleSpawnCli     → setProcesses([...fake...])      // 假进程
  handleClick        → // 空
  handleTypeText     → // 空
  handlePressKey     → // 空
  handleTerminalInput → // 空
```

### 方案

#### Step 1: 创建 `sandbox-web/src/api.ts`

定义沙箱 API 客户端，封装 HTTP 调用：

```typescript
const BASE_URL = `http://127.0.0.1:${getPort()}`;

export async function health(): Promise<HealthResponse>;
export async function takeScreenshot(): Promise<Blob>;
export async function takeScreenshotRegion(x, y, w, h): Promise<Blob>;
export async function click(x, y, button): Promise<void>;
export async function typeText(text): Promise<void>;
export async function pressKey(key, modifiers): Promise<void>;
export async function spawnApp(path): Promise<ProcessInfo>;
export async function spawnCli(command, args): Promise<ProcessInfo>;
export async function listProcesses(): Promise<ProcessInfo[]>;
export async function killProcess(pid): Promise<void>;
export async function ptyWrite(pid, data): Promise<void>;
export async function ptyRead(pid): Promise<string | null>;
```

端口优先从 URL search params 读取，fallback 到 `5801`。

#### Step 2: 替换 `main.tsx` 中的空桩

```typescript
const handleScreenshot = useCallback(async () => {
  setScreenshotLoading(true);
  try {
    const blob = await api.takeScreenshot();
    // 触发下载或显示预览
    const url = URL.createObjectURL(blob);
    // ... 处理截图结果
    setScreenshotCount((c) => c + 1);
  } catch (e) {
    console.error("Screenshot failed:", e);
  } finally {
    setScreenshotLoading(false);
  }
}, []);
```

其他 handler 类似用真实 API 调用替换。

#### Step 3: 连接 PTY 终端

在 `Terminal.tsx` 中：
- 添加 `useEffect` 定时轮询 `/pty/output/:pid`（200ms 间隔）
- `term.onData` 回调调用 `api.ptyWrite(pid, data)`
- 新增 `pid` prop 用于指定 PTY 进程

```typescript
// Terminal.tsx
useEffect(() => {
  if (!pid) return;
  const interval = setInterval(async () => {
    const result = await api.ptyRead(pid);
    if (result?.output) {
      term.write(result.output);
    }
  }, 200);
  return () => clearInterval(interval);
}, [pid]);
```

---

## B4: `spawn_app` 窗口追踪

### 问题

`spawn_app` 使用 `open` 启动独立应用，不追踪窗口 ID。

### 方案（最小改动）

在 `spawn_app` 后延迟 500ms，用 ScreenCaptureKit 搜索应用窗口并按标题匹配：

```rust
pub fn spawn_app_with_window(app_path: &str) -> Result<(ProcessInfo, Option<u32>)> {
    let info = Self::spawn_app(app_path)?;
    // 等待窗口出现
    std::thread::sleep(Duration::from_millis(500));
    // 按应用名搜索窗口
    let app_name = Path::new(app_path).file_stem()...;
    let window_id = ScreenCapture::find_window_by_title(&app_name).ok();
    Ok((info, window_id))
}
```

同时在 `src-tauri/src/main.rs` 的 app 模式下，自动调用 `find_window_by_title` 发现窗口并按标题关联。

> **注意**：真正嵌入 macOS 应用到 Tauri webview 在技术上不可行。替代方案是追踪应用的 SCWindow ID，后续截图用 `capture_window(app_window_id)` 截取应用窗口。

---

## B6: 沙箱相对坐标区域截图

### 方案

新增 `screenshot_sandbox_region` HTTP 端点和 MCP tool，接受相对于沙箱窗口的坐标：

```
GET /screenshot/sandbox-region?x=10&y=20&width=300&height=200
```

实现：
1. 获取沙箱窗口的 frame origin (窗口在屏幕上的位置)
2. 将沙箱相对坐标转为全局坐标：`global_x = sandbox_frame.x + x`
3. 调用 `capture_region(global_x, global_y, width, height)`

```rust
async fn screenshot_sandbox_region_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Query(q): Query<RegionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let window_id = q.window_id.or(state.lock().await.window_id)
        .ok_or(AppError::BadRequest("No sandbox window"))?;
    
    // 获取沙箱窗口在屏幕上的位置
    let content = SCShareableContent::get()...;
    let window = content.windows().iter().find(|w| w.window_id() == window_id)...;
    let frame = window.frame();
    
    let global_x = frame.x as i32 + q.x;
    let global_y = frame.y as i32 + q.y;
    
    ScreenCapture::capture_region(global_x, global_y, q.width, q.height)
}
```
