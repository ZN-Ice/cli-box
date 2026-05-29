use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

const LOG_BASE: &str = ".sandbox/logs";
const MAX_LOG_DAYS: u64 = 7;

// ── Path helpers ────────────────────────────────────────

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Base log directory: `~/.sandbox/logs/`
pub fn log_base_dir() -> PathBuf {
    home_dir().join(LOG_BASE)
}

/// Get today's date as `YYYY-MM-DD`.
fn today_str() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Days since epoch (UTC)
    let days = secs / 86400;
    // civil_from_days algorithm (Howard Hinnant)
    let z = days as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Today's log directory: `~/.sandbox/logs/YYYY-MM-DD/`
fn today_log_dir() -> PathBuf {
    log_base_dir().join(today_str())
}

/// Get the log file path for a specific sandbox.
pub fn sandbox_log_path(sandbox_id: &str) -> PathBuf {
    today_log_dir().join(format!("{sandbox_id}.log"))
}

/// Get the shared server log path.
pub fn server_log_path() -> PathBuf {
    today_log_dir().join("sandbox-server.log")
}

/// Get the CLI log path.
pub fn cli_log_path() -> PathBuf {
    log_base_dir().join(format!("sandbox-cli.log.{}", today_str()))
}

// ── Cleanup ─────────────────────────────────────────────

/// Delete log directories older than `MAX_LOG_DAYS`.
pub fn cleanup_old_logs() {
    let base = log_base_dir();
    if !base.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(&base) else {
        return;
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff_days = now_secs / 86400;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Match date directories: YYYY-MM-DD
        if name.len() == 10 && name.chars().all(|c| c.is_ascii_digit() || c == '-') {
            if let Some(dir_days) = parse_date_days(&name) {
                if cutoff_days.saturating_sub(dir_days) > MAX_LOG_DAYS {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
        // Match rolling CLI logs: sandbox-cli.log.YYYY-MM-DD
        if let Some(suffix) = name.strip_prefix("sandbox-cli.log.") {
            if suffix.len() == 10 {
                if let Some(dir_days) = parse_date_days(suffix) {
                    if cutoff_days.saturating_sub(dir_days) > MAX_LOG_DAYS {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

/// Parse `YYYY-MM-DD` to days since epoch. Returns `None` on invalid input.
fn parse_date_days(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // civil_from_days in reverse
    let y = y - if m <= 2 { 1 } else { 0 };
    let m = if m > 2 { m } else { m + 12 };
    let era = ((if y >= 0 { y } else { y - 399 }) / 400) as u64;
    let yoe = (y as u64) - era * 400;
    let doy = ((153 * (m - 3) + 2) / 5) as u64 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

// ── Log level ───────────────────────────────────────────

fn effective_level() -> &'static str {
    if std::env::var("SANDBOX_LOGGER_LEVEL")
        .map(|v| v.to_lowercase() == "debug")
        .unwrap_or(false)
    {
        "debug"
    } else {
        "info"
    }
}

// ── Init: sandbox (Tauri) ──────────────────────────────

/// Initialize logging for a sandbox Tauri process.
///
/// Creates:
/// - `~/.sandbox/logs/{date}/{sandbox_id}.log` (sandbox-specific)
/// - `~/.sandbox/logs/{date}/sandbox-server.log` (shared server)
///
/// Also cleans up log directories older than 7 days.
pub fn init_sandbox_logging(sandbox_id: &str) -> (WorkerGuard, WorkerGuard) {
    cleanup_old_logs();

    let dir = today_log_dir();
    fs::create_dir_all(&dir).ok();

    let level = effective_level();

    // Sandbox-specific log
    let sandbox_appender = RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!("{sandbox_id}.log"))
        .max_log_files(MAX_LOG_DAYS as usize)
        .build(&dir)
        .expect("Failed to create sandbox log appender");
    let (sandbox_writer, sandbox_guard) = tracing_appender::non_blocking(sandbox_appender);

    // Shared server log
    let server_appender = RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("sandbox-server.log")
        .max_log_files(MAX_LOG_DAYS as usize)
        .build(&dir)
        .expect("Failed to create server log appender");
    let (server_writer, server_guard) = tracing_appender::non_blocking(server_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let subscriber = tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(sandbox_writer)
                .with_ansi(false)
                .with_filter(filter),
        )
        .with(
            fmt::layer()
                .with_writer(server_writer)
                .with_ansi(false)
                .with_filter(
                    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level)),
                ),
        );

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");

    (sandbox_guard, server_guard)
}

// ── Init: CLI ──────────────────────────────────────────

/// Initialize logging for the CLI tool.
///
/// Creates `~/.sandbox/logs/sandbox-cli.log.{YYYY-MM-DD}`.
/// Outputs to both file and stderr.
pub fn init_cli_logging() -> WorkerGuard {
    let dir = log_base_dir();
    fs::create_dir_all(&dir).ok();

    let level = effective_level();

    let appender = RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("sandbox-cli.log")
        .max_log_files(MAX_LOG_DAYS as usize)
        .build(&dir)
        .expect("Failed to create CLI log appender");

    let (writer, guard) = tracing_appender::non_blocking(appender);

    let file_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let stderr_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let subscriber = tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(file_filter),
        )
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(stderr_filter),
        );

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");

    guard
}
