//! Shared sandbox runner + host-side `SessionChannel` relay.
//!
//! The in-jail runner (gRPC `SessionChannel` server, claude PTY bridge, `HTTPS_PROXY` CONNECT
//! egress shim) and the host-side relay that turns CONNECT tunnels into real outbound sockets are
//! platform-agnostic. They live here so the Darwin Seatbelt backend, the Linux cgroups backend,
//! the daemon, the standalone app, and tests all reuse one implementation instead of duplicating
//! it.

pub mod host_relay;
pub mod runner;

pub use host_relay::{
    relay_egress_request, run_host_relay, HostRelayConfig, HostToolHandler, NullToolHandler,
};
pub use runner::{
    connect_sandbox_client, connect_sandbox_client_uds, resolve_secret_envs, run_sandbox_runner,
    SandboxRunnerArgs,
};

/// Re-exported so host-relay callers can implement [`HostToolHandler`] without depending on
/// `tddy-service` directly.
pub use tddy_service::proto::connection::ExecuteToolResponse;
