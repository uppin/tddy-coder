//! The five state groups [`crate::presenter::Presenter`]'s 37 fields divide into.
//!
//! `presenter_impl.rs` is 1,788 production lines: one struct with 37 fields and one `impl` with 46
//! methods. The fields are not arbitrary — the struct's **own doc comments already group them**,
//! naming the agent-activity provenance set, the three external-surface channels, and the fields
//! that exist only for the deferred-start path. What the comments describe, the type system does not.
//!
//! These are that grouping made explicit. Grouping fields into owned types is a type-level change no
//! assist expresses, so it is the one substantial hand-written part of `#carve` 5/11 — and it is what
//! `#carve` 9/11 partitions the 46 methods along.
//!
//! `state` and `tddy_data_dir` belong to none of these groups and stay directly on `Presenter`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use tokio::sync::{broadcast, oneshot};

use crate::agent_activity::AgentActivityRecord;
use crate::backend::{ClarificationQuestion, SharedBackend};
use crate::toolcall::{ToolCallRequest, ToolCallResponse};
use crate::workflow::recipe::WorkflowRecipe;

use super::presenter_impl::DeferredBackendFactory;
use super::presenter_impl::PendingWorkflowStart;
use super::state::CriticalPresenterState;
use super::{PresenterEvent, UserIntent, WorkflowCompletePayload, WorkflowEvent};

/// A running workflow: the channels it is driven through, where its artifacts go, and its result.
///
/// Twelve of the 37 fields, every one of them `workflow_*` or the receiver/sender pair that carries
/// one. Nothing here is read while no workflow is running.
#[derive(Default)]
pub struct WorkflowRun {
    pub event_rx: Option<mpsc::Receiver<WorkflowEvent>>,
    pub answer_tx: Option<mpsc::Sender<String>>,
    pub backend: Option<SharedBackend>,
    /// The directory receiving the workflow's artifacts, which may be `.`.
    pub output_dir: Option<PathBuf>,
    /// The directory holding `changeset.yaml`. Not the same as [`Self::output_dir`], which is why
    /// both exist: `output_dir` alone may be `.` while the changeset lives elsewhere.
    pub session_dir: Option<PathBuf>,
    pub conversation_output: Option<PathBuf>,
    pub debug_output: Option<PathBuf>,
    pub debug: bool,
    /// Stored when `WorkflowComplete` arrives, so the result can be printed on TUI exit.
    pub result: Option<Result<WorkflowCompletePayload, String>>,
    pub handle: Option<thread::JoinHandle<()>>,
    /// Stored for restart after a dequeued prompt.
    pub socket_path: Option<PathBuf>,
    /// Pre-set, to skip git fetch and worktree creation in hooks.
    pub worktree_dir: Option<PathBuf>,
}

/// The clarification questions awaiting an operator, and the answers collected so far.
///
/// Four fields that are meaningful only together: an index into a list, the answers gathered against
/// it, and whether the workflow thread is blocked waiting for the next prompt.
#[derive(Default)]
pub struct PendingQuestions {
    pub questions: Vec<ClarificationQuestion>,
    pub current_index: usize,
    pub collected_answers: Vec<String>,
    /// Set when `ClarificationNeeded` arrives with no questions — the workflow thread is blocked on
    /// `answer_rx` waiting for a free-prompting turn.
    pub awaiting_open_answer: bool,
}

/// What this session records about the agent's own tool calls, and where.
///
/// Four fields carry the provenance the struct's comments already state at length — the session
/// directory holds the log, the worktree holds the files, and every record is stamped against the
/// commit it ran upon. The remaining two ([`Self::output_buffer`] and
/// [`Self::output_partial_row_active`]) are the activity log's own render state, kept here because
/// they are written by the same path that records a tool call and read by nothing else.
pub struct ActivityRecorder {
    /// The session directory receiving `agent-activity.jsonl`. When set, the presenter persists
    /// the agent's own tool calls here and broadcasts them as [`PresenterEvent::AgentActivity`].
    /// `None` when the daemon, not the coder, executes tools.
    pub dir: Option<PathBuf>,
    /// The checkout the agent edits — **not** [`Self::dir`]. Every record written for this session
    /// is stamped against it: the commit it ran upon and the paths it declared.
    ///
    /// `None` when the caller wiring the session did not know the checkout. A record then carries an
    /// empty `head_commit` and no paths, which is the documented "could not resolve" value
    /// (`docs/ft/daemon/session-worktree-sync.md` AC1); nothing else is read in its place.
    pub worktree: Option<PathBuf>,
    /// Provenance written on persisted rows: `"coder"` or `"cursor-cli"`.
    pub source: String,
    /// Running rows awaiting their terminal `ToolResult`, keyed by `call_id`, so the terminal row
    /// carries the same tool name and input the coalescing read side expects.
    pub pending: HashMap<String, AgentActivityRecord>,
    pub output_buffer: String,
    /// Whether the last `activity_log` row is the in-progress agent line.
    pub output_partial_row_active: bool,
}

impl Default for ActivityRecorder {
    /// Records nothing until a session dir is wired, and attributes what it then records to the
    /// coder — the process that executes tools unless a caller says otherwise.
    fn default() -> Self {
        Self {
            dir: None,
            worktree: None,
            source: "coder".to_string(),
            pending: HashMap::new(),
            output_buffer: String::new(),
            output_partial_row_active: false,
        }
    }
}

/// The surfaces outside the presenter that read its events or send it intents.
///
/// Every field here is a seam to something else — a broadcast to gRPC subscribers, an intent channel
/// from an external view, the tool-call relay, and the critical state a lagged subscriber recovers
/// from.
#[derive(Default)]
pub struct ViewChannels {
    /// When set, events are broadcast to gRPC subscribers.
    pub broadcast_tx: Option<broadcast::Sender<PresenterEvent>>,
    /// When set, `connect_view` hands this to an external view to send intents back.
    pub intent_tx: Option<mpsc::Sender<UserIntent>>,
    /// Relay requests from `tddy-tools`: `Submit`, `Ask`, `Approve`.
    pub tool_call_rx: Option<mpsc::Receiver<ToolCallRequest>>,
    /// When set, answers go to a tool-call response rather than to the workflow's `answer_tx`.
    pub pending_tool_call_response: Option<PendingToolCallResponse>,
    /// Updated on every `GoalStarted`/`StateChanged` and read by views after a `Lagged`, which is
    /// the whole reason it is shared rather than owned.
    pub critical_state: Arc<std::sync::Mutex<CriticalPresenterState>>,
}

/// Which backend and recipe this session runs, including the deferred-start path.
///
/// Eight fields that exist because backend selection can be answered interactively *after* the
/// session has started, so the workflow has to be held until it is.
pub struct BackendSelection {
    /// When true, the next `AnswerSelect` resolves the interactive backend choice (session start).
    pub selection_pending: bool,
    /// When set, backend selection creates the backend and starts the held workflow.
    pub deferred_factory: Option<DeferredBackendFactory>,
    pub pending_start: Option<PendingWorkflowStart>,
    /// Overrides the per-backend default model after selection (CLI `--model`).
    pub deferred_cli_model: Option<String>,
    /// The active workflow definition (TDD, bug-fix, …).
    pub recipe: Arc<dyn WorkflowRecipe>,
    /// After `/recipe` from the feature slash menu: the operator is picking TDD vs bugfix.
    pub recipe_slash_selection_pending: bool,
    pub recipe_resolver: Option<Arc<RecipeResolverFn>>,
    /// Set when a non-`free-prompting` workflow was started via `/start-*`; cleared when
    /// `WorkflowComplete` restores free prompting, or on error.
    pub start_slash_structured_run_active: bool,
}

impl BackendSelection {
    /// The selection a session starts from: the recipe it was given, and nothing deferred.
    pub fn new(recipe: Arc<dyn WorkflowRecipe>) -> Self {
        Self {
            selection_pending: false,
            deferred_factory: None,
            pending_start: None,
            deferred_cli_model: None,
            recipe,
            recipe_slash_selection_pending: false,
            recipe_resolver: None,
            start_slash_structured_run_active: false,
        }
    }
}

/// Resolves a CLI recipe name to a [`WorkflowRecipe`], wired from `tddy-coder`.
pub type RecipeResolverFn = dyn Fn(&str) -> Result<Arc<dyn WorkflowRecipe>, String> + Send + Sync;

/// What an answer is currently owed to, when it is not owed to the workflow.
pub enum PendingToolCallResponse {
    /// An `Ask` from `tddy-tools`, awaiting free text.
    Ask(oneshot::Sender<ToolCallResponse>),
    /// An `Approve` from `tddy-tools`, awaiting a yes or no.
    Approve(oneshot::Sender<ToolCallResponse>),
}
