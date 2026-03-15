//! tddy-coder library — shared by tddy-coder and tddy-demo binaries.

pub mod config;
pub mod plain;
mod run;
mod tty;
mod web_server;

pub use config::{load_config, merge_config_into_args};
pub use run::{run_main, run_with_args, Args, CoderArgs, DemoArgs};
pub use tddy_core::{
    ActivityEntry, ActivityKind, AppMode, Presenter, PresenterState, PresenterView, UserIntent,
};
pub use tddy_tui::disable_raw_mode;
