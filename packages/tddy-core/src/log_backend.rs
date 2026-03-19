//! Log backend: when TDDY_QUIET is set (TUI mode), buffer logs for display.
//! Otherwise write to stderr. When debug_output_path is set, redirect logs to file.
//! Prevents stderr output from breaking ratatui layout.

use log::{Level, Log, Metadata, Record};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

const MAX_BUFFER_LINES: usize = 200;

static LOG_BUFFER: once_cell::sync::Lazy<Mutex<Vec<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Vec::with_capacity(64)));

static DEBUG_OUTPUT_FILE: once_cell::sync::Lazy<Mutex<Option<std::fs::File>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

static WEBRTC_DEBUG_OUTPUT_FILE: once_cell::sync::Lazy<Mutex<Option<std::fs::File>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

/// True if the log message looks like libwebrtc output (e.g. "(connection.cc:985):").
fn is_webrtc_log(args: &std::fmt::Arguments) -> bool {
    let s = format!("{}", args);
    s.contains(".cc:") || s.contains(".cpp:")
}

/// Logger that buffers when TDDY_QUIET is set, writes to file when debug_output_path set,
/// otherwise writes to stderr.
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
        let ts = chrono::Local::now().format("%H:%M:%S%.3f");
        let line = format!("{} [{}] {}\n", ts, record.level(), record.args());
        let is_webrtc = is_webrtc_log(record.args());

        if is_webrtc {
            if let Ok(mut guard) = WEBRTC_DEBUG_OUTPUT_FILE.lock() {
                if let Some(ref mut f) = *guard {
                    let _ = f.write_all(line.as_bytes());
                    let _ = f.flush();
                    return;
                }
            }
        }

        if let Ok(mut guard) = DEBUG_OUTPUT_FILE.lock() {
            if let Some(ref mut f) = *guard {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
                return;
            }
        }
        if std::env::var("TDDY_QUIET").is_ok() {
            let mut buf = match LOG_BUFFER.lock() {
                Ok(b) => b,
                Err(e) => e.into_inner(),
            };
            buf.push(line.trim_end().to_string());
            let excess = buf.len().saturating_sub(MAX_BUFFER_LINES);
            if excess > 0 {
                buf.drain(0..excess);
            }
        } else {
            eprint!("{}", line);
        }
    }

    fn flush(&self) {}
}

/// Resolve conversation_output and debug_output defaults to plan_dir/logs/ when not set.
/// Returns the resolved conversation output path. Call when plan_dir becomes known.
pub fn resolve_log_defaults(
    conversation_output_path: Option<std::path::PathBuf>,
    debug_output_path: Option<impl AsRef<Path>>,
    plan_dir: &Path,
) -> Option<std::path::PathBuf> {
    let logs = plan_dir.join("logs");
    if debug_output_path.is_none() {
        let _ = std::fs::create_dir_all(&logs);
        redirect_debug_output(&logs.join("debug.log"));
        log::set_max_level(log::LevelFilter::Debug);
    }
    if conversation_output_path.is_none() {
        let _ = std::fs::create_dir_all(&logs);
        Some(logs.join("conversation.jsonl"))
    } else {
        conversation_output_path
    }
}

/// Redirect debug output to a file without changing the log level.
/// Use when plan_dir becomes known after init_tddy_logger; early logs go to stderr/buffer,
/// subsequent logs go to the file.
/// Flushes any buffered log lines (from TUI mode) into the file so early messages are preserved.
pub fn redirect_debug_output(path: &Path) {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            let buffered = take_buffered_logs();
            for line in &buffered {
                let _ = writeln!(f, "{}", line);
            }
            if !buffered.is_empty() {
                let _ = f.flush();
            }
            if let Ok(mut guard) = DEBUG_OUTPUT_FILE.lock() {
                *guard = Some(f);
            }
        }
        Err(e) => {
            eprintln!(
                "[tddy-core] warning: could not open debug output file {:?}: {}",
                path, e
            );
        }
    }
}

/// Initialize the tddy logger. Call once at startup.
/// When `debug` is true (--debug flag), uses Debug level. Otherwise respects RUST_LOG or defaults to Info.
/// When `debug_output_path` is Some, enables Debug level and redirects logs to that file.
/// When `webrtc_debug_output_path` is Some, libwebrtc logs (connection.cc, etc.) go to that file instead.
pub fn init_tddy_logger(
    debug: bool,
    debug_output_path: Option<&Path>,
    webrtc_debug_output_path: Option<&Path>,
) {
    if let Some(path) = debug_output_path {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => {
                if let Ok(mut guard) = DEBUG_OUTPUT_FILE.lock() {
                    *guard = Some(f);
                }
            }
            Err(e) => {
                eprintln!(
                    "[tddy-core] warning: could not open debug output file {:?}: {}",
                    path, e
                );
            }
        }
    }
    if let Some(path) = webrtc_debug_output_path {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => {
                if let Ok(mut guard) = WEBRTC_DEBUG_OUTPUT_FILE.lock() {
                    *guard = Some(f);
                }
            }
            Err(e) => {
                eprintln!(
                    "[tddy-core] warning: could not open webrtc debug output file {:?}: {}",
                    path, e
                );
            }
        }
    }
    let level = if debug || debug_output_path.is_some() || webrtc_debug_output_path.is_some() {
        log::LevelFilter::Debug
    } else if std::env::var("TDDY_QUIET").is_ok() {
        // TUI mode: enable Debug from the start so early messages are buffered.
        // They'll be flushed to the debug file when redirect_debug_output opens it.
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
