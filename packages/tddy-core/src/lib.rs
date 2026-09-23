//! Core library for tddy-coder.

pub use tddy_agent_backend::*;
pub use tddy_agent_skills::*;
pub use tddy_changeset::*;
pub use tddy_log::*;
pub use tddy_session_actions::*;
pub use tddy_session_worktree::*;
pub use tddy_toolcall::*;

pub mod atomic_file;
pub mod changeset;
pub mod error;
pub mod output;
pub mod post_workflow;
pub mod presenter;
pub mod ssh_exec;
#[cfg(test)]
pub(crate) mod test_support;
pub mod usage_watcher;
pub mod workflow;

pub use atomic_file::{write_atomic, write_atomic_labelled};
pub use error::{BackendError, ParseError, WorkflowError};
pub use post_workflow::{
    github_pr_operator_question, post_workflow_elicitation_step_order,
    post_workflow_github_pr_operator_elicitation_pending, post_workflow_pr_status_display_line,
    post_workflow_session_worktree_elicitation_pending, session_worktree_removal_question,
    should_prompt_session_worktree_removal, should_reprompt_github_pr_on_resume,
    GITHUB_PR_OPERATOR_LABEL_YES, SESSION_WORKTREE_LABEL_YES,
};
pub use presenter::{
    format_worktree_for_status_bar, ActivityEntry, ActivityKind, AgentOutputActivityLogMerge,
    AppMode, CriticalPresenterState, ExitAction, ModeChangedDetails, PendingWorkflowStart,
    Presenter, PresenterEvent, PresenterHandle, PresenterState, PresenterView, UserIntent,
    ViewConnection, WorkflowCompletePayload, WorkflowEvent,
};
pub use ssh_exec::{
    contain_remote_path, default_remote_repo_root, run_ssh_batch, shell_single_quote,
};
pub use tddy_workflow::{
    canonical_artifact_write_path, canonical_attachment_write_path, read_session_artifact_utf8,
    read_session_artifact_utf8_or_placeholder, resolve_existing_session_artifact,
    session_artifacts_root, session_attachments_root, SESSION_ARTIFACT_READ_PLACEHOLDER,
    SESSION_ATTACHMENTS_SUBDIR,
};
pub use workflow::{
    engine::WorkflowEngine,
    find_git_root,
    graph::{ElicitationEvent, ExecutionResult, ExecutionStatus},
    ids::{GoalId, WorkflowState},
    recipe::{GoalHints, PermissionHint, WorkflowRecipe},
    session::{workflow_engine_storage_dir, WORKFLOW_ENGINE_STORAGE_SUBDIR},
    start_goal_for_session_continue, GoalOptions,
};

#[cfg(test)]
mod workflow_decouple_acceptance {
    /// After decoupling, the legacy session primary-document path helper must not be re-exported from the crate root.
    #[test]
    fn core_src_free_of_prd_path_helper() {
        // Given
        let lib_rs = include_str!("lib.rs");
        let forbidden = [
            "pub ",
            "use session_plan_prd::",
            "plan_prd_path_for_session_dir",
        ]
        .concat();

        // Then
        assert!(
            !lib_rs.contains(&forbidden),
            "tddy-core lib.rs must not re-export the legacy session_plan_prd helper; use workflow manifest resolvers"
        );
    }
}
