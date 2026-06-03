# opencode TUI 空白屏幕问题分析

> 分析日期：2026-05-24
> 状态：Phase 1 已完成（专用 Reader 线程），opencode 在 Tauri 应用中仍空白

## 一、问题描述

在沙箱中启动 opencode（一个 TUI 终端应用）后，终端一直显示空白。PTY Reader 线程已在 cargo test 中验证可用（32103+ chars），但在 Tauri 应用中 HTTP `/pty/output/1000` 返回 `null`。

## 二、测试结果汇总

| 测试场景 | 结果 |
|---------|------|
| `cargo test pty_reader` — cat 命令 | 14 chars (PASS) |
| `cargo test pty_reader` — opencode 无 resize | 32103 chars (PASS) |
| `cargo test pty_reader` — opencode 有 resize | 50737 chars (PASS) |
| Tauri 应用：zsh 终端 | 正常渲染 |
| Tauri 应用：opencode 终端 | 空白 |
| Tauri 应用：curl `/pty/output/1000` | `{"output": null}` |
| Tauri 应用：curl `/pty/write/1000` + data | `{"written": true}` |

## 三、根因分析

### 3.1 Reader 线程架构（已完成）

```
spawn_cli("opencode")
  ├─ 创建 PTY (80x24)
  ├─ 启动 opencode 子进程
  ├─ 克隆 reader → 移入专用线程
  ├─ 创建 Arc<Mutex<VecDeque<String>>> buffer
  ├─ 启动 reader thread: 持续 read() → push_back(text)
  └─ 注册 PtySession { buffer, stop_flag, reader_thread }

read_output(pid)
  ├─ 从 SESSIONS 获取 buffer Arc
  ├─ 释放 SESSIONS 锁
  └─ drain buffer → Ok(Some(text)) 或 Ok(None)
```

### 3.2 为什么 cargo test 通过但 Tauri 应用失败

**关键差异：进程环境**

| 维度 | cargo test | Tauri 应用 |
|------|-----------|-----------|
| 运行方式 | `cargo test` (终端) | `cli-box start` → CLI → Tauri app |
| 进程环境 | 完整 shell 环境 | 继承 CLI 环境 |
| TERM | xterm-256color | 取决于启动方式 |
| PATH | 完整 | 取决于启动方式 |
| PTY 子进程环境 | 继承 test 进程 | 继承 Tauri 进程 |

**`portable-pty` 环境继承机制：**

```rust
// portable-pty 源码: cmdbuilder.rs:215
CommandBuilder::new(command) {
    // 调用 get_base_env() 捕获 std::env::vars_os()
    // 即捕获调用时父进程的全部环境变量
}
```

```rust
// portable-pty 源码: cmdbuilder.rs:498
as_command() {
    cmd.env_clear();                    // 清空
    cmd.env("SHELL", shell);           // 设置 SHELL
    cmd.envs(self.envs.values());      // 重新应用捕获的所有环境变量
}
```

**结论：PTY 子进程继承的是 `CommandBuilder::new()` 调用时父进程的环境。**

### 3.3 两种启动路径的环境差异

| 启动方式 | 环境 | opencode 能否工作 |
|---------|------|------------------|
| `cli-box start opencode` (CLI) | 完整 shell 环境 (PATH, TERM, HOME, ...) | 应该可以 |
| 双击 `.app` / `open` | 最小 GUI 环境 (仅 /usr/bin:/bin:/usr/sbin:/sbin) | 可能不行 |

**但用户是通过 CLI 启动的，理论上环境应该正确。**

### 3.4 可能的环境问题

即使通过 CLI 启动，以下环境变量可能仍然缺失或不正确：

1. **`TERM`** — opencode 可能检查此变量决定是否渲染 TUI
2. **`HOME`** — opencode 可能需要此变量读取配置文件
3. **`USER`** — 部分 TUI 应用依赖此变量
4. **`LANG` / `LC_ALL`** — 字符编码相关

### 3.5 另一个可能：opencode 检查终端能力

opencode 是一个 Go TUI 应用（基于 bubbletea/lipgloss）。它可能：
1. 检查 `TERM` 环境变量
2. 查询 terminfo 数据库
3. 如果终端能力不足，拒绝渲染或输出为空

在 cargo test 中，测试进程运行在终端里，`TERM=xterm-256color`。在 Tauri 应用中，即使通过 CLI 启动，`TERM` 的值可能不同。

## 四、解决方案

### 方案 1：显式设置 PTY 环境变量（推荐，最可能的修复）

在 `spawn_cli` 中显式设置 `TERM=xterm-256color` 和其他必要的环境变量：

```rust
// crates/sandbox-core/src/process/mod.rs
let mut cmd = CommandBuilder::new(command);
cmd.args(args);
// 显式设置终端环境变量
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
cmd.env("LANG", "en_US.UTF-8");
```

**优点：** 改动小（3 行），直接解决环境问题
**缺点：** 需要测试确认是否是根因

### 方案 2：添加文件日志调试

在 release 模式下，`tracing::debug!` 不可见。添加文件日志：

```rust
// crates/sandbox-core/src/process/mod.rs
// 在 reader thread 中添加文件日志
let log_path = std::env::temp_dir().format!("pty-reader-{}.log", tracked_id);
if let Ok(mut f) = std::fs::File::create(&log_path) {
    let _ = writeln!(f, "reader thread started, pid={}", tracked_id);
}
```

**优点：** 能看到 reader thread 的实际运行状态
**缺点：** 调试用，生产环境应移除

### 方案 3：添加诊断 HTTP 端点

添加一个端点返回 session 状态：

```rust
GET /pty/status/{pid}
→ { "buffer_size": 42, "reader_alive": true, "stop_flag": false }
```

**优点：** 能远程诊断 session 状态
**缺点：** 需要修改 server 和前端

### 方案 4：测试其他 TUI 应用

用 `vim`、`htop`、`nano` 等测试，确认是否是 opencode 特有问题：

```bash
./cli-box start vim
./cli-box start htop
```

**优点：** 快速判断是 opencode 特有问题还是通用问题
**缺点：** 需要手动测试

## 五、建议的执行顺序

1. **先测试方案 4**：用 vim/htop 测试，确认是否是 opencode 特有问题
2. **实施方案 1**：添加 `TERM=xterm-256color` 等环境变量
3. **实施方案 2**：添加文件日志，确认 reader thread 是否在运行
4. **实施方案 3**：如果以上都不行，添加诊断端点

## 六、已修复的问题（Phase 1）

以下问题已在专用 Reader 线程实现中修复：

1. ~~阻塞式 `read()` 无限期占用 reader~~ → 专用线程持续读取
2. ~~SESSIONS 锁在 drain 期间持有~~ → Clone Arc 后释放锁
3. ~~死代码 `reader` 字段~~ → 已移除
4. ~~前端单次错误终止轮询~~ → 保留（但根本原因已修复）

---

**下一步：** 测试方案 4（其他 TUI 应用），然后实施方案 1（环境变量）。
