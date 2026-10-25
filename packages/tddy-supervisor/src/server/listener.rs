use crate::authz::PeerIdentity;
use crate::config::SocketConfig;
use crate::proto::supervisor::SupervisorServiceServer;
use crate::server::rpc::SupervisorServiceImpl;
use crate::server::PrivilegedSurface;
use crate::socket::{resolve_socket_source, SocketSource};
use crate::spawn_broker::accounts;
use std::os::fd::{AsRawFd, FromRawFd};
use std::sync::Arc;
use std::time::Duration;
use tddy_stdio::StdioEndpoint;
use tokio::net::UnixListener;

/// Bind the privileged socket, or adopt the one systemd created for us.
///
/// When systemd handed over a listener it also owns the socket node's ownership and mode, so
/// nothing here creates a directory, unlinks a stale file, or chmods anything.
pub fn bind_privileged_listener(socket: &SocketConfig) -> anyhow::Result<UnixListener> {
    let mode = socket
        .mode_bits()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let source = resolve_socket_source(
        std::process::id(),
        std::env::var("LISTEN_PID").ok().as_deref(),
        std::env::var("LISTEN_FDS").ok().as_deref(),
        &socket.path,
    );

    match source {
        SocketSource::Activated(fd) => {
            // Consume the activation environment so managed services do not inherit it and adopt
            // whatever fd 3 happens to be for them.
            std::env::remove_var("LISTEN_PID");
            std::env::remove_var("LISTEN_FDS");
            std::env::remove_var("LISTEN_FDNAMES");

            let listener = adopt_activation_listener(fd)?;
            log::info!(
                target: "tddy_supervisor::server",
                "adopted the systemd activation listener on fd {fd}"
            );
            Ok(listener)
        }
        SocketSource::SelfBind(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    anyhow::anyhow!("create socket directory {}: {error}", parent.display())
                })?;
            }
            // A socket left behind by a previous run would make `bind` fail with EADDRINUSE.
            let _ = std::fs::remove_file(&path);
            let listener = UnixListener::bind(&path)
                .map_err(|error| anyhow::anyhow!("bind {}: {error}", path.display()))?;
            apply_socket_ownership(&path, socket.group.as_deref(), mode)?;
            log::info!(
                target: "tddy_supervisor::server",
                "listening on {} (mode {:o})",
                path.display(),
                mode
            );
            Ok(listener)
        }
    }
}

/// Take ownership of a listener systemd handed over, and make it safe to keep.
///
/// Two things have to be true of the adopted descriptor, and systemd guarantees neither:
///
/// * **Non-blocking**, because tokio drives it.
/// * **Close-on-exec.** Activation descriptors arrive with `FD_CLOEXEC` *clear* — `sd_listen_fds(1)`
///   is what sets it for you, and this hand-rolled adoption is not that. Rust's `Command` manages
///   only stdio, so an inherited descriptor stays open across `exec`: without the flag, every child
///   the supervisor forks — a managed service, or a session running as another OS user — execs
///   holding a *listening* descriptor on the root broker's privileged socket, and can `accept()` the
///   daemon's connections to it. Measured, not inferred: before this, a managed service's fd 3 was
///   the same socket inode as the supervisor's listener.
pub(super) fn adopt_activation_listener(fd: std::os::fd::RawFd) -> anyhow::Result<UnixListener> {
    // SAFETY: `resolve_socket_source` returned `Activated` only because LISTEN_PID names this
    // process and LISTEN_FDS >= 1, which is systemd's guarantee that `fd` is an open listening
    // AF_UNIX socket. Ownership is taken here and the raw fd is never touched again except through
    // the listener that now owns it.
    let inherited = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
    inherited
        .set_nonblocking(true)
        .map_err(|error| anyhow::anyhow!("adopted activation socket: {error}"))?;
    // SAFETY: `inherited` owns the descriptor and keeps it open for the duration of the call.
    // `F_SETFD` writes only this descriptor's flags.
    let marked = unsafe { libc::fcntl(inherited.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    if marked != 0 {
        return Err(anyhow::anyhow!(
            "close-on-exec on the adopted activation socket: {}",
            std::io::Error::last_os_error()
        ));
    }
    UnixListener::from_std(inherited)
        .map_err(|error| anyhow::anyhow!("adopt activation listener: {error}"))
}

/// Serve the privileged surface until the listener itself becomes unusable.
///
/// Each connection gets its own [`SupervisorServiceImpl`] carrying that connection's peer
/// credentials, and its own task — a caller that stalls mid-frame cannot hold up anyone else.
pub async fn serve(listener: UnixListener, surface: Arc<PrivilegedSurface>) {
    let slots = ConnectionSlots::new(MAX_CONCURRENT_CONNECTIONS);
    loop {
        // A slot is taken *before* accepting, not after. At the cap the next caller waits in the
        // kernel's backlog, which is backpressure; accepting first and then finding nothing to serve
        // it with would mean spending a descriptor and a task on a connection anyway.
        if slots.available() == 0 {
            log::warn!(
                target: "tddy_supervisor::server",
                "the privileged surface is serving its maximum of {MAX_CONCURRENT_CONNECTIONS} \
                 connections; no further connection is accepted until one of them finishes"
            );
        }
        let Some(slot) = slots.acquire().await else {
            log::error!(
                target: "tddy_supervisor::server",
                "the connection limiter was closed; the privileged surface is no longer accepting"
            );
            return;
        };

        let stream = match listener.accept().await {
            Ok((stream, _address)) => stream,
            Err(error) => match classify_accept_failure(&error) {
                AcceptFailure::RetryImmediately => {
                    // One caller's connection died on its way in, or a signal arrived. The listener
                    // is untouched, and the next `accept` is the one that matters.
                    log::debug!(
                        target: "tddy_supervisor::server",
                        "a connection to the privileged socket was lost before it was served: {error}"
                    );
                    continue;
                }
                AcceptFailure::RetryAfterBackoff => {
                    // A shortage, not a broken listener: retrying without pausing would spin a core
                    // for as long as it lasts, and the shortage clears when something else releases
                    // what it is holding.
                    log::warn!(
                        target: "tddy_supervisor::server",
                        "accept on the privileged socket failed for lack of a host resource, \
                         retrying in {ACCEPT_BACKOFF:?}: {error}"
                    );
                    drop(slot);
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                }
                AcceptFailure::Fatal => {
                    // The listener is the thing that is broken, so every later `accept` fails the
                    // same way. Retrying would be an infinite loop that logs.
                    log::error!(
                        target: "tddy_supervisor::server",
                        "the privileged socket is no longer usable, so the supervisor is no longer \
                         serving it: {error}"
                    );
                    return;
                }
            },
        };

        // A connection whose credentials cannot be read is dropped rather than served: without
        // them the first gate has nothing to decide on.
        let peer = match stream.peer_cred() {
            Ok(credentials) => PeerIdentity {
                uid: credentials.uid(),
                gid: credentials.gid(),
                pid: credentials.pid().unwrap_or(0) as u32,
            },
            Err(error) => {
                log::warn!(
                    target: "tddy_supervisor::server",
                    "dropped a connection with unreadable peer credentials: {error}"
                );
                continue;
            }
        };

        let (reader, writer) = stream.into_split();
        let service =
            SupervisorServiceServer::new(SupervisorServiceImpl::new(Arc::clone(&surface), peer));
        // The supervisor never calls back into a caller, so the client half of the endpoint is
        // dropped; the connection is request/response in one direction only.
        let (_client, endpoint) = StdioEndpoint::from_duplex(
            reader,
            writer,
            service,
            tddy_rpc::RequestTransport::UnixSocket,
        );
        tokio::spawn(async move {
            endpoint.run().await;
            // Held until the connection is finished with, which is what makes the cap a cap on
            // *concurrent* connections rather than on connections ever accepted.
            drop(slot);
        });
    }
}

/// How many connections the privileged surface serves at once.
///
/// Some cap is mandatory, because authorization is per-*request*: any peer that gets past the
/// socket's group check has its connection accepted and a task of its own for as long as it stays
/// connected, before it has asked for anything. Unbounded, a loop of `connect()` calls costs the
/// supervisor a descriptor and a task each until it runs out of descriptors.
///
/// 64 rather than a number fitted to a workload: the peers are the handful of processes that own a
/// declared service, and [`crate::SupervisorClient`] holds *one* connection per client for a burst of
/// requests. Sixty-four is far more than the daemon has ever needed at once, and small enough that
/// the surface cannot be made to exhaust the process's descriptors by connecting to it.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

/// How long the accept loop waits out a host resource shortage before trying again.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// Permits for the connections being served, one per connection, held for its lifetime.
#[derive(Debug, Clone)]
pub(super) struct ConnectionSlots {
    permits: Arc<tokio::sync::Semaphore>,
}

impl ConnectionSlots {
    pub(super) fn new(limit: usize) -> ConnectionSlots {
        ConnectionSlots {
            permits: Arc::new(tokio::sync::Semaphore::new(limit)),
        }
    }

    /// Wait for a free slot. `None` only if the limiter was closed, which nothing does.
    pub(super) async fn acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        Arc::clone(&self.permits).acquire_owned().await.ok()
    }

    pub(super) fn available(&self) -> usize {
        self.permits.available_permits()
    }
}

/// What an `accept` failure says about whether the listener is still worth using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcceptFailure {
    /// One inbound connection was lost, or a signal interrupted the call. The listener is fine.
    RetryImmediately,
    /// The host has run out of descriptors, memory or buffers. The listener is fine, but retrying
    /// without pausing would spin.
    RetryAfterBackoff,
    /// The listener itself is unusable, so every later `accept` fails identically.
    Fatal,
}

/// Decide what an `accept` failure means. Pure, so every branch is assertable.
///
/// The distinction is the whole point: treating a transient failure as fatal destroys the privileged
/// surface for the life of the process — its `JoinHandle` is only ever `abort`ed, so nothing restarts
/// it, and systemd goes on reporting the unit `active` — while treating a fatal one as transient turns
/// the accept loop into a spin.
pub(super) fn classify_accept_failure(error: &std::io::Error) -> AcceptFailure {
    match error.raw_os_error() {
        Some(libc::EMFILE | libc::ENFILE | libc::ENOBUFS | libc::ENOMEM) => {
            AcceptFailure::RetryAfterBackoff
        }
        Some(libc::ECONNABORTED | libc::EINTR | libc::EAGAIN) => AcceptFailure::RetryImmediately,
        // Anything else — `EBADF`, `EINVAL`, `ENOTSOCK`, or an errno this does not know — is treated
        // as a broken listener. Unknown failures fail closed: stopping is loud and bounded, whereas
        // retrying an error whose cause is unknown is a loop nothing breaks out of.
        _ => AcceptFailure::Fatal,
    }
}

/// Give the socket the group and mode the config asks for.
///
/// The socket stays owned by root; the group is the entire grant, which is why a missing group is
/// an error rather than something to shrug at.
fn apply_socket_ownership(
    path: &std::path::Path,
    group: Option<&str>,
    mode: u32,
) -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    if let Some(group) = group {
        let gid = accounts::resolve_group_gid(group)
            .map_err(|error| anyhow::anyhow!("socket group `{group}`: {error}"))?;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| anyhow::anyhow!("socket path contains a nul byte"))?;
        // SAFETY: `c_path` outlives the call; `-1` leaves the owning uid untouched.
        let changed = unsafe { libc::chown(c_path.as_ptr(), u32::MAX, gid) };
        if changed != 0 {
            return Err(anyhow::anyhow!(
                "chown {} to group `{group}`: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
    }

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| anyhow::anyhow!("chmod {}: {error}", path.display()))
}
