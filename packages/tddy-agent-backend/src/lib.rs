//! The coding-agent backends — Claude Code, Claude over ACP, Codex, Codex over ACP, Cursor, and the
//! mock and stub backends — with the stream parsers for their CLI output, token accounting, and
//! the hook and argv builders they launch with.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod adapters;
pub mod agent_env;
pub mod backend;
pub mod claude_hooks;
pub mod cursor_hooks;
pub mod provider_route;
pub mod provider_route_file;
pub mod session_pointer;
pub mod spawn_env;
pub mod stream;
pub mod token_accounting;
pub mod turn_files;

// The storage layer and the tool-call channels a backend reports through, named at the `crate::`
// paths these modules used inside `tddy-core`.
use tddy_session_store::{atomic_file, error};
use tddy_toolcall::toolcall;

pub use backend::acp_driver::{
    AcpAgentCommand, AcpAgentHooks, AcpDriver, AcpSession, AcpStopReason, AcpToolStatus,
    AcpTurnEnd, AcpUpdate, AcpUpdateSink, AllowOnce, McpServerSpec, NoHooks, OpenSession,
    PermissionDecision, PermissionHandler, PermissionOptionView, PermissionRequest,
    SessionErrorVerdict, SESSION_OPEN_DEADLINE,
};
pub use backend::headless_driver::{
    AcpConversation, ConversationDriver, HeadlessCommand, HeadlessDriver,
};
pub use backend::{
    kill_running_turns, recipe_cli_name_from_selection_label, spawn_codex_oauth_login,
    toml_route_overrides, workflow_recipe_selection_question, AgentOutputSink,
    ClarificationQuestion, CodingBackend, InMemoryToolExecutor, InvokeRequest, InvokeResponse,
    MockBackend, ProcessToolExecutor, QuestionOption, RemoteToolEnv, RunningGroup, SessionMode,
    SharedBackend, StubBackend, ToolExecutor, CODEX_DEFAULT_CLI_BINARY,
    CODEX_OAUTH_AUTHORIZE_URL_FILENAME, CODEX_THREAD_ID_FILENAME,
};
// `#agent-tools 16/19`: the workflow backend built from any coding agent's spec.
pub use backend::spec_backend::{SpecAcpBackend, SpecBackend, SpecBackendRefusal, SpecTransport};
// `#agent-tools 15/19`: the one headless workflow backend, configured by an agent's spec.
pub use backend::spec_headless::{
    SpecHeadlessBackend, DEFAULT_TURN_DEADLINE, MODEL_LISTING_DEADLINE,
};
pub use claude_hooks::{
    build_claude_hooks_settings, HookCommandParams, AWAIT_CLIENT_MARGIN_SECS,
    CLAUDE_HOOK_TIMEOUT_MARGIN_SECS,
};
pub use cursor_hooks::build_cursor_hooks_settings;
pub use stream::ProgressEvent;
