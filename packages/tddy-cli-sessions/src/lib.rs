//! The daemon's CLI session PTY runtime, split out of `tddy-session-lifecycle` by `#carve` 21/21.
pub mod cli_session_manager;
pub mod process_start;
pub mod session_toolcall;
pub mod tab_roster_store;
pub mod tmux;
pub mod tmux_location;

#[cfg(test)]
mod tmux_location_tests;
