//! Every log line a test binary emits, at every level — the once-per-binary logger
//! `login_opens_the_credential_store_acceptance.rs` and `pending_login_expiry_acceptance.rs` each
//! install a copy of, here for a binary that would otherwise need a third.

use std::sync::Mutex;

pub type Lines = Mutex<Vec<(log::Level, String)>>;

/// Every log line this test binary emits from here on, at every level.
pub fn captured_log() -> &'static Lines {
    if log::set_logger(&CAPTURING_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
    &LOGGED_LINES
}

/// Whether a line at `level` holds every one of `parts`.
pub fn logged(log: &Lines, level: log::Level, parts: &[&str]) -> bool {
    log.lock()
        .unwrap()
        .iter()
        .any(|(at, line)| *at == level && parts.iter().all(|part| line.contains(part)))
}

/// Every captured line, one per row, for a failure message.
pub fn everything(log: &Lines) -> String {
    log.lock()
        .unwrap()
        .iter()
        .map(|(level, line)| format!("{level} {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

static LOGGED_LINES: Lines = Mutex::new(Vec::new());
static CAPTURING_LOGGER: CapturingLogger = CapturingLogger;

struct CapturingLogger;

impl log::Log for CapturingLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        LOGGED_LINES.lock().unwrap().push((
            record.level(),
            format!("{} {}", record.target(), record.args()),
        ));
    }

    fn flush(&self) {}
}
