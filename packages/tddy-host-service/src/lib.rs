//! The hosts a daemon knows: the durable registry, each host's tooling probe, its telemetry, the
//! prompts it raises and the ssh keys it can be given — served as `host.HostService`.
//!
//! Split out of `connection.ConnectionService` by `#unbundle` node 1. The host-key path
//! (`host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add`) travels with hosts rather than
//! with auth because `AddHostKey` and `ListHostKeyCandidates` are host-service methods, and moving
//! it here cuts the `host_tooling ⇄ ssh_agent` cycle as a side effect.

pub mod host_desktop_targets;
pub mod host_keypair;
pub mod host_messages;
pub mod host_private_key;
pub mod host_prompt_stream;
pub mod host_prompts;
pub mod host_registry;
pub mod host_session_service;
pub mod host_stats;
pub mod host_tooling;
pub mod multi_host;
pub mod remote_desktop_probe;
pub mod ssh_agent;
pub mod ssh_agent_add;

pub mod service;
pub mod stream;
/// Shared test helpers, ungated like `tddy_daemon::test_util` is: the integration suites in
/// `tests/` reach for them, and a `#[cfg(test)]` module is invisible from there.
pub mod test_util;

pub use service::{HostServiceImpl, HOST_CPU_INTERVAL, HOST_DISK_INTERVAL};
pub use stream::{MpscHostPromptStream, MpscHostStatsStream};

// The handler tests that came out of `connection_service` with the code they exercise. They drive
// `HostServiceImpl` through the `host.HostService` trait, which is what the service now answers on.
#[cfg(test)]
mod host_add_key_handler_tests;
#[cfg(test)]
mod host_stats_handler_unit_tests;
#[cfg(test)]
mod host_tooling_handler_unit_tests;
#[cfg(test)]
mod known_hosts_handler_unit_tests;
#[cfg(test)]
mod ssh_agent_block_handler_tests;
