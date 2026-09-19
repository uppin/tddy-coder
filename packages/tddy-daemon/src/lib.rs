//! `tddy-daemon` endpoint — wiring, configuration, and transport only.

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

