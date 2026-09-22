//! The tddy log sink: config-driven policies and outputs, and the guards that keep log output off
//! the stdio a TUI or an RPC relay owns.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod log_backend;
pub mod stdio_safety;

pub use log_backend::{
    config_has_file_output, default_log_config, find_matching_policy, get_buffered_logs,
    init_tddy_logger, init_tddy_logger_legacy, matches_selector, redirect_debug_output,
    resolve_log_defaults, resolve_logger, take_buffered_logs, DefaultLogPolicy, LogConfig,
    LogOutput, LogPolicy, LogRotation, LogSelector, LoggerDefinition, MatchedPolicy,
};
