//! tddy-coder library — shared by tddy-coder and tddy-demo binaries.

pub mod plain;
mod run;
mod tty;

pub use run::{run_with_args, Args, CoderArgs, DemoArgs};
pub use tddy_core::{
    ActivityEntry, ActivityKind, AppMode, Presenter, PresenterState, PresenterView, UserIntent,
};
pub use tddy_tui::disable_raw_mode;
