//! What a session's `restructure_*` tools owe the agent: ask the warm index the daemon manages —
//! rooted at the session's own worktree, with the plan path bound on the host — and answer in
//! structured JSON: a check's findings, an apply's per-operation outcomes, a stale operation refused
//! by its id; and refuse a plan outside the worktree before the index hears of it.
//!
//! The executor under test is the one a host registers for these tools,
//! [`IndexRestructureExecutor`], asked through the [`RestructureExecutor`] port exactly as
//! `tddy_tool_engine`'s dispatch asks it. The index is a fake `code_index` server on an AF_UNIX
//! socket, dialled through the same kind of channel the daemon's `IndexDaemonRegistry::connect`
//! hands out, which records every request — so these tests pin this crate's translation, not the
//! index's own answers.

#![cfg(unix)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_core::toolcall::restructure::RestructureExecutor;
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::code_index::restructure_event::Event;
use tddy_index_daemon::proto::code_index::{CodeIndexService, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::EventStream;
use tddy_lsp_executor::index_backed::IndexChannel;
use tddy_lsp_executor::restructure_via_index::IndexRestructureExecutor;
use tempfile::TempDir;

// ---------------------------------------------------------------------------------------------
// Session worktrees

/// The plan every scenario names, relative to the session's worktree.
const THE_PLAN: &str = "plans/split.jsonl";

/// The operation of [`THE_PLAN`] the index finds stale, and the reason it gives.
const THE_STALE_OP: &str = "op-2";
const THE_STALE_REASON: &str = "item changed";

/// A session's worktree holding a two-operation restructure plan at [`THE_PLAN`].
struct ASessionWorktree {
    dir: TempDir,
}

fn a_session_worktree() -> ASessionWorktree {
    let dir = TempDir::new().expect("a session worktree");
    std::fs::create_dir_all(dir.path().join("plans")).expect("plans");
    std::fs::write(
        dir.path().join(THE_PLAN),
        concat!(
            r#"{"id":"op-1","op":"move_item","from":"src/lib.rs","to":"src/shapes.rs","items":["Circle"]}"#,
            "\n",
            r#"{"id":"op-2","op":"move_item","from":"src/lib.rs","to":"src/shapes.rs","items":["Square"]}"#,
            "\n",
        ),
    )
    .expect("the plan");
    ASessionWorktree { dir }
}

impl ASessionWorktree {
    /// The worktree as the host resolved it from the session — canonical, so the root the index is
    /// asked about is exactly the one handed to the executor.
    fn root(&self) -> PathBuf {
        self.dir
            .path()
            .canonicalize()
            .expect("an existing worktree")
    }

    fn root_string(&self) -> String {
        self.root().display().to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// The warm index: a fake `code_index` server that records what it is asked

/// Canned answers, and every restructure request the fake was asked.
#[derive(Clone, Default)]
struct AFakeIndex {
    check_events: Vec<index::RestructureEvent>,
    apply_events: Vec<index::RestructureEvent>,
    /// When set, the apply stream ends with this refusal instead of an outcome.
    apply_refused_with: Option<tddy_rpc::Status>,
    plan_status_answer: index::PlanStatusResponse,
    /// When set, `PlanStatus` fails with this instead of answering.
    plan_status_failure: Option<tddy_rpc::Status>,
    plans_answer: index::PlansResponse,
    anchors_answer: index::AnchorsResponse,
    asked: Arc<Mutex<Vec<AskedOfTheIndex>>>,
}

/// One request the fake index received, in arrival order.
#[derive(Debug, Clone, PartialEq)]
enum AskedOfTheIndex {
    Check(index::CheckRequest),
    Apply(index::ApplyRequest),
    PlanStatus(index::PlanStatusRequest),
    LoadPlans(index::LoadPlansRequest),
    ListPlans(index::ListPlansRequest),
    Anchors(index::AnchorsRequest),
}

fn a_fake_index() -> AFakeIndex {
    AFakeIndex::default()
}

impl AFakeIndex {
    fn checking_with(mut self, events: Vec<index::RestructureEvent>) -> Self {
        self.check_events = events;
        self
    }

    fn applying_with(mut self, events: Vec<index::RestructureEvent>) -> Self {
        self.apply_events = events;
        self
    }

    fn refusing_the_apply_with(self, refusal: &str) -> Self {
        self.refusing_the_apply_as(tddy_rpc::Status::failed_precondition(refusal))
    }

    fn refusing_the_apply_as(mut self, refusal: tddy_rpc::Status) -> Self {
        self.apply_refused_with = Some(refusal);
        self
    }

    fn failing_plan_status_with(mut self, failure: tddy_rpc::Status) -> Self {
        self.plan_status_failure = Some(failure);
        self
    }

    fn answering_plans_with(mut self, answer: index::PlansResponse) -> Self {
        self.plans_answer = answer;
        self
    }

    fn answering_anchors_with(mut self, answer: index::AnchorsResponse) -> Self {
        self.anchors_answer = answer;
        self
    }

    fn answering_plan_status_with(mut self, answer: index::PlanStatusResponse) -> Self {
        self.plan_status_answer = answer;
        self
    }

    fn what_it_was_asked(&self) -> Vec<AskedOfTheIndex> {
        self.asked.lock().expect("the request log").clone()
    }

    fn record(&self, asked: AskedOfTheIndex) {
        self.asked.lock().expect("the request log").push(asked);
    }
}

/// `events`, then `refusal` when there is one, as a finished server stream.
fn a_stream_of(
    events: Vec<index::RestructureEvent>,
    refusal: Option<tddy_rpc::Status>,
) -> EventStream<index::RestructureEvent> {
    let (sender, receiver) = tokio::sync::mpsc::channel(events.len() + 1);
    for event in events {
        sender.try_send(Ok(event)).expect("room for every event");
    }
    if let Some(refusal) = refusal {
        sender.try_send(Err(refusal)).expect("room for the refusal");
    }
    tokio_stream::wrappers::ReceiverStream::new(receiver)
}

fn not_part_of_this_fake() -> tddy_rpc::Status {
    tddy_rpc::Status::unimplemented("not part of the fake index")
}

#[async_trait::async_trait]
impl CodeIndexService for AFakeIndex {
    type WarmStream = EventStream<index::IndexProgress>;
    type CheckStream = EventStream<index::RestructureEvent>;
    type ApplyStream = EventStream<index::RestructureEvent>;
    type CoverageStream = EventStream<index::AnalyzeEvent>;
    type DuplicateTestsStream = EventStream<index::AnalyzeEvent>;

    async fn check(
        &self,
        request: tddy_rpc::Request<index::CheckRequest>,
    ) -> Result<tddy_rpc::Response<Self::CheckStream>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::Check(request.into_inner()));
        Ok(tddy_rpc::Response::new(a_stream_of(
            self.check_events.clone(),
            None,
        )))
    }

    async fn apply(
        &self,
        request: tddy_rpc::Request<index::ApplyRequest>,
    ) -> Result<tddy_rpc::Response<Self::ApplyStream>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::Apply(request.into_inner()));
        Ok(tddy_rpc::Response::new(a_stream_of(
            self.apply_events.clone(),
            self.apply_refused_with.clone(),
        )))
    }

    async fn plan_status(
        &self,
        request: tddy_rpc::Request<index::PlanStatusRequest>,
    ) -> Result<tddy_rpc::Response<index::PlanStatusResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::PlanStatus(request.into_inner()));
        if let Some(failure) = self.plan_status_failure.clone() {
            return Err(failure);
        }
        Ok(tddy_rpc::Response::new(self.plan_status_answer.clone()))
    }

    async fn warm(
        &self,
        _request: tddy_rpc::Request<index::WarmRequest>,
    ) -> Result<tddy_rpc::Response<Self::WarmStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn anchors(
        &self,
        request: tddy_rpc::Request<index::AnchorsRequest>,
    ) -> Result<tddy_rpc::Response<index::AnchorsResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::Anchors(request.into_inner()));
        Ok(tddy_rpc::Response::new(self.anchors_answer.clone()))
    }

    async fn load_plans(
        &self,
        request: tddy_rpc::Request<index::LoadPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::LoadPlans(request.into_inner()));
        Ok(tddy_rpc::Response::new(self.plans_answer.clone()))
    }

    async fn unload_plans(
        &self,
        _request: tddy_rpc::Request<index::UnloadPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn list_plans(
        &self,
        request: tddy_rpc::Request<index::ListPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::ListPlans(request.into_inner()));
        Ok(tddy_rpc::Response::new(self.plans_answer.clone()))
    }

    async fn snapshot(
        &self,
        _request: tddy_rpc::Request<index::SnapshotRequest>,
    ) -> Result<tddy_rpc::Response<index::SnapshotResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn verify(
        &self,
        _request: tddy_rpc::Request<index::VerifyRequest>,
    ) -> Result<tddy_rpc::Response<index::VerifyResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn coverage(
        &self,
        _request: tddy_rpc::Request<index::CoverageRequest>,
    ) -> Result<tddy_rpc::Response<Self::CoverageStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn report(
        &self,
        _request: tddy_rpc::Request<index::ReportRequest>,
    ) -> Result<tddy_rpc::Response<index::ReportResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn duplicate_tests(
        &self,
        _request: tddy_rpc::Request<index::DuplicateTestsRequest>,
    ) -> Result<tddy_rpc::Response<Self::DuplicateTestsStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn complexity(
        &self,
        _request: tddy_rpc::Request<index::ComplexityRequest>,
    ) -> Result<tddy_rpc::Response<index::ComplexityResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn workspaces(
        &self,
        _request: tddy_rpc::Request<index::WorkspacesRequest>,
    ) -> Result<tddy_rpc::Response<index::WorkspacesResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn definition(
        &self,
        _request: tddy_rpc::Request<index::DefinitionRequest>,
    ) -> Result<tddy_rpc::Response<index::DefinitionResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn references(
        &self,
        _request: tddy_rpc::Request<index::ReferencesRequest>,
    ) -> Result<tddy_rpc::Response<index::ReferencesResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn hover(
        &self,
        _request: tddy_rpc::Request<index::HoverRequest>,
    ) -> Result<tddy_rpc::Response<index::HoverResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn symbols(
        &self,
        _request: tddy_rpc::Request<index::SymbolsRequest>,
    ) -> Result<tddy_rpc::Response<index::SymbolsResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn diagnostics(
        &self,
        _request: tddy_rpc::Request<index::DiagnosticsRequest>,
    ) -> Result<tddy_rpc::Response<index::DiagnosticsResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }
}

const INDEX_SOCKET: &str = "index.sock";

/// `fake`, served on an AF_UNIX socket of its own and dialled per call, as the daemon's registry
/// dials the managed index daemon.
struct AnIndexOnASocket {
    dir: TempDir,
    /// The serving task, aborted on drop so the socket directory outlives it.
    server: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
}

async fn an_index_serving(fake: AFakeIndex) -> AnIndexOnASocket {
    let dir = TempDir::new().expect("a directory for the index socket");
    let listener = tokio::net::UnixListener::bind(dir.path().join(INDEX_SOCKET))
        .expect("the fake index binds");
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(TonicCodeIndexServiceServer::new(
                CodeIndexServiceTonicAdapter::new(Arc::new(fake)),
            ))
            .serve_with_incoming(tokio_stream::wrappers::UnixListenerStream::new(listener)),
    );
    AnIndexOnASocket { dir, server }
}

impl Drop for AnIndexOnASocket {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl AnIndexOnASocket {
    fn socket_path(&self) -> PathBuf {
        self.dir.path().join(INDEX_SOCKET)
    }
}

#[async_trait::async_trait]
impl IndexChannel for AnIndexOnASocket {
    async fn connect(&self) -> Result<tonic::transport::Channel, String> {
        let path = self.socket_path();
        tonic::transport::Endpoint::try_from("http://127.0.0.1:50051")
            .map_err(|err| err.to_string())?
            .connect_with_connector(tower::service_fn(move |_| {
                let path = path.clone();
                async move {
                    let stream = tokio::net::UnixStream::connect(&path).await?;
                    Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
                }
            }))
            .await
            .map_err(|err| err.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// The agent's tool calls

/// The executor a host that manages `index` registers for the `restructure_*` tools.
fn the_registered_executor(index: AnIndexOnASocket) -> Arc<dyn RestructureExecutor> {
    Arc::new(IndexRestructureExecutor::new(Arc::new(index)))
}

/// What the agent's `tool` call with `args` against `worktree` answers.
async fn the_agent_calls(
    executor: &Arc<dyn RestructureExecutor>,
    tool: &str,
    worktree: PathBuf,
    args: Value,
) -> Result<Value, String> {
    executor.execute(&worktree, tool, &args).await
}

// ---------------------------------------------------------------------------------------------
// Index events

fn a_finding(operation: u32, detail: &str) -> index::RestructureEvent {
    index::RestructureEvent {
        event: Some(Event::Finding(index::Finding {
            operation,
            detail: detail.to_string(),
        })),
    }
}

fn an_outcome(applied: u32, total: u32) -> index::RestructureEvent {
    index::RestructureEvent {
        event: Some(Event::Outcome(index::RunOutcome {
            applied,
            total,
            stopped_early: false,
        })),
    }
}

/// Operation `index` (`op_id`) of a two-operation plan, applied as the `done`th, moving `Circle` or
/// `Square` out of `src/lib.rs` into `src/shapes.rs`.
fn an_applied_operation(index: u32, op_id: &str) -> index::RestructureEvent {
    index::RestructureEvent {
        event: Some(Event::Operation(index::OperationApplied {
            index,
            done: index + 1,
            total: 2,
            kind: "move_item".to_string(),
            files: vec!["src/lib.rs".to_string(), "src/shapes.rs".to_string()],
            visibility: vec![],
            rehearsed_only: false,
            op_id: op_id.to_string(),
            group: String::new(),
        })),
    }
}

/// The per-operation outcome the tool reports for [`an_applied_operation`].
fn the_reported_operation(index: u32, op_id: &str) -> Value {
    json!({
        "index": index,
        "op_id": op_id,
        "kind": "move_item",
        "files": ["src/lib.rs", "src/shapes.rs"],
        "visibility": [],
        "rehearsed_only": false,
    })
}

/// How the index refuses a run whose next operation is stale (`RestructureError::StaleOperation`).
fn the_stale_refusal_of(op: &str) -> String {
    format!("operation `{op}` of {THE_PLAN} is stale ({THE_STALE_REASON}) — re-anchor it before applying")
}

fn a_plan_status_holding_stale(op: &str, reason: &str) -> index::PlanStatusResponse {
    index::PlanStatusResponse {
        completed: 1,
        in_flight: 0,
        pending: 1,
        failed: 0,
        stale: vec![index::StaleOp {
            op: op.to_string(),
            reason: reason.to_string(),
        }],
    }
}

/// A plan the index holds with `ops` operations, one of them stale.
fn a_loaded_plan_holding_stale(plan: &str, ops: u32, stale_op: &str) -> index::LoadedPlan {
    index::LoadedPlan {
        plan: plan.to_string(),
        ops,
        dirty: true,
        stale: vec![index::StaleOp {
            op: stale_op.to_string(),
            reason: THE_STALE_REASON.to_string(),
        }],
    }
}

/// The JSON the tools report for [`a_loaded_plan_holding_stale`].
fn the_reported_plan_holding_stale(plan: &str, ops: u32, stale_op: &str) -> Value {
    json!({
        "plan": plan,
        "ops": ops,
        "dirty": true,
        "stale": [{ "op": stale_op, "reason": THE_STALE_REASON }],
    })
}

fn a_range(from: (u32, u32), to: (u32, u32)) -> index::SourceRange {
    let position = |(line, column)| Some(index::SourcePosition { line, column });
    index::SourceRange {
        start: position(from),
        end: position(to),
    }
}

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn restructure_check_returns_findings_as_json() {
    // Given a session worktree holding a plan, and a warm index that finds a problem with its second
    // operation
    let worktree = a_session_worktree();
    let fake = a_fake_index().checking_with(vec![
        a_finding(1, "`Square` is not declared in src/lib.rs"),
        an_outcome(0, 2),
    ]);
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent checks the plan by its worktree-relative path
    let answer = the_agent_calls(
        &executor,
        "restructure_check",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then the index was asked to check that plan, rooted at the session's worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Check(index::CheckRequest {
            workspace_root: worktree.root_string(),
            plan: THE_PLAN.to_string(),
            deep: false,
            file_budget: 0,
        })]
    );
    // And the agent gets the findings as JSON
    assert_eq!(
        answer,
        Ok(json!({
            "plan": THE_PLAN,
            "findings": [{ "operation": 1, "detail": "`Square` is not declared in src/lib.rs" }],
            "operations": [],
            "notes": [],
            "outcome": { "applied": 0, "total": 2, "stopped_early": false },
            "refusal": null,
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restructure_apply_returns_per_operation_outcomes() {
    // Given a session worktree holding a two-operation plan, and a warm index that applies both
    let worktree = a_session_worktree();
    let fake = a_fake_index().applying_with(vec![
        an_applied_operation(0, "op-1"),
        an_applied_operation(1, "op-2"),
        an_outcome(2, 2),
    ]);
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent applies the plan
    let answer = the_agent_calls(
        &executor,
        "restructure_apply",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then the index was asked to apply that plan to the session's worktree, from the journal's next
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Apply(index::ApplyRequest {
            workspace_root: worktree.root_string(),
            plan: THE_PLAN.to_string(),
            dry_run: false,
            resume: false,
            from: None,
            stop_after: None,
        })]
    );
    // And the agent gets one outcome per applied operation, by id, and the run's outcome
    assert_eq!(
        answer,
        Ok(json!({
            "plan": THE_PLAN,
            "findings": [],
            "operations": [
                the_reported_operation(0, "op-1"),
                the_reported_operation(1, "op-2"),
            ],
            "notes": [],
            "outcome": { "applied": 2, "total": 2, "stopped_early": false },
            "refusal": null,
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restructure_apply_refuses_a_stale_operation_by_id() {
    // Given a warm index that applies the first operation, then refuses the second as stale
    let worktree = a_session_worktree();
    let fake = a_fake_index()
        .applying_with(vec![an_applied_operation(0, "op-1")])
        .refusing_the_apply_with(&the_stale_refusal_of(THE_STALE_OP))
        .answering_plan_status_with(a_plan_status_holding_stale(THE_STALE_OP, THE_STALE_REASON));
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent applies the plan
    let answer = the_agent_calls(
        &executor,
        "restructure_apply",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then after the refused apply the index was asked which of the plan's operations are stale
    assert_eq!(
        fake.what_it_was_asked(),
        vec![
            AskedOfTheIndex::Apply(index::ApplyRequest {
                workspace_root: worktree.root_string(),
                plan: THE_PLAN.to_string(),
                dry_run: false,
                resume: false,
                from: None,
                stop_after: None,
            }),
            AskedOfTheIndex::PlanStatus(index::PlanStatusRequest {
                workspace_root: worktree.root_string(),
                plan: THE_PLAN.to_string(),
            }),
        ]
    );
    // And the agent gets the operation that was applied, no outcome, and a refusal naming the
    // stale operation by its id
    assert_eq!(
        answer,
        Ok(json!({
            "plan": THE_PLAN,
            "findings": [],
            "operations": [the_reported_operation(0, "op-1")],
            "notes": [],
            "outcome": null,
            "refusal": {
                "class": "failed_precondition",
                "message": the_stale_refusal_of(THE_STALE_OP),
                "stale": [{ "op": THE_STALE_OP, "reason": THE_STALE_REASON }],
            },
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restructure_load_asks_the_index_for_worktree_relative_plans_and_returns_the_held_plans() {
    // Given a session worktree, and a warm index that holds the plan once loaded, one operation stale
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_plans_with(index::PlansResponse {
        plans: vec![a_loaded_plan_holding_stale(THE_PLAN, 2, THE_STALE_OP)],
    });
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);
    let absolute_plan = worktree.root().join(THE_PLAN).display().to_string();

    // When the agent loads the plan by its absolute path inside the worktree
    let answer = the_agent_calls(
        &executor,
        "restructure_load",
        worktree.root(),
        json!({ "plans": [absolute_plan] }),
    )
    .await;

    // Then the index was asked to load it by its worktree-relative path, rooted at the worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::LoadPlans(index::LoadPlansRequest {
            workspace_root: worktree.root_string(),
            plans: vec![THE_PLAN.to_string()],
        })]
    );
    // And the agent gets the plans the index holds, with their stale operations
    assert_eq!(
        answer,
        Ok(json!({ "plans": [the_reported_plan_holding_stale(THE_PLAN, 2, THE_STALE_OP)] }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restructure_plans_asks_the_index_about_the_worktree_and_returns_the_held_plans() {
    // Given a session worktree, and a warm index holding two plans
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_plans_with(index::PlansResponse {
        plans: vec![
            a_loaded_plan_holding_stale(THE_PLAN, 2, "op-1"),
            a_loaded_plan_holding_stale("plans/other.jsonl", 5, "op-4"),
        ],
    });
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent lists the plans
    let answer = the_agent_calls(&executor, "restructure_plans", worktree.root(), json!({})).await;

    // Then the index was asked about the session's worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::ListPlans(index::ListPlansRequest {
            workspace_root: worktree.root_string(),
        })]
    );
    // And the agent gets every held plan
    assert_eq!(
        answer,
        Ok(json!({ "plans": [
            the_reported_plan_holding_stale(THE_PLAN, 2, "op-1"),
            the_reported_plan_holding_stale("plans/other.jsonl", 5, "op-4"),
        ] }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restructure_status_asks_the_index_for_the_worktree_relative_plan_and_returns_its_counts() {
    // Given a session worktree, and a warm index whose plan has one operation done, one stale
    let worktree = a_session_worktree();
    let fake = a_fake_index()
        .answering_plan_status_with(a_plan_status_holding_stale(THE_STALE_OP, THE_STALE_REASON));
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent asks for the plan's status by its worktree-relative path
    let answer = the_agent_calls(
        &executor,
        "restructure_status",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then the index was asked about that plan, rooted at the session's worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::PlanStatus(index::PlanStatusRequest {
            workspace_root: worktree.root_string(),
            plan: THE_PLAN.to_string(),
        })]
    );
    // And the agent gets the counts and the stale operations by id
    assert_eq!(
        answer,
        Ok(json!({
            "completed": 1,
            "in_flight": 0,
            "pending": 1,
            "failed": 0,
            "stale": [{ "op": THE_STALE_OP, "reason": THE_STALE_REASON }],
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restructure_anchors_asks_the_index_for_the_named_items_and_returns_their_anchor() {
    // Given a session worktree, and a warm index that anchors `Circle` at a known range
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_anchors_with(index::AnchorsResponse {
        range: Some(a_range((3, 1), (9, 2))),
        anchor_json: r#""items":["Circle"]"#.to_string(),
    });
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent anchors `Circle` in a file named by its worktree-relative path
    let answer = the_agent_calls(
        &executor,
        "restructure_anchors",
        worktree.root(),
        json!({ "file": "src/lib.rs", "items": ["Circle"] }),
    )
    .await;

    // Then the index was asked for those items of that file, rooted at the session's worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Anchors(index::AnchorsRequest {
            workspace_root: worktree.root_string(),
            file: "src/lib.rs".to_string(),
            items: vec!["Circle".to_string()],
            at: None,
        })]
    );
    // And the agent gets the range in the index's coordinates and the anchor fragment
    assert_eq!(
        answer,
        Ok(json!({
            "range": {
                "start": { "line": 3, "column": 1 },
                "end": { "line": 9, "column": 2 },
            },
            "anchor_json": r#""items":["Circle"]"#,
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_stale_lookup_is_reported_beside_the_refusal_and_keeps_the_applied_operations() {
    // Given a warm index that applies the first operation, refuses the second as stale, and then
    // cannot say which operations are stale
    let worktree = a_session_worktree();
    let fake = a_fake_index()
        .applying_with(vec![an_applied_operation(0, "op-1")])
        .refusing_the_apply_with(&the_stale_refusal_of(THE_STALE_OP))
        .failing_plan_status_with(tddy_rpc::Status::internal("the plan store is unreadable"));
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent applies the plan
    let answer = the_agent_calls(
        &executor,
        "restructure_apply",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then the agent still gets an answer: the applied operation, no outcome, and the refusal with
    // no stale operations and the lookup's own message
    assert_eq!(
        answer,
        Ok(json!({
            "plan": THE_PLAN,
            "findings": [],
            "operations": [the_reported_operation(0, "op-1")],
            "notes": [],
            "outcome": null,
            "refusal": {
                "class": "failed_precondition",
                "message": the_stale_refusal_of(THE_STALE_OP),
                "stale": [],
                "stale_error": "the plan store is unreadable",
            },
        }))
    );
}

/// The apply of [`THE_PLAN`] after which the index refuses the run as invalid, and — were it asked —
/// would name a stale operation.
fn an_apply_refused_as_invalid() -> AFakeIndex {
    a_fake_index()
        .applying_with(vec![an_applied_operation(0, "op-1")])
        .refusing_the_apply_as(tddy_rpc::Status::invalid_argument("op-2 names no item"))
        .answering_plan_status_with(a_plan_status_holding_stale(THE_STALE_OP, THE_STALE_REASON))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_that_is_not_a_stale_operation_names_its_class() {
    // Given a warm index that applies the first operation, then refuses the run as invalid
    let worktree = a_session_worktree();
    let fake = an_apply_refused_as_invalid();
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent applies the plan
    let answer = the_agent_calls(
        &executor,
        "restructure_apply",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then the agent gets the applied operation and a refusal of that class, with no stale
    // operations and no lookup error
    assert_eq!(
        answer,
        Ok(json!({
            "plan": THE_PLAN,
            "findings": [],
            "operations": [the_reported_operation(0, "op-1")],
            "notes": [],
            "outcome": null,
            "refusal": {
                "class": "invalid_argument",
                "message": "op-2 names no item",
                "stale": [],
            },
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_that_is_not_a_stale_operation_does_not_ask_for_stale_ones() {
    // Given a warm index that applies the first operation, then refuses the run as invalid
    let worktree = a_session_worktree();
    let fake = an_apply_refused_as_invalid();
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent applies the plan
    let _ = the_agent_calls(
        &executor,
        "restructure_apply",
        worktree.root(),
        json!({ "plan": THE_PLAN }),
    )
    .await;

    // Then the index was only asked to apply, never for the plan's status
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Apply(index::ApplyRequest {
            workspace_root: worktree.root_string(),
            plan: THE_PLAN.to_string(),
            dry_run: false,
            resume: false,
            from: None,
            stop_after: None,
        })]
    );
}

/// The agent's `tool` call naming — through `args_naming` — a path inside a neighbouring session's
/// worktree (`in_the_neighbour`, relative to it), and what the host and the index made of it.
async fn refusing_a_path_outside(
    tool: &str,
    in_the_neighbour: &str,
    args_naming: impl FnOnce(&str) -> Value,
) {
    // Given a session worktree, a neighbouring session's worktree, and a warm index
    let worktree = a_session_worktree();
    let neighbour = a_session_worktree();
    let fake = a_fake_index();
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);
    let neighbours_path = neighbour
        .root()
        .join(in_the_neighbour)
        .display()
        .to_string();

    // When the agent's call names the neighbour's path
    let answer = the_agent_calls(
        &executor,
        tool,
        worktree.root(),
        args_naming(&neighbours_path),
    )
    .await;

    // Then the host refuses it, naming the path
    assert_eq!(
        answer,
        Err(format!(
            "{neighbours_path} is outside the session's worktree"
        ))
    );
    // And the index never heard of it
    assert_eq!(fake.what_it_was_asked(), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_path_outside_the_session_worktree_is_refused() {
    refusing_a_path_outside(
        "restructure_apply",
        THE_PLAN,
        |plan| json!({ "plan": plan }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_outside_the_session_worktree_is_refused_before_the_index_hears_of_it() {
    refusing_a_path_outside(
        "restructure_anchors",
        "src/lib.rs",
        |file| json!({ "file": file, "items": ["Circle"] }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_outside_the_session_worktree_is_refused_when_loading_it() {
    refusing_a_path_outside(
        "restructure_load",
        THE_PLAN,
        |plan| json!({ "plans": [THE_PLAN, plan] }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_plan_argument_is_refused_before_the_index_hears_of_it() {
    // Given a session worktree and a warm index
    let worktree = a_session_worktree();
    let fake = a_fake_index();
    let executor = the_registered_executor(an_index_serving(fake.clone()).await);

    // When the agent checks without naming a plan
    let answer = the_agent_calls(&executor, "restructure_check", worktree.root(), json!({})).await;

    // Then the host refuses the call, naming the missing argument
    assert_eq!(answer, Err("plan is required".to_string()));
    // And the index never heard of it
    assert_eq!(fake.what_it_was_asked(), vec![]);
}
