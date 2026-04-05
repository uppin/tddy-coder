//! Core library for tddy-coder.

pub mod agent_skills;
pub mod backend;
pub mod changeset;
pub mod elapsed_format;
pub mod error;
pub mod log_backend;
pub mod output;
pub mod presenter;
pub mod session_lifecycle;
pub mod session_metadata;
pub mod source_path;
pub mod stream;
pub mod toolcall;
pub mod workflow;
pub mod worktree;

pub use agent_skills::{
    agents_skills_scan_cache_token, compose_prompt_skill_reference,
    compose_prompt_with_selected_skill, folder_name_matches_frontmatter_name,
    parse_skill_frontmatter, read_skill_markdown_body_for_compose, scan_skills_at_project_root,
    slash_menu_entries, slash_menu_items, DiscoveredSkill, InvalidSkillEntry,
    ParsedSkillFrontmatter, SkillMdParseError, SkillScanReport, SlashMenuEntry, SlashMenuItem,
    AGENTS_SKILLS_DIR,
};
pub use backend::{
    backend_from_label, backend_selection_question, build_claude_args, clear_child_pid,
    default_model_for_agent, get_child_pid, kill_child_process, preselected_index_for_agent,
    recipe_cli_name_from_selection_label, set_child_pid, workflow_recipe_selection_question,
    AgentOutputSink, AnyBackend, ClarificationQuestion, ClaudeAcpBackend, ClaudeCodeBackend,
    ClaudeInvokeConfig, CodexBackend, CodingBackend, CursorBackend, GoalHints, GoalId,
    InMemoryToolExecutor, InvokeRequest, InvokeResponse, MockBackend, PermissionHint,
    PermissionMode, ProcessToolExecutor, QuestionOption, SessionMode, SharedBackend, StubBackend,
    ToolExecutor, WorkflowRecipe,
};
pub use changeset::{
    append_session_and_update_state, get_session_for_tag, merge_persisted_workflow_into_context,
    read_changeset, resolve_agent_from_changeset, resolve_model, start_goal_for_session_continue,
    update_state, write_changeset, write_changeset_atomic, Changeset, ChangesetState,
    ChangesetWorkflow, ClarificationQa, ClarificationQuestionForQa, DiscoveryData,
    QuestionOptionForQa, SessionEntry, StateTransition,
};
pub use elapsed_format::format_elapsed_compact;
pub use error::{BackendError, ParseError, WorkflowError};
pub use log_backend::{
    config_has_file_output, default_log_config, find_matching_policy, get_buffered_logs,
    init_tddy_logger, init_tddy_logger_legacy, matches_selector, redirect_debug_output,
    resolve_log_defaults, resolve_logger, take_buffered_logs, DefaultLogPolicy, LogConfig,
    LogOutput, LogPolicy, LogRotation, LogSelector, LoggerDefinition, MatchedPolicy,
};
pub use presenter::{
    format_worktree_for_status_bar, ActivityEntry, ActivityKind, AgentOutputActivityLogMerge,
    AppMode, CriticalPresenterState, ExitAction, ModeChangedDetails, PendingWorkflowStart,
    Presenter, PresenterEvent, PresenterHandle, PresenterState, PresenterView, UserIntent,
    ViewConnection, WorkflowCompletePayload, WorkflowEvent,
};
pub use session_lifecycle::{
    materialize_unified_session_directory, resolve_effective_session_id, unified_session_dir_path,
    validate_session_id_segment, SessionIdValidationError, SessionLifecycleBootstrap,
    UnifiedSessionTreeBootstrap,
};
pub use session_metadata::{
    read_session_metadata, write_initial_tool_session_metadata, write_session_metadata,
    InitialToolSessionMetadataOpts, SessionMetadata, SESSION_METADATA_FILENAME,
};
pub use source_path::{classify_rust_source_path, RustSourcePathKind};
pub use stream::ProgressEvent;
pub use tddy_workflow::{
    canonical_artifact_write_path, read_session_artifact_utf8,
    read_session_artifact_utf8_or_placeholder, resolve_existing_session_artifact,
    session_artifacts_root, SESSION_ARTIFACT_READ_PLACEHOLDER,
};
pub use workflow::{
    engine::WorkflowEngine,
    find_git_root,
    graph::{ElicitationEvent, ExecutionResult, ExecutionStatus},
    ids::WorkflowState,
    session::{workflow_engine_storage_dir, WORKFLOW_ENGINE_STORAGE_SUBDIR},
    GoalOptions,
};
pub use worktree::{
    create_worktree, fetch_integration_base, fetch_origin_master, list_recent_remote_branches,
    list_worktrees, remove_worktree, resolve_default_integration_base_ref,
    resolve_persisted_worktree_integration_base_for_session, setup_worktree_for_session,
    setup_worktree_for_session_with_integration_base,
    setup_worktree_for_session_with_optional_chain_base, validate_chain_pr_integration_base_ref,
    validate_integration_base_ref, worktree_dir, WorktreeInfo,
    DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF,
};

#[cfg(test)]
mod workflow_decouple_acceptance {
    /// After decoupling, the legacy session primary-document path helper must not be re-exported from the crate root.
    #[test]
    fn core_src_free_of_prd_path_helper() {
        let lib_rs = include_str!("lib.rs");
        let forbidden = [
            "pub ",
            "use session_plan_prd::",
            "plan_prd_path_for_session_dir",
        ]
        .concat();
        assert!(
            !lib_rs.contains(&forbidden),
            "tddy-core lib.rs must not re-export the legacy session_plan_prd helper; use workflow manifest resolvers"
        );
    }
}
