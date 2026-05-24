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
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;

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

        conn.execute(
            "INSERT INTO pty_output (sandbox_id, offset, data) VALUES (?1, ?2, ?3)",
            params![self.sandbox_id, offset as i64, data],
        )
        .map_err(|e| AppError::Process(format!("PT store insert failed: {e}")))?;

        let new_total = total_bytes + data.len() as u64;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![meta_key, new_total.to_string()],
        )
        .map_err(|e| AppError::Process(format!("PT store meta update failed: {e}")))?;

        if new_total as usize > self.max_size {
            self.truncate_old(&conn)?;
            // Recalculate actual total after truncation
            let actual: u64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(LENGTH(data)), 0) FROM pty_output WHERE sandbox_id = ?1",
                    params![self.sandbox_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                params![meta_key, actual.to_string()],
            )
            .map_err(|e| AppError::Process(format!("PT store meta update failed: {e}")))?;
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

        let oldest_offset: u64 = conn
            .query_row(
                "SELECT COALESCE(MIN(offset), 0) FROM pty_output WHERE sandbox_id = ?1",
                params![self.sandbox_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

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
    fn truncate_old(&self, conn: &Connection) -> Result<()> {
        // Re-read actual total from data (authoritative source)
        let actual: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(data)), 0) FROM pty_output WHERE sandbox_id = ?1",
                params![self.sandbox_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if actual as usize <= self.max_size {
            return Ok(());
        }

        let excess = actual as usize - self.max_size;

        // Delete oldest rows until we're under the limit
        conn.execute(
            "DELETE FROM pty_output WHERE sandbox_id = ?1 AND offset < (
                SELECT COALESCE(MAX(offset), 0) - ?2 FROM pty_output
                WHERE sandbox_id = ?1
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
        let conn = Connection::open_in_memory().unwrap();
        let mut store = PtyStore::init_schema(conn, "test-trunc").unwrap();
        store.max_size = 100;

        store.append(&"a".repeat(60)).unwrap();
        store.append(&"b".repeat(60)).unwrap();
        store.append(&"c".repeat(30)).unwrap();

        let total = store.total_bytes().unwrap();
        assert!(total <= 110, "Expected ~100, got {total}");

        let chunks = store.read_all().unwrap();
        let first_offset = chunks.first().map(|c| c.offset).unwrap_or(0);
        assert!(first_offset > 0, "Oldest data should have been truncated");
    }
}
