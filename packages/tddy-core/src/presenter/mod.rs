//! Presenter module: application state and workflow orchestration (MVP pattern).

mod events;
mod intent;
mod presenter_impl;
mod state;
mod view;
mod workflow_runner;

pub(crate) use events::WorkflowEvent;

pub use intent::UserIntent;
pub use presenter_impl::Presenter;
pub use state::{ActivityEntry, ActivityKind, AppMode, PresenterState};
pub use view::PresenterView;
