//! tddy-coder library — shared by tddy-coder and tddy-demo binaries.

pub mod plain;
pub mod tui;

mod run;

pub use run::{run_plan_via_flow_runner, run_with_args, Args, CoderArgs, DemoArgs};
pub use tui::raw::disable_raw_mode;
