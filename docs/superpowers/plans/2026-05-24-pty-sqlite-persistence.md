# PTY SQLite 持久化 — 参照 WaveTerm 的 blockfile 方案

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 使用 SQLite 持久化 PTY 输出，支持循环缓冲（10MB 上限）、晚订阅者重放、WebSocket 断连恢复。

**Architecture:** 参照 WaveTerm 的 blockfile 模式：Reader 线程每次读取 PTY 输出后，同时写入 SQLite 持久化存储和 broadcast channel 实时推送。WebSocket 连接时先从 SQLite 读取已有数据重放，再切换到 broadcast 实时流。SQLite 使用 WAL 模式支持并发读写。

**Tech Stack:** Rust, rusqlite, tokio, axum WebSocket

---

## 问题根因

```
当前架构：
PTY → Reader线程 → broadcast::send() → WebSocket
                  → VecDeque buffer（内存，不持久）

问题：
1. broadcast 不缓冲未订阅者消息 → 早期输出丢失
2. VecDeque 在内存中 → 进程退出/断连即丢失
3. 无断点续传 → 重连后无法恢复
```

## 目标架构

```
PTY → Reader线程 → SQLite 持久化（循环 10MB）
                  → broadcast::send() → WebSocket 实时流

WebSocket 连接：
1. 从 SQLite 读取已有数据 → 发送给客户端（重放）
2. 订阅 broadcast → 接收实时数据
```

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `Cargo.toml` (workspace) | 添加 `rusqlite` workspace 依赖 |
| `crates/sandbox-core/Cargo.toml` | 添加 `rusqlite` 依赖 |
| `crates/sandbox-core/src/pty_store.rs` | **新建** SQLite PTY 存储（循环缓冲、读写、清理） |
| `crates/sandbox-core/src/process/mod.rs` | 修改 PtySession 使用 PtyStore，修改 reader thread |
| `crates/sandbox-core/src/server/mod.rs` | 修改 `handle_pty_ws`：先从 SQLite 重放再 streaming |
| `crates/sandbox-core/tests/pty_store_test.rs` | **新建** PtyStore 单元测试 |

---

### Task 1: 添加 rusqlite 依赖

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/sandbox-core/Cargo.toml`

- [ ] **Step 1: 在 workspace Cargo.toml 中添加 rusqlite**

在 `Cargo.toml` 的 `[workspace.dependencies]` 中添加：

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

`bundled` feature 自带 SQLite 源码编译，无需系统安装。

- [ ] **Step 2: 在 sandbox-core Cargo.toml 中引用 workspace 依赖**

在 `crates/sandbox-core/Cargo.toml` 的 `[dependencies]` 中添加：

```toml
rusqlite.workspace = true
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p sandbox-core`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/sandbox-core/Cargo.toml
git commit -m "chore(core): add rusqlite dependency for PTY output persistence"
```

---

### Task 2: 创建 PtyStore — SQLite 循环缓冲

**Files:**
- Create: `crates/sandbox-core/src/pty_store.rs`

- [ ] **Step 1: 创建 pty_store.rs 模块**

```rust
//! SQLite-backed circular buffer for PTY output persistence.
//!
//! Inspired by WaveTerm's blockfile approach:
//! - PTY output is written to a SQLite table with byte-offset tracking
//! - Circular: keeps last `max_size` bytes, auto-truncates old data
//! - WAL mode for concurrent read/write from reader thread + WebSocket handler
//! - `sandbox_id` column enables future shared-database multi-session support

use crate::error::{AppError, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Default maximum buffer size: 10MB
const DEFAULT_MAX_SIZE: usize = 10 * 1024 * 1024;

/// A single chunk of PTY output with its byte offset
#[derive(Debug, Clone)]
pub struct PtyChunk {
    pub offset: u64,
    pub data: String,
}

/// SQLite-backed circular buffer for PTY output.
///
/// Schema:
/// - `pty_output` table: (id INTEGER PRIMARY KEY, sandbox_id TEXT, offset INTEGER, data TEXT)
/// - `meta` table: (key TEXT PRIMARY KEY, value TEXT) — stores total_bytes per sandbox
///
/// The `sandbox_id` column allows a single shared SQLite database to store
/// output from multiple sandbox sessions (future: file-mode persistence).
/// Currently each PtyStore uses an in-memory database, so sandbox_id is
/// always the same value — but the column is ready for shared-file mode.
///
/// Circular behavior: when total_bytes exceeds max_size, old rows are
/// deleted until we're back under the limit.
pub struct PtyStore {
    conn: Mutex<Connection>,
    sandbox_id: String,
    max_size: usize,
}

impl PtyStore {
    /// Create a new PtyStore backed by an in-memory SQLite database.
    pub fn new_in_memory(sandbox_id: &str) -> Result<Arc<Self>> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Process(format!("Failed to open in-memory SQLite: {e}")))?;
        Self::init_schema(conn, sandbox_id).map(Arc::new)
    }

    /// Create a new PtyStore backed by a file on disk.
    pub fn new_file(path: PathBuf, sandbox_id: &str) -> Result<Arc<Self>> {
        let conn = Connection::open(&path)
            .map_err(|e| AppError::Process(format!("Failed to open SQLite file: {e}")))?;
        Self::init_schema(conn, sandbox_id).map(Arc::new)
    }

    fn init_schema(conn: Connection, sandbox_id: &str) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS pty_output (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 sandbox_id TEXT NOT NULL,
                 offset INTEGER NOT NULL,
                 data TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_pty_sandbox
                 ON pty_output(sandbox_id, offset);
             CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .map_err(|e| AppError::Process(format!("Failed to init PTY store schema: {e}")))?;

        // Initialize total_bytes for this sandbox
        let meta_key = format!("total_bytes:{sandbox_id}");
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES (?1, '0')",
            params![meta_key],
        )
        .map_err(|e| AppError::Process(format!("PT store meta init failed: {e}")))?;

        Ok(Self {
            conn: Mutex::new(conn),
            sandbox_id: sandbox_id.to_string(),
            max_size: DEFAULT_MAX_SIZE,
        })
    }

    fn meta_key(&self) -> String {
        format!("total_bytes:{}", self.sandbox_id)
    }

    /// Append PTY output data. Returns the starting byte offset of this chunk.
    pub fn append(&self, data: &str) -> Result<u64> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;

        // Get current total bytes for this sandbox
        let meta_key = self.meta_key();
        let total_bytes: u64 = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![meta_key],
                |row| row.get::<_, String>(0),
            )
            .map(|s| s.parse().unwrap_or(0))
            .unwrap_or(0);

        let offset = total_bytes;

        // Insert the new chunk with sandbox_id
        conn.execute(
            "INSERT INTO pty_output (sandbox_id, offset, data) VALUES (?1, ?2, ?3)",
            params![self.sandbox_id, offset as i64, data],
        )
        .map_err(|e| AppError::Process(format!("PT store insert failed: {e}")))?;

        // Update total bytes
        let new_total = total_bytes + data.len() as u64;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![meta_key, new_total.to_string()],
        )
        .map_err(|e| AppError::Process(format!("PT store meta update failed: {e}")))?;

        // Circular: truncate old data if over limit
        if new_total as usize > self.max_size {
            self.truncate_old(&conn, new_total)?;
        }

        Ok(offset)
    }

    /// Read all chunks from a given byte offset.
    /// If offset is before the oldest available data, adjusts to oldest.
    pub fn read_from(&self, byte_offset: u64) -> Result<Vec<PtyChunk>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;

        // Get the oldest available offset for this sandbox
        let oldest_offset: u64 = conn
            .query_row(
                "SELECT COALESCE(MIN(offset), 0) FROM pty_output WHERE sandbox_id = ?1",
                params![self.sandbox_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Adjust if requested offset is too old
        let effective_offset = byte_offset.max(oldest_offset);

        let mut stmt = conn
            .prepare(
                "SELECT offset, data FROM pty_output
                 WHERE sandbox_id = ?1 AND offset >= ?2
                 ORDER BY offset ASC",
            )
            .map_err(|e| AppError::Process(format!("PT store prepare failed: {e}")))?;

        let chunks = stmt
            .query_map(params![self.sandbox_id, effective_offset as i64], |row| {
                Ok(PtyChunk {
                    offset: row.get(0)?,
                    data: row.get(1)?,
                })
            })
            .map_err(|e| AppError::Process(format!("PT store query failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(chunks)
    }

    /// Read ALL available chunks (for initial replay).
    pub fn read_all(&self) -> Result<Vec<PtyChunk>> {
        self.read_from(0)
    }

    /// Get the current total bytes written.
    pub fn total_bytes(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;

        let meta_key = self.meta_key();
        conn.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![meta_key],
            |row| row.get::<_, String>(0),
        )
        .map(|s| s.parse().unwrap_or(0))
        .map_err(|e| AppError::Process(format!("PT store meta read failed: {e}")))
    }

    /// Truncate old data to keep total under max_size.
    fn truncate_old(&self, conn: &Connection, current_total: u64) -> Result<()> {
        let excess = current_total as usize - self.max_size;
        if excess == 0 {
            return Ok(());
        }

        // Delete oldest rows for this sandbox until we're under the limit
        conn.execute(
            "DELETE FROM pty_output WHERE id IN (
                SELECT id FROM pty_output
                WHERE sandbox_id = ?1
                ORDER BY offset ASC
                LIMIT (
                    SELECT COUNT(*) FROM pty_output
                    WHERE sandbox_id = ?1 AND offset < (
                        SELECT COALESCE(MAX(offset), 0) - ?2 FROM pty_output
                        WHERE sandbox_id = ?1
                    )
                )
            )",
            params![self.sandbox_id, excess as i64],
        )
        .map_err(|e| AppError::Process(format!("PT store truncate failed: {e}")))?;

        Ok(())
    }

    /// Clear all data for this sandbox (for cleanup on session drop).
    pub fn clear(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;

        let meta_key = self.meta_key();
        conn.execute(
            "DELETE FROM pty_output WHERE sandbox_id = ?1",
            params![self.sandbox_id],
        )
        .map_err(|e| AppError::Process(format!("PT store clear failed: {e}")))?;

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '0')",
            params![meta_key],
        )
        .map_err(|e| AppError::Process(format!("PT store meta reset failed: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_read_all() {
        let store = PtyStore::new_in_memory("test-001").unwrap();
        store.append("hello ").unwrap();
        store.append("world").unwrap();

        let chunks = store.read_all().unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, "hello ");
        assert_eq!(chunks[1].data, "world");
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[1].offset, 6);
    }

    #[test]
    fn test_read_from_offset() {
        let store = PtyStore::new_in_memory("test-002").unwrap();
        store.append("aaa").unwrap();
        store.append("bbb").unwrap();
        store.append("ccc").unwrap();

        let chunks = store.read_from(3).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, "bbb");
        assert_eq!(chunks[1].data, "ccc");
    }

    #[test]
    fn test_read_from_before_oldest() {
        let store = PtyStore::new_in_memory("test-003").unwrap();
        store.append("first").unwrap();
        store.append("second").unwrap();

        // Request from offset 0, but oldest might have been truncated
        let chunks = store.read_from(0).unwrap();
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_total_bytes() {
        let store = PtyStore::new_in_memory("test-004").unwrap();
        assert_eq!(store.total_bytes().unwrap(), 0);

        store.append("abc").unwrap();
        assert_eq!(store.total_bytes().unwrap(), 3);

        store.append("de").unwrap();
        assert_eq!(store.total_bytes().unwrap(), 5);
    }

    #[test]
    fn test_clear() {
        let store = PtyStore::new_in_memory("test-005").unwrap();
        store.append("data").unwrap();
        assert_eq!(store.total_bytes().unwrap(), 4);

        store.clear().unwrap();
        assert_eq!(store.total_bytes().unwrap(), 0);
        assert!(store.read_all().unwrap().is_empty());
    }

    #[test]
    fn test_multiple_sandboxes_isolated() {
        let store_a = PtyStore::new_in_memory("sandbox-a").unwrap();
        let store_b = PtyStore::new_in_memory("sandbox-b").unwrap();

        store_a.append("from-a").unwrap();
        store_b.append("from-b").unwrap();

        let chunks_a = store_a.read_all().unwrap();
        let chunks_b = store_b.read_all().unwrap();

        assert_eq!(chunks_a.len(), 1);
        assert_eq!(chunks_a[0].data, "from-a");
        assert_eq!(chunks_b.len(), 1);
        assert_eq!(chunks_b[0].data, "from-b");
    }

    #[test]
    fn test_truncate_at_10mb() {
        // Create a store with small max_size to test truncation
        let conn = Connection::open_in_memory().unwrap();
        let mut store = PtyStore::init_schema(conn, "test-trunc").unwrap();
        // Override max_size to 100 bytes for testing
        Arc::get_mut(&mut store).unwrap().max_size = 100;

        // Write 150 bytes total
        store.append(&"a".repeat(60)).unwrap();
        store.append(&"b".repeat(60)).unwrap();
        store.append(&"c".repeat(30)).unwrap();

        // Total should be capped near 100
        let total = store.total_bytes().unwrap();
        assert!(total <= 110, "Expected ~100, got {total}");

        // Oldest data should be truncated
        let chunks = store.read_all().unwrap();
        let first_offset = chunks.first().map(|c| c.offset).unwrap_or(0);
        assert!(first_offset > 0, "Oldest data should have been truncated");
    }
}
```

- [ ] **Step 2: 在 lib.rs 中注册模块**

在 `crates/sandbox-core/src/lib.rs` 中添加：

```rust
pub mod pty_store;
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p sandbox-core -- pty_store`
Expected: 5 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/pty_store.rs crates/sandbox-core/src/lib.rs Cargo.toml crates/sandbox-core/Cargo.toml
git commit -m "feat(core): add PtyStore for SQLite-backed PTY output persistence

Circular 2MB buffer in SQLite with WAL mode. Supports append, read_all,
read_from (for late subscribers), and automatic old-data truncation."
```

---

### Task 3: 修改 PtySession — 集成 PtyStore

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs`

- [ ] **Step 1: 修改 PtySession 结构体**

替换 `buffer: Arc<Mutex<VecDeque<String>>>` 为 `store: Arc<PtyStore>`：

```rust
#[cfg(target_os = "macos")]
struct PtySession {
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn MasterPty>,
    #[allow(dead_code)]
    child_pid: u32,
    command: String,
    /// SQLite-backed persistent output store (replaces VecDeque buffer)
    store: Arc<PtyStore>,
    /// Flag to signal the reader thread to stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the reader thread (for join on cleanup)
    reader_thread: Option<std::thread::JoinHandle<()>>,
    /// Broadcast sender for streaming PTY output to WebSocket subscribers
    output_tx: broadcast::Sender<String>,
}
```

- [ ] **Step 2: 修改 spawn_cli 中的初始化代码**

替换 buffer 创建为 PtyStore 创建：

```rust
// Before:
let buffer: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));

// After:
let store = PtyStore::new_in_memory()?;
```

同时修改 reader thread 内部，用 store 替代 buffer：

```rust
// Before:
let thread_buffer = Arc::clone(&buffer);

// After:
let thread_store = Arc::clone(&store);
```

- [ ] **Step 3: 修改 reader thread 内部逻辑**

```rust
// Before:
Ok(n) => {
    let text = String::from_utf8_lossy(&read_buf[..n]).to_string();
    let _ = thread_tx.send(text.clone());
    if let Ok(mut buf) = thread_buffer.lock() {
        if buf.len() >= 1000 {
            buf.pop_front();
        }
        buf.push_back(text);
    }
}

// After:
Ok(n) => {
    let text = String::from_utf8_lossy(&read_buf[..n]).to_string();
    // Persist to SQLite (survives reconnections)
    if let Err(e) = thread_store.append(&text) {
        warn!("PTY reader {tracked_id}: store append failed: {e}");
    }
    // Real-time broadcast to current subscribers
    let _ = thread_tx.send(text);
}
```

- [ ] **Step 4: 修改 PtySession 插入代码**

```rust
// Before:
sessions.insert(tracked_id, PtySession {
    writer,
    master: pty_pair.master,
    child_pid: child_pid.unwrap_or(0),
    command: command.to_string(),
    buffer,
    stop_flag,
    reader_thread: Some(reader_thread),
    output_tx,
});

// After:
sessions.insert(tracked_id, PtySession {
    writer,
    master: pty_pair.master,
    child_pid: child_pid.unwrap_or(0),
    command: command.to_string(),
    store,
    stop_flag,
    reader_thread: Some(reader_thread),
    output_tx,
});
```

- [ ] **Step 5: 修改 read_output 方法（HTTP 轮询兼容）**

将 `read_output` 改为从 PtyStore 读取：

```rust
pub fn read_output(pid: u32) -> Result<Option<String>> {
    let sessions = SESSIONS
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;

    let chunks = session.store.read_all()?;
    if chunks.is_empty() {
        return Ok(None);
    }

    let text: String = chunks.into_iter().map(|c| c.data).collect();
    // Clear after reading (HTTP poll mode)
    session.store.clear()?;

    Ok(Some(text))
}
```

- [ ] **Step 6: 编译验证**

Run: `cargo check -p sandbox-core`
Expected: 编译通过

- [ ] **Step 7: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "refactor(process): replace VecDeque buffer with PtyStore

Reader thread now writes to SQLite via PtyStore instead of in-memory
VecDeque. Output persists across WebSocket reconnections and supports
late-subscriber replay."
```

---

### Task 4: 修改 WebSocket handler — 先从 SQLite 重放

**Files:**
- Modify: `crates/sandbox-core/src/server/mod.rs:395-469` (`handle_pty_ws`)
- Modify: `crates/sandbox-core/src/process/mod.rs` (添加 `get_store` 方法)

- [ ] **Step 1: 在 ProcessManager 中添加 get_store 方法**

```rust
/// Get the PtyStore for a session (for WebSocket replay).
#[cfg(target_os = "macos")]
pub fn get_store(pid: u32) -> Result<Arc<PtyStore>> {
    let sessions = SESSIONS
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;
    Ok(Arc::clone(&session.store))
}

#[cfg(not(target_os = "macos"))]
pub fn get_store(_pid: u32) -> Result<Arc<PtyStore>> {
    Err(AppError::Process(
        "get_store only supported on macOS".into(),
    ))
}
```

- [ ] **Step 2: 修改 handle_pty_ws 函数**

```rust
async fn handle_pty_ws(pid: u32, socket: WebSocket) {
    let mut rx = match ProcessManager::subscribe_output(pid) {
        Ok(rx) => rx,
        Err(e) => {
            tracing::warn!("[pty_ws] pid={pid}: subscribe failed: {e}");
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Phase 1: Replay existing output from SQLite (late subscriber recovery)
    match ProcessManager::get_store(pid) {
        Ok(store) => {
            match store.read_all() {
                Ok(chunks) => {
                    let total_chars: usize = chunks.iter().map(|c| c.data.len()).sum();
                    for chunk in chunks {
                        if ws_tx.send(Message::Text(chunk.data.into())).await.is_err() {
                            tracing::debug!("[pty_ws] pid={pid}: client disconnected during replay");
                            return;
                        }
                    }
                    tracing::debug!(
                        "[pty_ws] pid={pid}: replayed {} chunks ({} chars) from SQLite",
                        chunks.len(),
                        total_chars
                    );
                }
                Err(e) => {
                    tracing::warn!("[pty_ws] pid={pid}: SQLite read failed: {e}");
                }
            }
        }
        Err(e) => {
            tracing::warn!("[pty_ws] pid={pid}: get_store failed: {e}");
        }
    }

    // Phase 2: Real-time streaming via broadcast (same as before)
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // recv_task 保持不变 ...
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p sandbox-core`
Expected: 编译通过

- [ ] **Step 4: 运行现有测试**

Run: `cargo test -p sandbox-core`
Expected: 所有测试通过

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-core/src/server/mod.rs crates/sandbox-core/src/process/mod.rs
git commit -m "fix(server): replay SQLite-stored PTY output to late WebSocket subscribers

Two-phase load pattern (inspired by WaveTerm):
1. Read all existing output from SQLite and send to client
2. Subscribe to broadcast for real-time streaming

This ensures shell prompts and early output are visible immediately
when the frontend connects, even if the PTY started before WebSocket."
```

---

### Task 5: 端到端验证 — release 构建 + 手动测试

**Files:** 无代码修改，纯验证

- [ ] **Step 1: 运行全量测试**

Run: `cargo test --all && pnpm test:unit && pnpm typecheck`
Expected: 全部通过

- [ ] **Step 2: 构建 release**

Run: `./release.sh`
Expected: 构建成功

- [ **Step 3: 测试 zsh — 启动后立即可见**

```bash
./release/cli-box start zsh
sleep 5
./release/cli-box list
./release/cli-box screenshot --id <id> -o test_zsh_sqlite.png
```

Expected: 截图中立即显示 zsh prompt（`zn-ice@MacBook-Neo ~ %`）

- [ ] **Step 4: 测试 claude — 启动后立即可见**

```bash
./release/cli-box start claude
sleep 8
./release/cli-box list
./release/cli-box screenshot --id <id> -o test_claude_sqlite.png
```

Expected: 截图中显示 claude 的 "Welcome back!" 界面

- [ ] **Step 5: 测试 WebSocket 断连恢复**

```bash
# 启动 zsh，发送命令
./release/cli-box start zsh
sleep 5
ID=$(./release/cli-box list | grep -o '[a-f0-9]\{8\}' | head -1)
./release/cli-box type --id $ID --pty 'echo "before disconnect"'
./release/cli-box key --id $ID Return
sleep 2

# 截图确认输出存在
./release/cli-box screenshot --id $ID -o before_disconnect.png

# 关闭再重新打开（模拟断连）
./release/cli-box close $ID
sleep 2
# 注意：SQLite 在内存中，关闭即丢失。这是已知限制。
```

- [ ] **Step 6: 完成**
