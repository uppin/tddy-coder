//! The transports, and what shutting them down has to take with it.
//!
//! Every transport here serves **the same `Arc<CodeIndexServiceImpl>`**. That is not an
//! optimisation: the warm index, the per-root queue that keeps two callers off one journal, and the
//! record of which roots this process holds are all state on that one instance, so a second
//! instance over the same `LspRegistry` would share the servers and none of the rest.
//! `CodeIndexServiceServer::from_arc` and `CodeIndexServiceTonicAdapter::new` both take an
//! already-shared handle for exactly this.
//!
//! Shutdown is the other half. The language servers are *children of this process* — a
//! multi-gigabyte rust-analyzer per root — so leaving them behind is the defect
//! `docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md` records
//! against `tddy-daemon`, and this path deliberately does not inherit it.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use tddy_index_daemon::proto::code_index::{CodeIndexServiceServer, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::CodeIndexServiceImpl;
use tddy_lsp::LspRegistry;
use tddy_task::TaskRegistry;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::cli::Transports;

/// Serve `service` over every transport `transports` names, until one of them stops or this
/// process is asked to.
pub(crate) async fn serve(
    transports: Transports,
    service: Arc<CodeIndexServiceImpl>,
    servers: LspRegistry,
    tasks: TaskRegistry,
) -> anyhow::Result<()> {
    let stop = CancellationToken::new();
    let mut serving: JoinSet<anyhow::Result<()>> = JoinSet::new();

    if let Some(address) = transports.grpc {
        let service = Arc::clone(&service);
        let stop = stop.clone();
        serving.spawn(async move { over_tcp(address, service, stop).await });
    }
    if let Some(path) = transports.grpc_uds.clone() {
        let service = Arc::clone(&service);
        let stop = stop.clone();
        serving.spawn(async move { over_unix_socket(&path, service, stop).await });
    }
    if transports.stdio {
        let service = Arc::clone(&service);
        serving.spawn(async move { over_process_stdio(service).await });
    }

    // Whichever ends first ends the process. A transport that has stopped serving is not a
    // condition to carry on under, and stdin reaching end of file is how a `--stdio` caller says it
    // is finished with this process.
    let outcome = tokio::select! {
        ended = serving.join_next() => match ended {
            Some(Ok(outcome)) => outcome,
            Some(Err(failure)) => Err(anyhow::anyhow!("a transport did not complete: {failure}")),
            // `cli::lifetime_of` refuses a serving process with no transport, so an empty set here
            // would mean this binary contradicted its own mode selection.
            None => Err(anyhow::anyhow!("nothing was serving")),
        },
        () = interrupted() => {
            log::info!(target: crate::MAIN, "interrupted; stopping");
            Ok(())
        }
    };

    log::info!(target: crate::MAIN, "stopping the transports and the servers behind them");
    stop.cancel();
    // Aborting the transports drops every in-flight response stream's receiver with them, and a
    // send into a dropped receiver is the one disconnect signal an operation gets
    // (`operations::progress_into`) — so the runs still in flight cancel rather than carrying on
    // for nobody.
    serving.shutdown().await;
    // The servers next, because cancelling their tasks is what kills the child processes.
    servers.shutdown_all().await;
    cancel_every_remaining_task(&tasks).await;
    outcome
}

/// gRPC on a loopback or wildcard port.
async fn over_tcp(
    address: std::net::SocketAddr,
    service: Arc<CodeIndexServiceImpl>,
    stop: CancellationToken,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("bind the code_index gRPC listener on {address}"))?;
    // The bound address rather than the requested one, so `--grpc 127.0.0.1:0` reports the port it
    // actually got. Announced on stderr because stdout may be carrying RPC frames, and a caller
    // that has to discover this process's coordinate has nowhere else to read it from.
    let bound = listener.local_addr().context("the bound address")?;
    log::info!(target: crate::MAIN, "listening on {bound}");

    tonic::transport::Server::builder()
        .add_service(TonicCodeIndexServiceServer::new(
            CodeIndexServiceTonicAdapter::new(service),
        ))
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            stop.cancelled_owned(),
        )
        .await
        .context("serve code_index over gRPC")
}

/// gRPC on an AF_UNIX socket, for a caller that reaches this process by filesystem coordinate.
async fn over_unix_socket(
    path: &Path,
    service: Arc<CodeIndexServiceImpl>,
    stop: CancellationToken,
) -> anyhow::Result<()> {
    // A socket already at this path is refused rather than unlinked. Unlinking is the shorter
    // route and the wrong one: the file is indistinguishable from the socket of a daemon that is
    // still serving clients, and taking it away from that process would leave it running and
    // unreachable. A path left behind by a process that was killed outright is the operator's to
    // clear, and saying so beats guessing which case this is.
    if path.exists() {
        anyhow::bail!(
            "{} already exists: another index daemon may be serving on it. Remove the socket if \
             nothing is.",
            path.display()
        );
    }
    let listener = tokio::net::UnixListener::bind(path)
        .with_context(|| format!("bind the code_index gRPC listener on {}", path.display()))?;
    let _bound = BoundSocket(path.to_path_buf());
    log::info!(target: crate::MAIN, "listening on {}", path.display());

    tonic::transport::Server::builder()
        .add_service(TonicCodeIndexServiceServer::new(
            CodeIndexServiceTonicAdapter::new(service),
        ))
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::UnixListenerStream::new(listener),
            stop.cancelled_owned(),
        )
        .await
        .context("serve code_index over gRPC on a unix socket")
}

/// The socket path this process bound, unlinked when it stops holding it.
///
/// A guard rather than a line after the serve, because shutdown *aborts* the transport: the serve
/// future is dropped where it is awaiting, so anything written after that await would never run.
/// Dropping the future drops this, which is the one cleanup hook an aborted task still has — and it
/// has to run, because the next start refuses a path it finds occupied.
struct BoundSocket(std::path::PathBuf);

impl Drop for BoundSocket {
    fn drop(&mut self) {
        if let Err(failure) = std::fs::remove_file(&self.0) {
            log::warn!(
                target: crate::MAIN,
                "could not remove {}: {failure}", self.0.display()
            );
        }
    }
}

/// The service over this process's own stdin and stdout.
///
/// Ends when stdin reaches end of file, which is how the caller that piped this process says it is
/// done — and which is why nothing here waits on `stop`.
async fn over_process_stdio(service: Arc<CodeIndexServiceImpl>) -> anyhow::Result<()> {
    let (_client, endpoint) =
        tddy_stdio::StdioEndpoint::from_process_stdio(CodeIndexServiceServer::from_arc(service));
    log::info!(
        target: crate::MAIN,
        "serving {} over this process's stdio", tddy_index_daemon::CODE_INDEX_SERVICE
    );
    endpoint.run().await;
    log::info!(target: crate::MAIN, "stdio input closed");
    Ok(())
}

/// Resolves when this process is asked to stop — `^C` or `SIGTERM`, the pair `tddy-daemon` waits
/// on.
async fn interrupted() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signals) => {
                signals.recv().await;
            }
            // Nothing can arrive on a signal stream that could not be installed, so this wait is
            // simply the one that never resolves.
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}

/// Cancel every task this process still holds.
///
/// [`LspRegistry::shutdown_all`] has already cancelled the language servers it knows about; this is
/// the sweep that means a task it did *not* know about — a server it had reaped from its own map,
/// or anything else registered here later — is cancelled rather than left running past the exit.
async fn cancel_every_remaining_task(tasks: &TaskRegistry) {
    for handle in tasks.list().await {
        if tasks.cancel_task(&handle.id).await {
            log::info!(
                target: crate::MAIN,
                "cancelled {} task {}", handle.kind, handle.id.0
            );
        }
    }
}
