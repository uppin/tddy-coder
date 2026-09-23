//! The coding-agent backends — Claude Code, Claude over ACP, Codex, Codex over ACP, Cursor, and the
//! mock and stub backends — with the stream parsers for their CLI output, token accounting, and
//! the hook and argv builders they launch with.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod backend;
pub mod claude_argv;
pub mod claude_hooks;
pub mod cursor_hooks;
pub mod spawn_env;
pub mod stream;
pub mod token_accounting;

// The storage layer and the tool-call channels a backend reports through, named at the `crate::`
// paths these modules used inside `tddy-core`.
use tddy_session_store::{atomic_file, error};
use tddy_toolcall::toolcall;

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
pub use stream::ProgressEvent;
