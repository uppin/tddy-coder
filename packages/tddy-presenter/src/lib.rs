//! The presenter: application state and workflow orchestration behind every view (MVP), the
//! post-workflow elicitation that follows a run, and the usage watcher that reports token spend.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod post_workflow;
pub mod presenter;
#[cfg(test)]
pub(crate) mod test_support;
pub mod usage_watcher;

// Every layer the presenter orchestrates, named at the `crate::` paths these modules used inside
// `tddy-core`.
use tddy_agent_backend::{
    backend, token_accounting, ClarificationQuestion, ProgressEvent, SharedBackend,
};
#[cfg(test)]
use tddy_agent_backend::{AnyBackend, QuestionOption, StubBackend};
use tddy_agent_skills::feature_start_slash;
use tddy_changeset::{agent_activity, changeset, session_metadata};
use tddy_changeset::{get_session_for_tag, read_changeset, write_changeset_atomic, GithubPrStatus};
use tddy_log::{redirect_debug_output, resolve_log_defaults};
use tddy_session_store::{atomic_file, output};
use tddy_session_worktree::git_head;
use tddy_toolcall::toolcall;
use tddy_workflow_engine::{workflow, WorkflowEngine, WorkflowRecipe};

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
