//! The binary: two lifetimes over one implementation.
//!
//! With no transport argument it runs a single operation against the service trait **in process**
//! — prost structs in, prost structs out, no encode or decode — renders the result, and exits with
//! a status. With `--grpc`, `--grpc-uds` and/or `--stdio` it serves that same implementation and
//! stays alive. One code path, not two that must be kept in step.
//!
//! The two lifetimes differ in three things and nothing else: how long the process lives, who the
//! caller is, and what a result *means* — a command line turns a refusal into an exit status, where
//! a served client is handed the refusal and decides for itself. The operations themselves are
//! reached identically.
//!
//! This file is the process; [`cli`] is the shape of its arguments, [`serve`] the transports,
//! [`single_shot`] the in-process call, and [`render`] what an operator is told. They are modules
//! of the binary rather than of the library because none of them is something a *host* of this
//! service needs — a host registers `build_code_index_entry` and owns its own console.

mod cli;
mod render;
mod serve;
mod single_shot;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tddy_index_daemon::{CodeIndexPorts, CodeIndexServiceImpl};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

use crate::cli::{IndexDaemonArgs, Lifetime};

/// The target every line this binary logs carries.
///
/// One target for the whole binary, distinct from the library's per-module targets
/// (`tddy_index_daemon::index`, `::operations`, `::apply`), so a log policy can turn the process's
/// own narration up or down without touching what the service says about a request.
pub(crate) const MAIN: &str = "tddy_index_daemon::main";

/// How long a language server may sit unused before its host is entitled to reap it.
///
/// Nothing in *this* process reaps: the PRD puts the index's lifecycle in `tddy-daemon`, which
/// "stops it when every workspace root has gone idle". This is the registry's own constructor
/// argument, set to what the one-shot command line already uses, so a root abandoned mid-session is
/// not pinned warm forever by a host that never asks.
///
/// TODO(M6): once `tddy-daemon` owns this process's lifecycle, decide whether the index daemon
/// should drive `LspRegistry::reap_idle` itself. Nothing in *this* process calls it yet, so the
/// timeout currently only bounds what a caller of that method would find stale. Both existing hosts
/// pair it with a 60-second ticker — `tddy-daemon/src/runtime.rs:310` and
/// `tddy-sandbox-app/src/main.rs:607` — so the pattern to copy already exists; the open question is
/// whether the reaper belongs here or in the host that owns this process's lifetime.
const A_SERVER_MAY_SIT_UNUSED_FOR: Duration = Duration::from_secs(600);

#[tokio::main]
async fn main() -> ExitCode {
    let args = IndexDaemonArgs::parse();
    let serving_over_stdio = args.stdio;
    let log_file = args.log_file.clone();

    // Before `init_tddy_logger`, because `log::set_logger` succeeds only once: under `--stdio` fd 1
    // carries RPC frames, and a logger configured to write there would corrupt every frame after
    // its first line. `tddy_core::default_log_config` does not produce a stdout logger today, so
    // this is a guard on the invariant rather than a repair of a known config — which is the point
    // of putting it where a future config path cannot get past it.
    let mut log_config = tddy_core::default_log_config(None, None);
    if serving_over_stdio {
        tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut log_config);
    }
    tddy_core::init_tddy_logger(log_config);

    // After the logger, so a file that cannot be created is reported on the stderr this process was
    // given rather than into the file it failed to open.
    if let Some(path) = &log_file {
        if let Err(failure) = send_stderr_to(path) {
            log::error!(target: MAIN, "{failure:#}");
            return ExitCode::FAILURE;
        }
    }

    let lifetime = match cli::lifetime_of(args) {
        Ok(lifetime) => lifetime,
        Err(refusal) => {
            log::error!(target: MAIN, "{refusal}");
            return ExitCode::FAILURE;
        }
    };

    let tasks = TaskRegistry::new();
    let servers = LspRegistry::new(
        rust_analyzer_as_this_crate_needs_it(),
        tasks.clone(),
        A_SERVER_MAY_SIT_UNUSED_FOR,
    );
    // One instance, whichever lifetime this is. Behind an `Arc` even for a single-shot run, because
    // the serving path needs the same handle in more than one place and a second construction
    // shape would be a second thing to keep in step.
    let service = Arc::new(CodeIndexServiceImpl::new(CodeIndexPorts {
        servers: servers.clone(),
    }));

    match lifetime {
        Lifetime::SingleShot(requested) => {
            let verdict = single_shot::run_once(service.as_ref(), requested).await;
            // The same cleanup a serving process does, for the same reason: this run may have
            // started a language server, and it is a child of this process either way.
            servers.shutdown_all().await;
            verdict.exit_code()
        }
        Lifetime::Serve(transports) => {
            match serve::serve(transports, service, servers, tasks).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(failure) => {
                    log::error!(target: MAIN, "{failure:#}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// Point this process's stderr at `path`.
///
/// fd 2 rather than the logger's destination, and deliberately so: redirecting the descriptor sends
/// *everything* that reaches stderr to the file — the log, a panic message, and any line a
/// dependency writes there without going through `log` — through a single writer, where pointing
/// the logger at the file as well would give the same file two independent offsets.
#[cfg(unix)]
fn send_stderr_to(path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::os::unix::io::AsRawFd;

    tddy_core::stdio_safety::redirect_fd_to_file(std::io::stderr().as_raw_fd(), path)
        .with_context(|| format!("send this process's stderr to {}", path.display()))
}

/// Refused rather than ignored: an operator who asked for a log file and silently did not get one
/// would read an empty file as a quiet process.
#[cfg(not(unix))]
fn send_stderr_to(path: &std::path::Path) -> anyhow::Result<()> {
    anyhow::bail!(
        "--log-file {} cannot be honoured on this platform: redirecting a file descriptor is a \
         unix operation",
        path.display()
    )
}

/// rust-analyzer, launched with the handshake the restructure backend needs.
///
/// `LspAllowList::rust_only` advertises nothing, and a server told nothing answers accordingly: no
/// code actions at all — which reads as a range that supports no refactoring — and positions
/// counted in utf-16 code units while this client counts bytes. Both are settled by the handshake,
/// which is why this process cannot share `tddy-daemon`'s registry: the handshake is fixed at spawn.
fn rust_analyzer_as_this_crate_needs_it() -> LspAllowList {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new("rust-analyzer")
            .with_capabilities(tddy_code_restructuring::client_capabilities())
            .with_initialization_options(tddy_code_restructuring::server_settings()),
    );
    allow
}
