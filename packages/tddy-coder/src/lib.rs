//! tddy-coder library — shared by tddy-coder and tddy-demo binaries.

pub mod config;
pub mod plain;
pub mod recipe;
mod run;
mod tty;
pub mod web_server;

pub use config::{load_config, merge_config_into_args};
pub use recipe::{
    default_unspecified_workflow_recipe_cli_name, resolve_workflow_recipe_from_cli_name,
    WorkflowRecipeResolver,
};
pub use run::{
    merge_session_coder_config_for_resume, run_main, run_with_args, Args, CoderArgs, DemoArgs,
};
pub use tddy_core::{
    ActivityEntry, ActivityKind, AppMode, Presenter, PresenterState, PresenterView, UserIntent,
};
pub use tddy_tui::disable_raw_mode;
