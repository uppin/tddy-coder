//! Log backend: when TDDY_QUIET is set (TUI mode), buffer logs for display.
//! Otherwise write to stderr. Prevents stderr output from breaking ratatui layout.

use log::{Level, Log, Metadata, Record};
use std::sync::Mutex;

const MAX_BUFFER_LINES: usize = 200;

static LOG_BUFFER: once_cell::sync::Lazy<Mutex<Vec<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Vec::with_capacity(64)));

/// Logger that buffers when TDDY_QUIET is set, otherwise writes to stderr.
pub struct TddyLogger;

static LOGGER: TddyLogger = TddyLogger;

impl Log for TddyLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let msg = format!("{}", record.args());
        if std::env::var("TDDY_QUIET").is_ok() {
            let mut buf = match LOG_BUFFER.lock() {
                Ok(b) => b,
                Err(e) => e.into_inner(),
            };
            buf.push(msg);
            let excess = buf.len().saturating_sub(MAX_BUFFER_LINES);
            if excess > 0 {
                buf.drain(0..excess);
            }
        } else {
            eprintln!("[{}] {}", record.level(), msg);
        }
    }

    fn flush(&self) {}
}

/// Initialize the tddy logger. Call once at startup.
/// When `debug` is true (--debug flag), uses Debug level. Otherwise respects RUST_LOG or defaults to Info.
pub fn init_tddy_logger(debug: bool) {
    let level = if debug {
        log::LevelFilter::Debug
    } else {
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| {
                s.parse::<log::LevelFilter>()
                    .ok()
                    .or_else(|| match s.to_lowercase().as_str() {
                        "off" => Some(log::LevelFilter::Off),
                        "error" => Some(log::LevelFilter::Error),
                        "warn" => Some(log::LevelFilter::Warn),
                        "info" => Some(log::LevelFilter::Info),
                        "debug" => Some(log::LevelFilter::Debug),
                        "trace" => Some(log::LevelFilter::Trace),
                        _ => None,
                    })
            })
            .unwrap_or(log::LevelFilter::Info)
    };

    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(level));
}

/// Return recent buffered log lines for TUI display. Clears the buffer.
pub fn take_buffered_logs() -> Vec<String> {
    let mut buf = match LOG_BUFFER.lock() {
        Ok(b) => b,
        Err(e) => e.into_inner(),
    };
    std::mem::take(&mut *buf)
}

/// Return a clone of buffered log lines without clearing. For display during TUI draw.
pub fn get_buffered_logs() -> Vec<String> {
    let buf = match LOG_BUFFER.lock() {
        Ok(b) => b,
        Err(e) => e.into_inner(),
    };
    buf.clone()
}
