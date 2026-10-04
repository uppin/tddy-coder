//! The `restructure_*` session tools, answered on the host by the warm index the daemon manages.
//!
//! The tools reach the host the way the `Lsp*` tools do — an `ExecuteTool` relayed over the
//! session-tool transport — and `tddy_tool_engine` hands them to the registered
//! [`RestructureExecutor`]. This is that executor: it dials the index through the same
//! [`IndexChannel`] port [`crate::index_backed::IndexLspExecutor`] dials, roots every request at the
//! session's worktree **as the host resolved it**, and binds every plan or file path the agent names
//! with [`crate::index_backed::bind_to_session_worktree`] — one binding path for both tool families, so a path from the
//! jail never names a root and never leaves the worktree.
//!
//! # Tool → index request
//!
//! | Tool | Arguments | Index RPC |
//! |---|---|---|
//! | `restructure_load` | `plans: [path]` | `LoadPlans` |
//! | `restructure_check` | `plan`, `deep?`, `file_budget?` | `Check` (streamed) |
//! | `restructure_apply` | `plan`, `dry_run?`, `resume?`, `from?`, `stop_after?` | `Apply` (streamed) |
//! | `restructure_status` | `plan` | `PlanStatus` |
//! | `restructure_plans` | — | `ListPlans` |
//! | `restructure_anchors` | `file`, `items?: [name]`, `at?: range` | `Anchors` |
//!
//! Every path is sent relative to the worktree; `workspace_root` is always the worktree itself.
//!
//! # Results
//!
//! `restructure_check` and `restructure_apply` answer one **run** object, folded from the event
//! stream (indexing progress is dropped):
//!
//! ```json
//! {
//!   "plan": "plans/split.jsonl",
//!   "findings":   [{ "operation": 1, "detail": "…" }],
//!   "operations": [{ "index": 0, "op_id": "op-1", "kind": "move_item",
//!                    "files": ["src/a.rs"], "visibility": [], "rehearsed_only": false }],
//!   "notes":      ["…"],
//!   "outcome":    { "applied": 1, "total": 2, "stopped_early": false },
//!   "refusal":    null
//! }
//! ```
//!
//! A run the index refused mid-stream still answers `Ok`: the operations it applied before the
//! refusal are on disk and the agent needs them. `outcome` is then `null` and `refusal` is
//! `{ "class", "message", "stale" }` — `class` the gRPC status class in snake case
//! (`failed_precondition`, `invalid_argument`, …, the classes `tddy_index_daemon::status` assigns),
//! and, for a `failed_precondition`, `stale` the plan's held stale operations as `PlanStatus`
//! reports them (`[{ "op", "reason" }]`), so a stale operation is refused **by its id**.
//! When that `PlanStatus` lookup itself fails, the refusal carries `stale_error` (the message) and an
//! empty `stale` — the run's applied operations are never dropped for it.
//!
//! `restructure_load` and `restructure_plans` answer `{ "plans": [{ "plan", "ops", "dirty",
//! "stale": [{ "op", "reason" }] }] }`; `restructure_status` answers `{ "completed", "in_flight",
//! "pending", "failed", "stale" }`; `restructure_anchors` answers `{ "range", "anchor_json" }` in
//! the index's one-based byte coordinates — the coordinates a plan's anchors are written in.
//!
//! An `Err` is a refusal before the index was asked (a path outside the worktree, a missing
//! argument) or a failure to reach it.

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};
use tddy_core::toolcall::restructure::RestructureExecutor;
use tddy_index_daemon::proto::code_index;
use tddy_index_daemon::proto::code_index::restructure_event::Event;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tonic::transport::Channel;

use crate::index_backed::{bind_to_session_worktree, IndexChannel};

/// Answers the `restructure_*` tools from the warm index reached through an [`IndexChannel`].
pub struct IndexRestructureExecutor {
    index: Arc<dyn IndexChannel>,
}

impl IndexRestructureExecutor {
    /// An executor asking the index `index` connects to.
    #[must_use]
    pub fn new(index: Arc<dyn IndexChannel>) -> Self {
        Self { index }
    }

    /// A client on a fresh channel to the index.
    async fn client(&self) -> Result<CodeIndexServiceClient<Channel>, String> {
        let channel = self
            .index
            .connect()
            .await
            .map_err(|err| format!("index daemon: {err}"))?;
        Ok(CodeIndexServiceClient::new(channel))
    }
}

/// The daemon's own explanation of a failed call, without the gRPC status framing.
fn message_of(status: tonic::Status) -> String {
    status.message().to_string()
}

/// The gRPC status class in snake case, as `tddy_index_daemon::status` assigns them.
fn class_of(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "cancelled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid_argument",
        tonic::Code::DeadlineExceeded => "deadline_exceeded",
        tonic::Code::NotFound => "not_found",
        tonic::Code::AlreadyExists => "already_exists",
        tonic::Code::PermissionDenied => "permission_denied",
        tonic::Code::ResourceExhausted => "resource_exhausted",
        tonic::Code::FailedPrecondition => "failed_precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out_of_range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data_loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

fn required_str<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{name} is required"))
}

fn flag(args: &Value, name: &str) -> bool {
    args.get(name).and_then(Value::as_bool).unwrap_or(false)
}

fn count(args: &Value, name: &str) -> Result<Option<u32>, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| format!("{name} must be a non-negative integer")),
    }
}

fn strings(args: &Value, name: &str) -> Result<Vec<String>, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{name} must be a list of strings"))
            })
            .collect(),
        Some(_) => Err(format!("{name} must be a list of strings")),
    }
}

fn stale_json(stale: &[code_index::StaleOp]) -> Vec<Value> {
    stale
        .iter()
        .map(|stale| json!({ "op": stale.op, "reason": stale.reason }))
        .collect()
}

fn plans_json(answered: &code_index::PlansResponse) -> Value {
    let plans: Vec<Value> = answered
        .plans
        .iter()
        .map(|plan| {
            json!({
                "plan": plan.plan,
                "ops": plan.ops,
                "dirty": plan.dirty,
                "stale": stale_json(&plan.stale),
            })
        })
        .collect();
    json!({ "plans": plans })
}

/// Parses a range argument `{ "start": {"line", "column"}, "end": {…} }` in the index's one-based
/// byte coordinates.
fn range_of(value: &Value) -> Result<code_index::SourceRange, String> {
    let position = |name: &str| {
        let at = value
            .get(name)
            .ok_or_else(|| format!("at.{name} is required"))?;
        let part = |field: &str| {
            at.get(field)
                .and_then(Value::as_u64)
                .and_then(|number| u32::try_from(number).ok())
                .ok_or_else(|| format!("at.{name}.{field} must be a non-negative integer"))
        };
        Ok::<_, String>(code_index::SourcePosition {
            line: part("line")?,
            column: part("column")?,
        })
    };
    Ok(code_index::SourceRange {
        start: Some(position("start")?),
        end: Some(position("end")?),
    })
}

fn range_json(range: &Option<code_index::SourceRange>) -> Value {
    let position = |at: &Option<code_index::SourcePosition>| {
        at.as_ref()
            .map(|at| json!({ "line": at.line, "column": at.column }))
            .unwrap_or(Value::Null)
    };
    match range {
        Some(range) => json!({ "start": position(&range.start), "end": position(&range.end) }),
        None => Value::Null,
    }
}

/// What a streamed `Check` or `Apply` amounted to, folded from its events.
#[derive(Default)]
struct Run {
    findings: Vec<Value>,
    operations: Vec<Value>,
    notes: Vec<String>,
    outcome: Option<Value>,
    refusal: Option<tonic::Status>,
}

impl Run {
    fn fold(&mut self, event: code_index::RestructureEvent) {
        match event.event {
            // Indexing progress says nothing of the plan.
            None | Some(Event::Indexing(_)) => {}
            Some(Event::Finding(finding)) => self.findings.push(json!({
                "operation": finding.operation,
                "detail": finding.detail,
            })),
            Some(Event::Operation(applied)) => self.operations.push(json!({
                "index": applied.index,
                "op_id": applied.op_id,
                "kind": applied.kind,
                "files": applied.files,
                "visibility": applied.visibility,
                "rehearsed_only": applied.rehearsed_only,
            })),
            Some(Event::Note(note)) => self.notes.push(note),
            Some(Event::Outcome(outcome)) => {
                self.outcome = Some(json!({
                    "applied": outcome.applied,
                    "total": outcome.total,
                    "stopped_early": outcome.stopped_early,
                }));
            }
        }
    }
}

impl IndexRestructureExecutor {
    /// Folds a run's event stream, keeping what happened before a refusal.
    async fn run(
        mut events: tonic::Streaming<code_index::RestructureEvent>,
    ) -> Result<Run, String> {
        let mut run = Run::default();
        loop {
            match events.message().await {
                Ok(Some(event)) => run.fold(event),
                Ok(None) => return Ok(run),
                Err(status) => {
                    run.refusal = Some(status);
                    return Ok(run);
                }
            }
        }
    }

    /// The plan's stale operations as the index reports them now.
    async fn stale_of(&self, worktree: &Path, plan: &str) -> Result<Vec<Value>, String> {
        let held = self
            .client()
            .await?
            .plan_status(code_index::PlanStatusRequest {
                workspace_root: worktree.display().to_string(),
                plan: plan.to_string(),
            })
            .await
            .map_err(message_of)?
            .into_inner();
        Ok(stale_json(&held.stale))
    }

    /// The run object the tool answers; for a `failed_precondition` refusal, with the plan's stale
    /// operations as the index reports them now.
    async fn run_json(&self, worktree: &Path, plan: &str, run: Run) -> Result<Value, String> {
        let refusal = match &run.refusal {
            None => Value::Null,
            Some(status) => {
                // A failed lookup is reported beside the refusal, never instead of it: what the run
                // already did stays in the answer.
                let mut refusal = json!({
                    "class": class_of(status.code()),
                    "message": status.message(),
                    "stale": Vec::<Value>::new(),
                });
                if status.code() == tonic::Code::FailedPrecondition {
                    match self.stale_of(worktree, plan).await {
                        Ok(stale) => refusal["stale"] = json!(stale),
                        Err(err) => refusal["stale_error"] = json!(err),
                    }
                }
                refusal
            }
        };
        Ok(json!({
            "plan": plan,
            "findings": run.findings,
            "operations": run.operations,
            "notes": run.notes,
            "outcome": run.outcome,
            "refusal": refusal,
        }))
    }
}

/// One method per tool; `execute` only names which.
impl IndexRestructureExecutor {
    async fn load(&self, worktree: &Path, args: &Value) -> Result<Value, String> {
        let plans = strings(args, "plans")?
            .iter()
            .map(|plan| bind_to_session_worktree(worktree, plan))
            .collect::<Result<Vec<_>, _>>()?;
        let answered = self
            .client()
            .await?
            .load_plans(code_index::LoadPlansRequest {
                workspace_root: worktree.display().to_string(),
                plans,
            })
            .await
            .map_err(message_of)?
            .into_inner();
        Ok(plans_json(&answered))
    }

    async fn plans(&self, worktree: &Path) -> Result<Value, String> {
        let answered = self
            .client()
            .await?
            .list_plans(code_index::ListPlansRequest {
                workspace_root: worktree.display().to_string(),
            })
            .await
            .map_err(message_of)?
            .into_inner();
        Ok(plans_json(&answered))
    }

    async fn check(&self, worktree: &Path, args: &Value) -> Result<Value, String> {
        let plan = bind_to_session_worktree(worktree, required_str(args, "plan")?)?;
        let events = self
            .client()
            .await?
            .check(code_index::CheckRequest {
                workspace_root: worktree.display().to_string(),
                plan: plan.clone(),
                deep: flag(args, "deep"),
                file_budget: count(args, "file_budget")?.unwrap_or(0),
            })
            .await
            .map_err(message_of)?
            .into_inner();
        let run = Self::run(events).await?;
        self.run_json(worktree, &plan, run).await
    }

    async fn apply(&self, worktree: &Path, args: &Value) -> Result<Value, String> {
        let plan = bind_to_session_worktree(worktree, required_str(args, "plan")?)?;
        let events = self
            .client()
            .await?
            .apply(code_index::ApplyRequest {
                workspace_root: worktree.display().to_string(),
                plan: plan.clone(),
                dry_run: flag(args, "dry_run"),
                resume: flag(args, "resume"),
                from: count(args, "from")?,
                stop_after: count(args, "stop_after")?,
            })
            .await
            .map_err(message_of)?
            .into_inner();
        let run = Self::run(events).await?;
        self.run_json(worktree, &plan, run).await
    }

    async fn status(&self, worktree: &Path, args: &Value) -> Result<Value, String> {
        let plan = bind_to_session_worktree(worktree, required_str(args, "plan")?)?;
        let status = self
            .client()
            .await?
            .plan_status(code_index::PlanStatusRequest {
                workspace_root: worktree.display().to_string(),
                plan,
            })
            .await
            .map_err(message_of)?
            .into_inner();
        Ok(json!({
            "completed": status.completed,
            "in_flight": status.in_flight,
            "pending": status.pending,
            "failed": status.failed,
            "stale": stale_json(&status.stale),
        }))
    }

    async fn anchors(&self, worktree: &Path, args: &Value) -> Result<Value, String> {
        let file = bind_to_session_worktree(worktree, required_str(args, "file")?)?;
        let at = match args.get("at") {
            None | Some(Value::Null) => None,
            Some(range) => Some(range_of(range)?),
        };
        let answered = self
            .client()
            .await?
            .anchors(code_index::AnchorsRequest {
                workspace_root: worktree.display().to_string(),
                file,
                items: strings(args, "items")?,
                at,
            })
            .await
            .map_err(message_of)?
            .into_inner();
        Ok(json!({
            "range": range_json(&answered.range),
            "anchor_json": answered.anchor_json,
        }))
    }
}

#[async_trait::async_trait]
impl RestructureExecutor for IndexRestructureExecutor {
    async fn execute(
        &self,
        worktree: &Path,
        tool_name: &str,
        args: &Value,
    ) -> Result<Value, String> {
        match tool_name {
            "restructure_load" => self.load(worktree, args).await,
            "restructure_plans" => self.plans(worktree).await,
            "restructure_check" => self.check(worktree, args).await,
            "restructure_apply" => self.apply(worktree, args).await,
            "restructure_status" => self.status(worktree, args).await,
            "restructure_anchors" => self.anchors(worktree, args).await,
            other => Err(format!("{other} is not a restructure tool")),
        }
    }
}
