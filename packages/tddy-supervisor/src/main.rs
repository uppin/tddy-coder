//! Thin entry point. All behavior lives in the library so it is reachable from tests.

use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use log::{Level, LevelFilter, Log, Metadata, Record};

#[derive(Parser, Debug)]
#[command(
    name = "tddy-supervisor",
    about = "Privileged broker and mini-init for the tddy stack"
)]
struct Args {
    /// Path to the root-owned supervisor configuration.
    #[arg(short, long, env = "TDDY_SUPERVISOR_CONFIG")]
    config: PathBuf,
}

/// Logs to stderr, which is where systemd expects a service's diagnostics and where the journal
/// picks them up. Hand-rolled rather than pulled from a crate: the supervisor is the one process
/// that runs as root, so its dependency list is worth keeping to what it cannot do without.
struct StderrLogger {
    /// Verbosity for the supervisor's own modules.
    own: LevelFilter,
    /// Verbosity for everything it links.
    dependencies: LevelFilter,
}

impl StderrLogger {
    fn level_for(&self, target: &str) -> LevelFilter {
        if target.starts_with(OWN_TARGET_PREFIX) {
            self.own
        } else {
            self.dependencies
        }
    }
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level_for(metadata.target())
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // A failed write has nowhere left to be reported, so it is dropped rather than panicking a
        // root process over a closed stderr.
        let _ = writeln!(
            std::io::stderr(),
            "[{:<5} {}] {}",
            record.level(),
            record.target(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Log target prefix of the supervisor's own modules.
const OWN_TARGET_PREFIX: &str = "tddy_supervisor";

/// Install the logger.
///
/// `RUST_LOG` (read as a bare level, `error`…`trace`) raises the verbosity of everything. Without
/// it the supervisor narrates its own lifecycle at `Info` while its dependencies are held to
/// `Warn`, for two reasons: the RPC layer logs a line per request at `Info`, which a privileged
/// broker has no business writing into the journal for every call; and a log sink is a synchronous
/// write, so an unread stderr pipe filling up would block the process that is logging.
fn install_logger() {
    let requested = std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| value.trim().parse::<Level>().ok())
        .map(|level| level.to_level_filter());
    // Leaked on purpose: `log` requires a logger that lives for the rest of the process, and this
    // one does — it is installed once at startup and never replaced.
    let logger: &'static StderrLogger = Box::leak(Box::new(StderrLogger {
        own: requested.unwrap_or(LevelFilter::Info),
        dependencies: requested.unwrap_or(LevelFilter::Warn),
    }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(logger.own.max(logger.dependencies));
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_logger();
    let args = Args::parse();
    tddy_supervisor::run(&args.config).await
}
