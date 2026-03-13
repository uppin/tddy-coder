//! RPC traffic log collector.
//!
//! Routes `log::trace!` events with target [`TARGET`] to a dedicated
//! `rpc-traffic.log` file, independent of `RUST_LOG` configuration.
//!
//! ## Usage
//!
//! ```ignore
//! let inner = env_logger::Builder::new().parse_default_env().build();
//! let collector = RpcTrafficCollector::wrap("/tmp/session-logs", Box::new(inner))?;
//! collector.install();
//! ```
//!
//! Production code emits traffic via the [`rpc_trace!`] macro; the collector
//! writes those events to `rpc-traffic.log` inside the given directory and
//! forwards **all** events (including traffic) to the inner logger.

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

/// Log target used by the [`rpc_trace!`] macro.
pub const TARGET: &str = "rpc_traffic";

/// A [`log::Log`] implementation that intercepts RPC traffic events
/// and writes them to a file while forwarding all events to an inner logger.
pub struct RpcTrafficCollector {
    inner: Box<dyn log::Log>,
    file: Mutex<BufWriter<File>>,
    start: Instant,
}

impl RpcTrafficCollector {
    /// Wrap an existing logger, adding RPC traffic file output.
    /// Creates `rpc-traffic.log` inside `log_dir`.
    pub fn wrap(log_dir: impl AsRef<Path>, inner: Box<dyn log::Log>) -> std::io::Result<Self> {
        let dir = log_dir.as_ref();
        fs::create_dir_all(dir)?;
        let path = dir.join("rpc-traffic.log");
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            inner,
            file: Mutex::new(BufWriter::new(file)),
            start: Instant::now(),
        })
    }

    /// Install as the global logger.
    ///
    /// Sets max level to `Trace` so RPC traffic events are never
    /// filtered out by the log facade, regardless of `RUST_LOG`.
    pub fn install(self) -> Result<(), log::SetLoggerError> {
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(log::LevelFilter::Trace);
        Ok(())
    }
}

impl log::Log for RpcTrafficCollector {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target() == TARGET || self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if record.target() == TARGET {
            if let Ok(mut w) = self.file.lock() {
                let elapsed = self.start.elapsed();
                let _ = writeln!(w, "[{:>8.3}s] {}", elapsed.as_secs_f64(), record.args());
                let _ = w.flush();
            }
        }
        self.inner.log(record);
    }

    fn flush(&self) {
        if let Ok(mut w) = self.file.lock() {
            let _ = w.flush();
        }
        self.inner.flush();
    }
}

/// Log an RPC traffic event at `trace` level with the [`TARGET`] tag.
///
/// The [`RpcTrafficCollector`] intercepts these events and writes them
/// to `rpc-traffic.log`.  When no collector is installed the events are
/// plain `trace!` calls handled by whatever logger is active.
#[macro_export]
macro_rules! rpc_trace {
    ($($arg:tt)*) => {
        log::trace!(target: $crate::rpc_log::TARGET, $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_traffic_collector_creates_log_file() {
        let dir = std::env::temp_dir().join(format!("rpc_log_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        struct Noop;
        impl log::Log for Noop {
            fn enabled(&self, _: &log::Metadata) -> bool {
                false
            }
            fn log(&self, _: &log::Record) {}
            fn flush(&self) {}
        }

        let collector = RpcTrafficCollector::wrap(&dir, Box::new(Noop)).unwrap();
        assert!(dir.join("rpc-traffic.log").exists());
        drop(collector);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
