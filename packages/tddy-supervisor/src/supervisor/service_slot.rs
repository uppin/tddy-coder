use crate::config::{ManagedService, ServiceSocket};
use crate::services::ServiceRuntime;
use crate::spawn_broker::accounts;
use crate::spawn_broker::accounts::TargetUser;
use crate::spawn_broker::{ChildOutput, EnvironmentBase, SocketHandover, SpawnPlan};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// One declared service: its declaration, its resolved account, and its lifecycle state.
pub(super) struct Slot {
    pub(super) service: ManagedService,
    pub(super) target: TargetUser,
    pub(super) runtime: ServiceRuntime,
    pub(super) started_at: Option<Instant>,
    /// The listener created for a service that declared a socket, held for as long as the supervisor
    /// runs so every start — including every restart — is handed the same one.
    pub(super) listener: Option<Arc<UnixListener>>,
    /// The per-OS-user host sockets this service declared, as `(os user, listener)`, created once
    /// and handed to every start like [`Self::listener`].
    pub(super) host_listeners: Vec<(String, Arc<UnixListener>)>,
}
impl Slot {
    pub(super) fn spawn_plan(&self, supervisor_socket: &Path) -> SpawnPlan {
        SpawnPlan {
            program: self.service.exec_start.clone(),
            args: self.service.args.clone(),
            env: self.service.env.clone(),
            working_dir: self
                .service
                .working_dir
                .clone()
                .unwrap_or_else(|| self.target.home.clone()),
            target: self.target.clone(),
            // Managed services live in the supervisor's own cgroup: they are the host's services,
            // not somebody's session, and nothing has asked for them to be limited.
            scope_procs: None,
            // So a managed service's output reaches the same journal the supervisor's does.
            output: ChildOutput::Inherit,
            // A declared service is root's own decision, and an operator who sets a variable on the
            // supervisor unit means it for the services under it. It is also the only child told
            // where the supervisor listens — see [`EnvironmentBase::Inherited`].
            environment: EnvironmentBase::Inherited {
                supervisor_socket: supervisor_socket.to_path_buf(),
            },
            // A managed service is a host service, not a sandboxed session.
            sandbox: None,
        }
    }

    /// The listener this service's socket declaration created, ready to hand to the next start.
    pub(super) fn socket_handover(&self) -> Option<SocketHandover> {
        let mut handover = SocketHandover::new(self.listener.clone()?);
        for (user, listener) in &self.host_listeners {
            handover = handover.with_host_session_socket(user, Arc::clone(listener));
        }
        Some(handover)
    }
}

/// Create the listening socket a service declared, as root, before the service exists.
///
/// This is the job `tddy-daemon.socket` used to do: root binds the socket so it can live in a
/// directory the service user cannot write, and the service is handed the listener instead of trying
/// to create one. The service's own account is never the owner — the mode, and the group when one is
/// declared, are the whole access grant.
///
/// Deliberately a second implementation of what `server::bind_privileged_listener` does for the
/// supervisor's own socket: that one also has to cope with systemd handing over a listener, which a
/// managed service's socket never involves.
// TODO(supervisor): fold both into one helper owned by `socket.rs`, so ownership and mode are
// applied by exactly one piece of code.
pub(super) fn bind_service_listener(
    service: &ManagedService,
    socket: &ServiceSocket,
) -> anyhow::Result<UnixListener> {
    let mode = socket
        .mode_bits()
        .map_err(|error| anyhow::anyhow!("service `{}`: {error}", service.name))?;

    if let Some(parent) = socket.path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            anyhow::anyhow!(
                "service `{}`: create socket directory {}: {error}",
                service.name,
                parent.display()
            )
        })?;
    }
    // A socket left behind by a previous run would make `bind` fail with EADDRINUSE.
    let _ = std::fs::remove_file(&socket.path);
    let listener = UnixListener::bind(&socket.path).map_err(|error| {
        anyhow::anyhow!(
            "service `{}`: bind {}: {error}",
            service.name,
            socket.path.display()
        )
    })?;
    apply_socket_ownership(&socket.path, socket.group.as_deref(), mode)
        .map_err(|error| anyhow::anyhow!("service `{}`: {error}", service.name))?;

    log::info!(
        target: "tddy_supervisor::supervisor",
        "created {} (mode {mode:o}) for service '{}'",
        socket.path.display(),
        service.name
    );
    Ok(listener)
}

/// Give a socket the group and mode its declaration asks for.
///
/// The socket stays owned by root, so a group that cannot be resolved is an error rather than
/// something to shrug at: it is the grant.
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
