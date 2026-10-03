//! `tddy-daemon` endpoint — wiring, configuration, and transport only.

pub mod agent_tool_socket;
/// Background warm-up of a session's code index and its latest progress — see
/// [`code_index_warmup::warm_for_session`].
pub mod code_index_warmup;
pub mod code_navigation;
pub mod common_room_key_directory;
pub mod config;
pub mod daemon_config_service;
pub mod daemon_settings;
/// Lazy spawn, supervision, restart and idle stop of the `tddy-index-daemon` process this daemon
/// manages — see [`index_daemon::IndexDaemonRegistry`].
pub mod index_daemon;
mod index_daemon_body;
pub mod local_socket_server;
pub mod runtime;
pub mod server;
pub mod startup;
