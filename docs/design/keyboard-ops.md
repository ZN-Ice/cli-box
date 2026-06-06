# 键盘操作特性设计文档

> 目标：支持通过 CLI 命令在沙箱中进行键盘输入和按键操作，实现场景一（claude 回车→输入→回车）和场景二（zsh 输入命令→回车）。

## 一、现状分析

### 已有实现

| 层级 | 模块 | 状态 |
|------|------|------|
| Rust Core | `InputSimulator::type_text()` / `press_key()` | ✅ CGEvent 已实现，支持 target_pid 定向投递 |
| HTTP API | `POST /input/type` / `POST /input/key` | ✅ 端点已存在 |
| HTTP API | `POST /pty/write` | ✅ PTY 写入已存在 |
| HTTP API | `GET /processes` | ✅ 进程列表已存在 |
| 前端 | `api.ts` → `typeText()` / `pressKey()` | ✅ 前端 API 已封装 |
| 实例注册 | `InstanceRegistry` + `~/.sandbox/instances/` | ✅ 文件系统注册中心 |

### 缺失部分

| 缺失 | 说明 |
|------|------|
| CLI `type` 命令 | 无法通过 CLI 输入文本到沙箱 |
| CLI `key` 命令 | 无法通过 CLI 按键（Return/Tab 等）|
| CLI `list` 命令 | 无法列出所有沙箱实例 |
| CLI `close` 命令 | 无法通过 CLI 关闭指定沙箱 |
| CLI `inspect` 命令 | 无法查看沙箱详情 |
| CLI `click` 命令 | 无法通过 CLI 点击 |
| CLI `processes` 命令 | 无法列出沙箱内进程 |
| HTTP 客户端模块 | CLI 无法与沙箱实例通信 |

## 二、输入投递策略

沙箱有两种运行模式，键盘输入需要不同的投递路径：

### CLI 模式（PTY）

```
CLI command: sandbox-cli type --id abc123 "你好"
    ↓
SandboxClient.type_text("你好")
    ↓
POST http://127.0.0.1:5801/input/type  { text: "你好" }
    ↓
InputSimulator::type_text("你好", Some(tauri_pid))
    ↓
CGEvent → Tauri 进程 → xterm.js → 用户看到文字
```

CLI 沙箱自动使用 PTY 直写模式（根据 `InstanceKind` 自动检测）：

```
CLI command: sandbox-cli type --id abc123 "你好"
    ↓
SandboxClient.list_processes() → 获取 PTY PID
SandboxClient.pty_write(pid, "你好")
    ↓
POST http://127.0.0.1:5801/pty/write  { pid: 1001, data: "你好" }
    ↓
ProcessManager::send_input(pid, b"你好")
    ↓
PTY master → CLI 进程接收输入
```

### App 模式（CGEvent）

```
CLI command: sandbox-cli key --id def456 Return
    ↓
POST http://127.0.0.1:5802/input/key  { key: "return" }
    ↓
InputSimulator::press_key("return", [], Some(app_pid))
    ↓
CGEvent → 目标应用接收按键
```

### 按键到 PTY 字节映射

| 按键 | PTY 字节 |
|------|----------|
| return / enter | `\r` |
| tab | `\t` |
| escape | `\x1b` |
| backspace / delete | `\x7f` |
| space | ` ` |

## 三、CLI 命令设计

```bash
# 沙箱管理
sandbox-cli start <command> [args...]      # 启动沙箱（已有）
sandbox-cli list                           # 列出所有沙箱实例
sandbox-cli inspect <id>                   # 查看沙箱详情
sandbox-cli close <id>                     # 关闭沙箱

# 键盘操作
sandbox-cli type --id <id> "text"          # 输入文本（自动路由：CLI→PTY，App→CGEvent）
sandbox-cli key --id <id> Return           # 按键
sandbox-cli key --id <id> Return -m cmd    # 按键 + 修饰键

# 鼠标操作
sandbox-cli click --id <id> 100 200        # 点击
sandbox-cli click --id <id> 100 200 --btn right  # 右键点击

# 观察
sandbox-cli screenshot --id <id> -o out.png  # 截图
sandbox-cli processes --id <id>             # 列出进程
```

## 四、模块结构

```
crates/sandbox-cli/src/
├── main.rs          # CLI 入口 + 所有子命令
└── client.rs        # SandboxClient：HTTP 客户端封装
```

### SandboxClient API

```rust
pub struct SandboxClient { base_url: String, client: reqwest::Client }

impl SandboxClient {
    // 构造
    pub fn from_instance_id(id: &str) -> Result<Self>  // 从注册中心查找
    pub fn from_port(port: u16) -> Self                 // 直接指定端口

    // 输入（CGEvent）
    pub async fn type_text(&self, text: &str) -> Result<()>
    pub async fn press_key(&self, key: &str, modifiers: &[String]) -> Result<()>
    pub async fn click(&self, x: f64, y: f64, button: &str) -> Result<()>

    // 输入（PTY）
    pub async fn pty_write(&self, pid: u32, data: &str) -> Result<()>
    pub async fn pty_write_auto(&self, data: &str) -> Result<()>  // 自动发现 PTY PID

    // 观察
    pub async fn health(&self) -> Result<HealthResponse>
    pub async fn sandbox_info(&self) -> Result<InfoResponse>
    pub async fn list_windows(&self) -> Result<Vec<(u32, String)>>
    pub async fn list_processes(&self) -> Result<Vec<ProcessInfo>>
    pub async fn screenshot(&self) -> Result<Vec<u8>>

    // 控制
    pub async fn shutdown(&self) -> Result<()>
}
```

## 五、实现步骤

1. 创建 `client.rs`：SandboxClient HTTP 客户端
2. 更新 `main.rs`：添加 list/inspect/close/type/key/click/processes 命令
3. 编写测试
4. 本地验证 + CI

## 六、场景验证

### 场景一：Claude Code 交互

```bash
# 启动沙箱运行 claude
sandbox-cli start claude
# 输出: Sandbox started (查看 sandbox-cli list 获取 id)

# 等待 claude 启动完成后操作
sandbox-cli type --id <id> "你是谁？"
sandbox-cli key --id <id> Return
```

### 场景二：Zsh 命令执行

```bash
# 启动沙箱运行 zsh
sandbox-cli start zsh

# 输入命令并执行
sandbox-cli type --id <id> 'echo "hello world"'
sandbox-cli key --id <id> Return
```
