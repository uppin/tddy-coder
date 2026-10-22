//! The tonic local socket the binary host serves beside HTTP.

use super::build_context::BuildContext;
use super::mount::Mounted;
use super::tasks::BinaryLocalSocketServices;
use super::*;

impl BuildContext {
    /// Local Unix-domain socket transport (SO_PEERCRED peer-trust + MintLocalToken). Served by the
    /// binary host only: the socket path names one daemon, and a systemd-activated listener is
    /// addressed to the binary's pid.
    pub(super) fn mount_local_socket(&mut self, m: &Mounted) {
        if self.options.host == RuntimeHost::Binary {
            self.tasks.local_socket = Some(LocalSocketTransport {
                socket_path: m.mount.config_arc.local_socket_path(),
                services: local_socket_services(m),
            });
        }
    }
}

/// The adapters the daemon's local socket serves.
///
/// Built from `connection_arc` — the same `CliSessionManager` and sandbox registry the
/// `terminal_session_entry` serves, so the socket and every other transport address one set of
/// PTYs and one control lease. The ports themselves hold no state: they are clones of those
/// managers' `Arc`s. The host and worktree adapters wrap the same `Arc`s the entries serve, so a
/// host prompt raised over the socket is the one a browser answers over HTTP. The session-agent and
/// activity adapters are likewise built from `connection_arc`, so the roster an in-jail agent reaches
/// over this socket is the one a browser sees over HTTP; they are the reason the socket's service
/// list is checked by an acceptance test: five of the nine family-B methods are what
/// `tddy-sandbox-runner`'s relay allowlist permits, and a coordinate missing here fails closed
/// inside a jail.
fn local_socket_services(m: &Mounted) -> BinaryLocalSocketServices {
    let config_arc = &m.mount.config_arc;
    let connection_arc = &m.hosted.connection_arc;
    let rpc_handlers = &m.hosted.rpc_handlers;
    let livekit_service = &m.livekit_service;
    let host_service_impl = &m.served.host_service_impl;
    let worktree_service_impl = &m.served.worktree_service_impl;
    let session_tokens = &m.mount.session_tokens;
    // The same signer the login flow mints with, so a locally minted token is one every
    // gate on the fleet already trusts.
    let signer = Some(session_tokens.signer().clone());
    let uid_to_username: tddy_session_lifecycle::local_token_tonic_adapter::UidToUsername =
        Arc::new(tddy_session_lifecycle::user_sessions_path::username_for_uid);
    crate::local_socket_server::LocalSocketServices {
        session: tddy_service::proto::session::SessionServiceTonicAdapter::new(Arc::new(
            connection_arc.session_lifecycle_service(),
        )),
        project: tddy_service::proto::project::ProjectServiceTonicAdapter::new(Arc::new(
            rpc_handlers.project_service(),
        )),
        demo_vm: tddy_service::proto::demo_vm::DemoVmServiceTonicAdapter::new(Arc::new(
            tddy_session_lifecycle::connection_service::DemoVmServiceImpl::new(
                connection_arc.demo_vm_service_state(),
            ),
        )),
        livekit: tddy_service::proto::livekit::LiveKitServiceTonicAdapter::new(Arc::clone(
            livekit_service,
        )),
        local_token:
            tddy_session_lifecycle::local_token_tonic_adapter::LocalTokenUdsTonicAdapter::new(
                config_arc.clone(),
                signer,
                uid_to_username,
            ),
        host: HostServiceTonicAdapter::new(Arc::clone(host_service_impl)),
        worktree: WorktreeServiceTonicAdapter::new(Arc::clone(worktree_service_impl)),
        terminal:
            tddy_terminal_rpc::proto::terminal_session::TerminalSessionServiceTonicAdapter::new(
                Arc::new(connection_arc.terminal_session_service()),
            ),
        session_agents:
            tddy_service::proto::session_agents_svc::SessionAgentServiceTonicAdapter::new(Arc::new(
                connection_arc.session_agents_service(),
            )),
        activity: tddy_service::proto::activity::ActivityServiceTonicAdapter::new(Arc::new(
            connection_arc.activity_service(),
        )),
        catalog: tddy_service::proto::catalog::CatalogServiceTonicAdapter::new(Arc::new(
            rpc_handlers.catalog_service(),
        )),
        exec_tools: tddy_service::proto::exec_tools::ExecToolServiceTonicAdapter::new(Arc::new(
            rpc_handlers.exec_tool_service(),
        )),
        pr_stack: tddy_service::proto::pr_stack::PrStackServiceTonicAdapter::new(Arc::new(
            rpc_handlers.pr_stack_service(),
        )),
    }
}
