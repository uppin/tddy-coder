//! Restoring a session's **tab roster** on Resume (`#session-liveness` 4/4): after main's resume,
//! every other entry of `tabs.yaml` ([`tddy_core::tab_roster`]) is brought back under its recorded
//! terminal id, each as its launch chain allows.
//!
//! The rule per entry is [`restore_plan`] — pure, over the entry and a [`LiveTabs`] snapshot — and
//! [`restore_tab_roster`] carries it out, one [`RestoredTab`](tddy_service::proto::session::RestoredTab)
//! per entry. A tab that cannot be restored is dropped from the roster and reported; it never fails
//! the resume.
//!
//! PRD: docs/ft/daemon/session-liveness.md

mod new_tab;
mod on_resume;
mod plan;
mod roster;

pub use new_tab::{
    agent_launch_node, agent_tab_argv, new_agent_tab, tab_conversation_is_resumable, NewAgentTab,
};
pub use plan::{restore_plan, LiveTabs, TabRestore, RESUMABLE_PROPERTY};
pub use roster::{restore_tab_roster, RosterRestore};

#[cfg(test)]
#[path = "restore_tabs_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "restore_tabs_sandbox_tests.rs"]
mod sandbox_tests;

#[cfg(test)]
#[path = "restore_tabs_target_tests.rs"]
mod target_tests;
