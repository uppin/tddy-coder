//! Core library for tddy-coder.

pub use tddy_agent_skills::*;
pub use tddy_changeset::*;
pub use tddy_log::*;

pub mod atomic_file;
pub mod backend;
pub mod base_sync;
pub mod changeset;
pub mod claude_argv;
pub mod claude_hooks;
pub mod cursor_hooks;
pub mod error;
pub mod git_head;
pub mod output;
pub mod post_workflow;
pub mod presenter;
pub mod session_action_jobs;
pub mod session_action_pipeline;
pub mod session_actions;
pub mod session_chain;
pub mod spawn_env;
pub mod ssh_exec;
pub mod stream;
#[cfg(test)]
pub(crate) mod test_support;
pub mod token_accounting;
pub mod toolcall;
pub mod usage_watcher;
pub mod workflow;
pub mod worktree;

pub use atomic_file::{write_atomic, write_atomic_labelled};
pub use backend::{
    backend_from_label, backend_selection_question, build_claude_args, clear_child_pid,
    default_model_for_agent, get_child_pid, kill_child_process, preselected_index_for_agent,
    recipe_cli_name_from_selection_label, set_child_pid, workflow_recipe_selection_question,
    AgentOutputSink, AnyBackend, ClarificationQuestion, ClaudeAcpBackend, ClaudeCodeBackend,
    ClaudeInvokeConfig, CodexAcpBackend, CodexBackend, CodingBackend, CursorBackend,
    InMemoryToolExecutor, InvokeRequest, InvokeResponse, MockBackend, PermissionMode,
    ProcessToolExecutor, QuestionOption, RemoteToolEnv, SessionMode, SharedBackend, StubBackend,
    ToolExecutor, CODEX_OAUTH_AUTHORIZE_URL_FILENAME, CODEX_THREAD_ID_FILENAME,
};
pub use claude_hooks::{build_claude_hooks_settings, HookCommandParams};
pub use cursor_hooks::build_cursor_hooks_settings;
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
pub use session_chain::{
    classify_stack_parent_route, integrate_chain_base_into_session_worktree_bootstrap,
    parent_is_pr_stack_orchestrator, pr_stack_node_for_spawn, resolve_chain_base_for_session_spawn,
    resolve_chain_base_ref, resolve_chain_integration_base_ref_from_parent_session,
    select_worktree_base_ref, spawn_chain_child_worktree, StackParentRoute,
};
pub use ssh_exec::{
    contain_remote_path, default_remote_repo_root, run_ssh_batch, shell_single_quote,
};
pub use stream::ProgressEvent;
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
#[allow(deprecated)]
pub use worktree::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF;
pub use worktree::{
    create_worktree, detect_default_remote_name, fetch_integration_base, fetch_origin_master,
    list_recent_remote_branches, list_recent_remote_branches_skip, list_worktrees,
    local_branch_name, local_branch_name_for_remote, push_new_branch_to_origin,
    push_new_branch_to_remote, remove_worktree, resolve_default_integration_base_ref,
    resolve_default_integration_base_ref_with_remote,
    resolve_persisted_worktree_integration_base_for_session, set_git_ssh_command,
    setup_worktree_for_session, setup_worktree_for_session_over_ssh,
    setup_worktree_for_session_with_integration_base,
    setup_worktree_for_session_with_optional_chain_base, validate_chain_pr_integration_base_ref,
    validate_integration_base_ref, worktree_dir, worktree_path_for_branch, WorktreeInfo,
    FALLBACK_DEFAULT_INTEGRATION_BASE_REF,
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
