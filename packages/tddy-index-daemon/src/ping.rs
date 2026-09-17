//! Is a daemon serving this socket? Dial it and ask.
//!
//! A third lifetime, and the shortest: it serves nothing, runs no operation, reaches no language
//! server and loads no crate graph. It connects, issues `Workspaces` — the cheapest call the
//! service has, answered from the registry the daemon already holds — and exits with what it
//! found.
//!
//! It exists because the two cheaper probes both lie. A pid says something with that number is
//! alive, which after a pid is reused says nothing at all; a socket **file** outlives the process
//! that bound it, so its presence says only that a daemon was here once. `run-index-daemon`
//! answered on both together and still reported healthy daemons that refused the next connection.
//! An answer is the only thing that is not an inference.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use hyper_util::rt::TokioIo;
use tddy_index_daemon::proto::code_index::WorkspacesRequest;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;

/// Dial `socket`, report what answered, and exit with whether anything did.
pub(crate) async fn run(socket: &Path) -> ExitCode {
    match warm_roots_on(socket).await {
        Ok(held) => {
            // stdout, because this is the answer the caller asked for — the same rule the rest of
            // this binary follows, and what lets a script read it.
            println!(
                "index daemon answering on {} — {held} warm workspace(s)",
                socket.display()
            );
            ExitCode::SUCCESS
        }
        Err(failure) => {
            // Not `log::error!`: a probe's refusal is its answer, and a caller that redirected the
            // log somewhere else still needs to read this one. The phrasing is deliberate — it has
            // to be distinguishable from a command line the binary rejected, which also exits
            // non-zero.
            eprintln!(
                "no index daemon answered on {}: {failure:#}",
                socket.display()
            );
            ExitCode::FAILURE
        }
    }
}

/// How many workspace roots the daemon on `socket` is holding, or why it could not be asked.
async fn warm_roots_on(socket: &Path) -> Result<usize> {
    let path = socket.to_path_buf();

    // The `tddy_sandbox_runner::connect_uds_channel` shape: tonic reaches an AF_UNIX socket only
    // through a connector of its own, and the authority in the URI is never dialled — the
    // connector ignores it and opens the path instead. Restated here rather than depended on: that
    // function lives in the sandbox runner, and putting the sandbox runner behind the index daemon
    // to borrow eight lines is the worse coupling.
    let channel = tonic::transport::Endpoint::try_from("http://127.0.0.1:50051")
        .context("build the uds endpoint")?
        .connect_with_connector(tower::service_fn(move |_| {
            let path = path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(&path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .context("connect to the socket")?;

    let held = CodeIndexServiceClient::new(channel)
        .workspaces(WorkspacesRequest {})
        .await
        .context("ask which workspaces it holds")?
        .into_inner();

    Ok(held.workspaces.len())
}
