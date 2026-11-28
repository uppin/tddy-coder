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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// The traffic log's writer: each record appended through the filesystem port
/// (one append per record), instead of a held-open buffered file.
struct TrafficLog {
    fs: std::sync::Arc<dyn tddy_fs::Fs>,
    path: PathBuf,
}

impl Write for TrafficLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.fs.append(&self.path, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Log target used by the [`rpc_trace!`] macro.
pub const TARGET: &str = "rpc_traffic";

/// A [`log::Log`] implementation that intercepts RPC traffic events
/// and writes them to a file while forwarding all events to an inner logger.
pub struct RpcTrafficCollector {
    inner: Box<dyn log::Log>,
    file: Mutex<TrafficLog>,
    start: Instant,
}

impl RpcTrafficCollector {
    /// Wrap an existing logger, adding RPC traffic file output.
    /// Creates `rpc-traffic.log` inside `log_dir`.
    pub fn wrap(log_dir: impl AsRef<Path>, inner: Box<dyn log::Log>) -> std::io::Result<Self> {
        Self::wrap_on(log_dir, inner, std::sync::Arc::new(tddy_fs::DiskFs))
    }

    /// [`RpcTrafficCollector::wrap`] with the filesystem port the traffic log runs through.
    pub fn wrap_on(
        log_dir: impl AsRef<Path>,
        inner: Box<dyn log::Log>,
        fs: std::sync::Arc<dyn tddy_fs::Fs>,
    ) -> std::io::Result<Self> {
        let dir = log_dir.as_ref();
        fs.create_dir_all(dir)?;
        let path = dir.join("rpc-traffic.log");
        // Opening is what proves the file can be appended to.
        fs.append(&path, b"")?;
        Ok(Self {
            inner,
            file: Mutex::new(TrafficLog { fs, path }),
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
        // Given a temp directory and a no-op inner logger
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

        // When wrapping the inner logger with RpcTrafficCollector
        let collector = RpcTrafficCollector::wrap(&dir, Box::new(Noop)).unwrap();

        // Then rpc-traffic.log is created in the directory
        assert!(dir.join("rpc-traffic.log").exists());
        drop(collector);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
