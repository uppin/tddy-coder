//! Whether a client can reach this service, and what it does when one does.
//!
//! Every test dispatches at the *registered coordinate* —
//! `entry.service.handle_rpc("code_index.CodeIndexService", "<Method>", …)` with an encoded
//! request, decoding the answer back — rather than calling the handler behind it. Seven methods
//! that compile prove nothing about whether a client can reach any of them, which is the failure
//! `tddy-terminal-rpc`'s own suite exists to prevent.
//!
//! The warm index is a `fake_lsp` server, not a real rust-analyzer: a real one costs minutes per
//! run and is a production test by this repo's own definition.

use std::path::Path;
use std::time::Duration;

use prost::Message;
use tddy_index_daemon::proto::code_index::{
    restructure_event, AnalyzeEvent, AnchorsRequest, AnchorsResponse, ApplyRequest, CheckRequest,
    CodeIndexServiceServer, ComplexityRequest, ComplexityResponse, CoverageRequest,
    DuplicateTestsRequest, Finding, FunctionComplexity, IndexProgress, ListPlansRequest,
    LoadPlansRequest, LoadedPlan, PlanStatusRequest, PlanStatusResponse, PlansResponse,
    ReportRequest, ReportResponse, RestructureEvent, RunOutcome, SourcePosition, SourceRange,
    UnloadPlansRequest, VerifyRequest, VerifyResponse, WarmRequest, WorkspacesRequest,
    WorkspacesResponse,
};
use tddy_index_daemon::{
    build_code_index_entry, CodeIndexPorts, CodeIndexServiceImpl, CODE_INDEX_SERVICE,
};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

/// The concrete server type whose `NAME` a client actually reaches.
type ServedCoordinate = CodeIndexServiceServer<CodeIndexServiceImpl>;

/// A host whose warm servers are the deterministic fake, launched with this crate's own handshake.
fn a_host_over_fake_language_servers() -> tddy_rpc::ServiceEntry {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(tddy_code_restructuring::client_capabilities())
            .with_initialization_options(tddy_code_restructuring::server_settings()),
    );
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60)),
    })
}

/// A host whose warm servers narrate a crate-graph load and then report themselves quiescent, the
/// way a real rust-analyzer does.
///
/// Separate from [`a_host_over_fake_language_servers`] because the default fake answers everything
/// at once and never claims to load anything — which is what the other forty-odd tests here want,
/// and the one thing a test about *waiting for a graph* cannot use.
fn a_host_over_fake_language_servers_that_load_a_graph() -> tddy_rpc::ServiceEntry {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
        .with_capabilities(tddy_code_restructuring::client_capabilities())
        .with_initialization_options(tddy_code_restructuring::server_settings());
    spec.args = vec!["--loads-crate-graph".to_string()];
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60)),
    })
}

/// A git worktree holding one source file, at a path that is not the process directory.
fn a_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git init");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(workspace.path().join("src/lib.rs"), source).expect("a source file");
    workspace
}

/// Dispatch one unary request at the registered coordinate and decode its answer.
async fn unary_at<Req: Message, Res: Message + Default>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Res, tddy_rpc::Status> {
    match entry
        .service
        .handle_rpc(entry.name, method, &a_message_carrying(request))
        .await
    {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(Res::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from {method}"),
    }
}

/// An incoming request message with no transport metadata — nothing in this service dispatches on
/// the sender, so a test that invented an identity would be asserting on fiction.
fn a_message_carrying<Req: Message>(request: Req) -> tddy_rpc::RpcMessage {
    tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    )
}

/// Dispatch a server-streaming request at the registered coordinate and drain it.
async fn stream_at<Req: Message, Item: Message + Default>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Vec<Item>, tddy_rpc::Status> {
    match entry
        .service
        .handle_rpc(entry.name, method, &a_message_carrying(request))
        .await
    {
        tddy_rpc::RpcResult::ServerStream(Ok(mut receiver)) => {
            let mut items = Vec::new();
            while let Some(next) = receiver.recv().await {
                items.push(Item::decode(next?.as_slice()).expect("a stream item decodes"));
            }
            Ok(items)
        }
        tddy_rpc::RpcResult::ServerStream(Err(status)) => Err(status),
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::Unary(Ok(_)) => panic!("expected a stream from {method}"),
    }
}

#[test]
fn names_the_service_the_wiring_layer_registers() {
    // Given the entry a host registers
    let entry = a_host_over_fake_language_servers();

    // Then it is named for the coordinate a client addresses
    assert_eq!(entry.name, "code_index.CodeIndexService");
}

#[test]
fn registers_at_the_coordinate_its_generated_server_answers_to() {
    // Given the entry a host registers
    let entry = a_host_over_fake_language_servers();

    // Then the name it is registered under is the one its own server answers to — a registration
    // that drifted from the server's `NAME` would make every method unreachable
    assert_eq!(entry.name, ServedCoordinate::NAME);
}

#[test]
fn publishes_the_coordinate_its_schema_declares() {
    // Given this crate's proto as it sits on disk
    let schema = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/code_index.proto"),
    )
    .expect("the proto this crate owns");
    let package = declared(&schema, "package ").expect("a package declaration");
    let service = declared(&schema, "service ").expect("a service declaration");

    // Then the published coordinate is the one the schema declares
    assert_eq!(format!("{package}.{service}"), CODE_INDEX_SERVICE);
}

/// The first identifier following `keyword` at the start of a line, stripped of its terminator.
fn declared(schema: &str, keyword: &str) -> Option<String> {
    schema
        .lines()
        .find_map(|line| line.trim().strip_prefix(keyword))
        .map(|rest| {
            rest.trim()
                .trim_end_matches(&[';', '{'][..])
                .trim()
                .to_string()
        })
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_request_at_a_neighbouring_coordinate() {
    // Given the entry a host registers
    let entry = a_host_over_fake_language_servers();
    let message = a_message_carrying(WorkspacesRequest {});

    // When a request arrives addressed to a service this entry does not serve
    let answer = entry
        .service
        .handle_rpc("code_index.SomeOtherService", "Workspaces", &message)
        .await;

    // Then it is refused as unknown rather than answered by the wrong service
    let tddy_rpc::RpcResult::Unary(Err(status)) = answer else {
        panic!("a foreign coordinate must be refused");
    };
    assert_eq!(status.code(), tddy_rpc::Code::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_unknown_method_on_its_own_coordinate() {
    // Given the entry a host registers
    let entry = a_host_over_fake_language_servers();
    let message = a_message_carrying(WorkspacesRequest {});

    // When a method this service does not offer is asked for
    let answer = entry
        .service
        .handle_rpc(entry.name, "Reindex", &message)
        .await;

    // Then it is refused rather than silently ignored
    let tddy_rpc::RpcResult::Unary(Err(status)) = answer else {
        panic!("an unknown method must be refused");
    };
    assert_eq!(status.code(), tddy_rpc::Code::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn warms_a_workspace_root_and_reports_it_ready() {
    // Given a workspace root and a host holding no index for it
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let entry = a_host_over_fake_language_servers_that_load_a_graph();

    // When that root is warmed
    let progress: Vec<IndexProgress> = stream_at(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
        },
    )
    .await
    .expect("warming a reachable root succeeds");

    // Then the stream ends by saying the index is ready
    let last = progress.last().expect("at least one progress message");
    assert!(
        last.ready,
        "a completed warm must end by reporting the index ready"
    );
}

/// What `ready` is *for*: a client that reads it as "ask me anything now" and then pays the whole
/// graph load on its next request has been told nothing. A live server holding the root is not a
/// loaded graph, and the server itself is the only thing that knows the difference — so its phases
/// are forwarded and its own quiescence is what ends the wait.
#[tokio::test(flavor = "multi_thread")]
async fn warming_a_root_forwards_the_servers_phases_and_reports_ready_only_once_it_is_quiescent() {
    // Given a workspace root whose server narrates a crate-graph load before it goes quiescent
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let entry = a_host_over_fake_language_servers_that_load_a_graph();

    // When that root is warmed
    let progress: Vec<IndexProgress> = stream_at(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
        },
    )
    .await
    .expect("warming a reachable root succeeds");

    // Then the server's own phases reached the caller, named and with the percentage they carried
    let narrated: Vec<(String, u32)> = progress
        .iter()
        .filter(|message| !message.phase.is_empty())
        .map(|message| (message.phase.clone(), message.percentage))
        .collect();
    assert_eq!(
        narrated,
        vec![
            ("loading crate graph".to_string(), 25),
            ("loading crate graph".to_string(), 50),
            ("loading crate graph".to_string(), 75),
        ],
        "the server's phases did not reach the caller: {progress:?}"
    );
    // and the furthest the load got travels with them, so a wait that ends badly can say where it
    // stopped rather than only that it did
    assert!(
        progress
            .iter()
            .any(|message| message.furthest.contains("75%")),
        "no message said how far the load got: {progress:?}"
    );
    // and exactly one message claims the index is ready: the last, after the server said so
    assert_eq!(
        progress
            .iter()
            .map(|message| message.ready)
            .collect::<Vec<bool>>(),
        {
            let mut expected = vec![false; progress.len() - 1];
            expected.push(true);
            expected
        },
        "a warm claimed the index ready before the server was quiescent: {progress:?}"
    );
}

/// `Warm` is documented idempotent, and a graph that is loaded is loaded: a server reports itself
/// quiescent on the transition and never again, so a second warm that waited to be told would wait
/// for ever. What this process observed once is what answers it.
#[tokio::test(flavor = "multi_thread")]
async fn warming_a_root_whose_graph_is_already_loaded_answers_ready_without_waiting_again() {
    // Given a root this process has already warmed to a loaded graph
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let entry = a_host_over_fake_language_servers_that_load_a_graph();
    let root = workspace.path().to_string_lossy().to_string();
    stream_at::<_, IndexProgress>(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: root.clone(),
        },
    )
    .await
    .expect("the first warm succeeds");

    // When it is warmed again
    let progress: Vec<IndexProgress> = stream_at(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: root,
        },
    )
    .await
    .expect("the second warm succeeds");

    // Then it is answered ready from what this process already observed, rather than waiting for a
    // server to repeat a transition it has already made
    assert!(
        progress.last().expect("at least one message").ready,
        "a second warm of a loaded root did not report it ready: {progress:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn warms_two_workspace_roots_independently() {
    // Given two workspace roots and one host
    let one = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let other = a_workspace_holding("pub fn bar() -> u32 {\n    2\n}\n");
    let entry = a_host_over_fake_language_servers_that_load_a_graph();

    // When only the first is warmed
    stream_at::<_, IndexProgress>(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: one.path().to_string_lossy().to_string(),
        },
    )
    .await
    .expect("warming the first root succeeds");

    // Then the host holds an index for that root alone
    let held: WorkspacesResponse = unary_at(&entry, "Workspaces", WorkspacesRequest {})
        .await
        .expect("the host reports what it holds");
    let roots: Vec<String> = held
        .workspaces
        .iter()
        .map(|workspace| workspace.workspace_root.clone())
        .collect();
    assert_eq!(roots, vec![one.path().to_string_lossy().to_string()]);
    assert!(
        !roots.contains(&other.path().to_string_lossy().to_string()),
        "warming one root must not warm another"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_an_unreachable_workspace_root_as_a_failed_precondition() {
    // Given a path that is not a workspace at all
    let entry = a_host_over_fake_language_servers();

    // When it is warmed
    let outcome = stream_at::<_, IndexProgress>(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: "/nonexistent/workspace/root".to_string(),
        },
    )
    .await;

    // Then the refusal says the tree is wrong, not that the request was
    assert_eq!(
        outcome.expect_err("an unreachable root is refused").code(),
        tddy_rpc::Code::FailedPrecondition
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_plan_that_carries_source_text_as_an_invalid_argument() {
    // Given a plan carrying code text, which the plan vocabulary refuses by design
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = workspace.path().join("plan.jsonl");
    std::fs::write(
        &plan,
        "{\"v\":1,\"snapshot\":{}}\n\
         {\"op\":\"extract_module\",\"anchor\":{\"kind\":\"symbol\",\"file\":\"src/lib.rs\",\
         \"path\":\"foo\"},\"name\":\"grouped\",\"text\":\"pub fn foo() {}\"}\n",
    )
    .expect("write the plan");
    let entry = a_host_over_fake_language_servers();

    // When it is checked
    let outcome = stream_at::<_, RestructureEvent>(
        &entry,
        "Check",
        CheckRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: plan.to_string_lossy().to_string(),
            deep: false,
            file_budget: 0,
        },
    )
    .await;

    // Then the refusal names the plan as the thing that is wrong
    assert_eq!(
        outcome
            .expect_err("a plan carrying code text is refused")
            .code(),
        tddy_rpc::Code::InvalidArgument
    );
}

/// A git worktree whose one source file is committed, so a git ref exists to compare it against.
fn a_committed_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = a_workspace_holding(source);
    git(&workspace, &["add", "--all"]);
    git(
        &workspace,
        &[
            // Identity and signing are supplied per-command: a temporary worktree inherits the
            // machine's git config, and a commit here must not depend on — or be refused by — it.
            "-c",
            "user.name=tddy tests",
            "-c",
            "user.email=tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--message",
            "the tree as the plan was written against it",
        ],
    );
    workspace
}

fn git(workspace: &tempfile::TempDir, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(workspace.path())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

/// A plan file holding a snapshot header and the operation lines given, in order.
fn a_plan_in(workspace: &tempfile::TempDir, operations: &[&str]) -> std::path::PathBuf {
    let plan = workspace.path().join("plan.jsonl");
    let mut lines = vec!["{\"v\":1,\"snapshot\":{}}".to_string()];
    lines.extend(operations.iter().map(|line| (*line).to_string()));
    std::fs::write(&plan, format!("{}\n", lines.join("\n"))).expect("write the plan");
    plan
}

/// One `extract_module` operation, anchored at a symbol in `file`.
fn an_extraction_of(symbol: &str, file: &str) -> String {
    format!(
        "{{\"op\":\"extract_module\",\"anchor\":{{\"kind\":\"symbol\",\"file\":\"{file}\",\
         \"path\":\"{symbol}\"}},\"name\":\"grouped\"}}"
    )
}

/// One `extract_module` operation over a run of whole lines in `file`, naming the module it would
/// create. A range anchor rather than a symbol one because the checks a plan can be judged against
/// without a language server read the text of the seam itself.
fn an_extraction_of_lines(lines: std::ops::RangeInclusive<u32>, file: &str, name: &str) -> String {
    format!(
        "{{\"op\":\"extract_module\",\"anchor\":{{\"kind\":\"range\",\"file\":\"{file}\",\
         \"start\":{{\"line\":{},\"col\":1}},\"end\":{{\"line\":{},\"col\":2}}}},\
         \"name\":\"{name}\"}}",
        lines.start(),
        lines.end()
    )
}

/// The findings a stream carried, in the order it carried them.
fn findings_in(events: &[RestructureEvent]) -> Vec<Finding> {
    events
        .iter()
        .filter_map(|event| match &event.event {
            Some(restructure_event::Event::Finding(finding)) => Some(finding.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn ends_an_apply_by_reporting_what_the_whole_run_amounted_to() {
    // Given a plan whose operations are all behind it, so the run has nothing left to do
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_in(&workspace, &[]);
    let entry = a_host_over_fake_language_servers();

    // When it is applied
    let events: Vec<RestructureEvent> = stream_at(
        &entry,
        "Apply",
        ApplyRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: plan.to_string_lossy().to_string(),
            dry_run: false,
            resume: false,
            from: None,
            stop_after: None,
        },
    )
    .await
    .expect("applying a plan with no operations left succeeds");

    // Then the stream ends with the outcome of the run, not merely with silence
    assert_eq!(
        events,
        vec![RestructureEvent {
            event: Some(restructure_event::Event::Outcome(RunOutcome {
                applied: 0,
                total: 0,
                stopped_early: false,
            })),
        }]
    );
}

/// A two-crate workspace whose `origin` test binary reads a file beside it with `include_str!`.
///
/// Moving the binary to `destination` is an operation the engine accepts and authors without
/// asking the server anything, and it leaves the file behind — so the moved binary no longer
/// compiles, which only a compiler can say.
fn a_workspace_whose_test_binary_reads_a_file_beside_it() -> tempfile::TempDir {
    let workspace = a_workspace_holding("");
    for (path, text) in [
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/origin\", \"crates/destination\"]\n",
        ),
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/origin/src/lib.rs", "pub fn level() -> u32 {\n    2\n}\n"),
        (
            "crates/origin/tests/golden.rs",
            "#[test]\nfn matches_the_golden_output() {\n    \
             assert_eq!(include_str!(\"golden/expected.txt\"), \"2\\n\");\n}\n",
        ),
        ("crates/origin/tests/golden/expected.txt", "2\n"),
        (
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/destination/src/lib.rs", "//! Where the test goes.\n"),
    ] {
        let absolute = workspace.path().join(path);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("a directory");
        std::fs::write(absolute, text).expect("a fixture file");
    }
    git(&workspace, &["add", "--all"]);
    workspace
}

#[tokio::test(flavor = "multi_thread")]
async fn fails_an_apply_that_leaves_a_tree_the_compiler_rejects() {
    // Given a plan whose one operation leaves a test binary that no longer compiles
    let workspace = a_workspace_whose_test_binary_reads_a_file_beside_it();
    let plan = a_plan_in(
        &workspace,
        &[
            "{\"op\":\"move_test_binary_to_crate\",\"anchor\":{\"kind\":\"symbol\",\
           \"file\":\"crates/origin/tests/golden.rs\",\"path\":\"golden\"},\
           \"to\":\"crates/destination\"}",
        ],
    );
    let entry = a_host_over_fake_language_servers();

    // When it is applied through the daemon
    let refusal = stream_at::<_, RestructureEvent>(
        &entry,
        "Apply",
        ApplyRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: plan.to_string_lossy().to_string(),
            dry_run: false,
            resume: false,
            from: None,
            stop_after: None,
        },
    )
    .await
    .expect_err("an apply whose result does not compile is a failed run");

    // Then the run fails saying the applied tree does not compile, rather than reporting an outcome
    assert_eq!(refusal.code(), tddy_rpc::Code::Internal);
    assert!(
        refusal
            .message()
            .starts_with("1 of 1 operation(s) were applied, and the tree no longer compiles"),
        "{}",
        refusal.message()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_apply_whose_plan_names_a_file_no_backend_handles() {
    // Given a plan anchored in a file no language backend claims
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    std::fs::write(workspace.path().join("styles.css"), ".a {}\n").expect("a stylesheet");
    let plan = a_plan_in(&workspace, &[&an_extraction_of("foo", "styles.css")]);
    let entry = a_host_over_fake_language_servers();

    // When it is applied
    let outcome = stream_at::<_, RestructureEvent>(
        &entry,
        "Apply",
        ApplyRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: plan.to_string_lossy().to_string(),
            dry_run: false,
            resume: false,
            from: None,
            stop_after: None,
        },
    )
    .await;

    // Then the refusal names the plan as the thing that is wrong
    assert_eq!(
        outcome
            .expect_err("a plan no backend can carry out is refused")
            .code(),
        tddy_rpc::Code::InvalidArgument
    );
}

/// The range comes from the language server's own outline — the fixture reports `foo` spanning
/// lines 11 to 13 (zero-based 10 to 12) with its closing brace at column 1 — because hand-counting
/// a seam's extent is the busywork the anchor query exists to remove.
#[tokio::test(flavor = "multi_thread")]
async fn emits_the_range_anchor_its_language_server_outlines_for_a_named_item() {
    // Given a workspace whose language server can outline its source
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let entry = a_host_over_fake_language_servers();

    // When the anchor covering one named item is asked for
    let anchor: AnchorsResponse = unary_at(
        &entry,
        "Anchors",
        AnchorsRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            file: "src/lib.rs".to_string(),
            items: vec!["foo".to_string()],
            at: None,
        },
    )
    .await
    .expect("an outlined item has an anchor");

    // Then it covers that item, in the one-based byte coordinates a plan is written in
    assert_eq!(
        anchor.range,
        Some(SourceRange {
            start: Some(SourcePosition {
                line: 11,
                column: 1
            }),
            end: Some(SourcePosition {
                line: 13,
                column: 2
            }),
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_every_operation_as_pending_for_a_plan_that_has_not_run() {
    // Given a two-operation plan and a root whose journal holds nothing
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_in(
        &workspace,
        &[
            &an_extraction_of("foo", "src/lib.rs"),
            &an_extraction_of("bar", "src/lib.rs"),
        ],
    );
    let entry = a_host_over_fake_language_servers();

    // When the plan's progress is asked for
    let progress: PlanStatusResponse = unary_at(
        &entry,
        "PlanStatus",
        PlanStatusRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: plan.to_string_lossy().to_string(),
        },
    )
    .await
    .expect("a plan that has not run still has a status");

    // Then every operation is pending and none is in flight, completed or failed
    assert_eq!(
        progress,
        PlanStatusResponse {
            completed: 0,
            in_flight: 0,
            pending: 2,
            failed: 0,
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn holds_a_tree_against_the_ref_it_was_committed_as() {
    // Given a worktree whose statements are exactly those of its last commit
    let workspace = a_committed_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let entry = a_host_over_fake_language_servers();

    // When it is verified against that commit
    let comparison: VerifyResponse = unary_at(
        &entry,
        "Verify",
        VerifyRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            against: "HEAD".to_string(),
        },
    )
    .await
    .expect("a tree matching its ref verifies");

    // Then the whole comparison comes back, not merely the verdict: two statements either side —
    // the function's signature and its body — with nothing lost and nothing gained. The closing
    // brace is scaffolding a restructure is allowed to move, so it is not one of them.
    assert_eq!(
        comparison,
        VerifyResponse {
            holds: true,
            before: 2,
            after: 2,
            missing: Vec::new(),
            added: Vec::new(),
        }
    );
}

/// A finding is not a refusal. `runner::check` hands its findings back as values, so a plan with
/// findings is a check that *worked*, and this is the stream carrying them — the refusal path, a
/// plan the vocabulary rejects outright, is covered by
/// `refuses_a_plan_that_carries_source_text_as_an_invalid_argument`.
#[tokio::test(flavor = "multi_thread")]
async fn streams_a_finding_attributed_to_the_operation_that_caused_it() {
    // Given a plan whose one extraction would declare a module the file already declares — an
    // `E0428` collision the text alone shows, so no language server is asked about it
    let workspace = a_workspace_holding("mod grouped;\npub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_in(
        &workspace,
        &[&an_extraction_of_lines(2..=4, "src/lib.rs", "grouped")],
    );
    let entry = a_host_over_fake_language_servers();

    // When it is checked
    let events: Vec<RestructureEvent> = stream_at(
        &entry,
        "Check",
        CheckRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: plan.to_string_lossy().to_string(),
            deep: false,
            file_budget: 0,
        },
    )
    .await
    .expect("a plan with findings is a check that worked");

    // Then the stream carries that one finding, attributed to the operation it belongs to
    let findings = findings_in(&events);
    assert_eq!(
        findings.len(),
        1,
        "expected one finding in the stream, got {events:?}"
    );
    assert_eq!(findings[0].operation, 0);
    // The wording is the backend's own, pinned by `tddy-code-restructuring`'s tests; what this
    // suite is about is that it crosses the wire at all rather than reaching a console.
    assert!(
        findings[0].detail.contains("`grouped` is already taken")
            && findings[0].detail.contains("E0428"),
        "the finding did not say what is wrong: {}",
        findings[0].detail
    );
}

// ---------------------------------------------------------------------------------------------
// Analysis
//
// A real coverage capture is 55.8 minutes on this workspace and shells out to cargo and llvm-cov
// (`docs/dev/todo/2026-09-09-coverage-capture-writes-its-denominator-only-at-the-end.md`), and
// duplicate-tests detection is ~22 minutes. Neither is an acceptance test by this repo's own
// definition, so what these pin is that a client can *reach* each analysis RPC at the registered
// coordinate and that a refusal arrives in the class a caller can act on. `Complexity` is the one
// analysis operation cheap enough to answer for real, so it is answered for real.

/// One branching function, so a score of 2 means the source was walked rather than counted.
const A_BRANCHING_FUNCTION: &str = r#"pub fn classify(x: i32) -> &'static str {
    if x < 0 {
        "neg"
    } else {
        "pos"
    }
}
"#;

#[tokio::test(flavor = "multi_thread")]
async fn scores_every_function_in_the_source_file_a_request_names() {
    // Given a workspace holding one branching function
    let workspace = a_workspace_holding(A_BRANCHING_FUNCTION);
    let entry = a_host_over_fake_language_servers();

    // When its complexity is asked for
    let scored: ComplexityResponse = unary_at(
        &entry,
        "Complexity",
        ComplexityRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            file: "src/lib.rs".to_string(),
        },
    )
    .await
    .expect("a source file this process can read scores");

    // Then the function comes back with the score its branches earn, at the line it is declared on
    assert_eq!(
        scored,
        ComplexityResponse {
            functions: vec![FunctionComplexity {
                name: "classify".to_string(),
                line: 1,
                complexity: 2,
            }],
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_score_a_file_that_is_not_parseable_rust() {
    // Given a file whose text no Rust parser accepts
    let workspace = a_workspace_holding("pub fn ((( {\n");
    let entry = a_host_over_fake_language_servers();

    // When its complexity is asked for
    let outcome = unary_at::<_, ComplexityResponse>(
        &entry,
        "Complexity",
        ComplexityRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            file: "src/lib.rs".to_string(),
        },
    )
    .await;

    // Then the refusal names what the request asked about, rather than reporting a broken server
    assert_eq!(
        outcome
            .expect_err("source that does not parse cannot be scored")
            .code(),
        tddy_rpc::Code::InvalidArgument
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_score_a_file_the_tree_does_not_hold() {
    // Given a workspace with no such file in it
    let workspace = a_workspace_holding(A_BRANCHING_FUNCTION);
    let entry = a_host_over_fake_language_servers();

    // When a file that is not there is asked about
    let outcome = unary_at::<_, ComplexityResponse>(
        &entry,
        "Complexity",
        ComplexityRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            file: "src/absent.rs".to_string(),
        },
    )
    .await;

    // Then the tree is named as the thing that is wrong, not the request
    assert_eq!(
        outcome
            .expect_err("a file that is not there is refused")
            .code(),
        tddy_rpc::Code::FailedPrecondition
    );
}

/// The refusal arrives before cargo is invoked — `capture_coverage` resolves the manifest first —
/// so this reaches the capture RPC without paying for a capture.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_coverage_capture_of_a_path_that_holds_no_crate() {
    // Given a tree with no Cargo.toml anywhere in it
    let workspace = a_workspace_holding(A_BRANCHING_FUNCTION);
    let entry = a_host_over_fake_language_servers();

    // When a capture of it is asked for
    let outcome = stream_at::<_, AnalyzeEvent>(
        &entry,
        "Coverage",
        CoverageRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            crate_path: ".".to_string(),
            coverage_dir: "coverage".to_string(),
        },
    )
    .await;

    // Then the refusal names the path the request chose, which is the only thing that can change
    assert_eq!(
        outcome.expect_err("there is nothing to instrument").code(),
        tddy_rpc::Code::InvalidArgument
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_report_before_any_coverage_has_been_captured() {
    // Given a workspace whose coverage directory holds no capture
    let workspace = a_workspace_holding(A_BRANCHING_FUNCTION);
    let entry = a_host_over_fake_language_servers();

    // When a report over it is asked for
    let outcome = unary_at::<_, ReportResponse>(
        &entry,
        "Report",
        ReportRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            coverage_dir: "coverage".to_string(),
            crate_path: ".".to_string(),
        },
    )
    .await;

    // Then the refusal says the state is missing — capture first — rather than blaming the request
    assert_eq!(
        outcome
            .expect_err("a report needs a capture to report on")
            .code(),
        tddy_rpc::Code::FailedPrecondition
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_duplicate_tests_before_any_coverage_has_been_captured() {
    // Given a workspace whose coverage directory holds no per-test artifacts
    let workspace = a_workspace_holding(A_BRANCHING_FUNCTION);
    let entry = a_host_over_fake_language_servers();

    // When duplicate tests are asked for
    let outcome = stream_at::<_, AnalyzeEvent>(
        &entry,
        "DuplicateTests",
        DuplicateTestsRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            coverage_dir: "coverage".to_string(),
            out_dir: "coverage/duplicate-tests".to_string(),
            min_signature: 5,
            subset_ratio: 0.5,
            include_test_sources: false,
        },
    )
    .await;

    // Then the refusal says the state is missing, in the same class a missing capture always is
    assert_eq!(
        outcome
            .expect_err("duplicate detection needs per-test artifacts")
            .code(),
        tddy_rpc::Code::FailedPrecondition
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_a_plan_that_is_not_there_as_a_failed_precondition_naming_the_path() {
    // Given a reachable workspace and a request naming a plan that does not exist
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let entry = a_host_over_fake_language_servers();

    // When it is checked
    let outcome = stream_at::<_, RestructureEvent>(
        &entry,
        "Check",
        CheckRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plan: "absent.jsonl".to_string(),
            deep: false,
            file_budget: 0,
        },
    )
    .await;

    // Then the refusal says the tree is wrong and names what is missing — not `Internal`, which
    // would claim the caller neither caused it nor can fix it
    let refusal = outcome.expect_err("a plan that is not there is refused");
    assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
    assert!(
        refusal.message().contains("absent.jsonl"),
        "the refusal must name the plan it could not find, was: {}",
        refusal.message()
    );
}

/// An apply request for `plan` under `workspace`, run from its start.
fn an_apply_of(workspace: &tempfile::TempDir, plan: &Path) -> ApplyRequest {
    ApplyRequest {
        workspace_root: workspace.path().to_string_lossy().to_string(),
        plan: plan.to_string_lossy().to_string(),
        dry_run: false,
        resume: false,
        from: None,
        stop_after: None,
    }
}

/// A plan file named `name` under `workspace`, holding the header and `operations`.
fn a_plan_named(
    workspace: &tempfile::TempDir,
    name: &str,
    operations: &[&str],
) -> std::path::PathBuf {
    let plan = workspace.path().join(name);
    let mut lines = vec!["{\"v\":1,\"snapshot\":{}}".to_string()];
    lines.extend(operations.iter().map(|line| (*line).to_string()));
    std::fs::write(&plan, format!("{}\n", lines.join("\n"))).expect("write the plan");
    plan
}

fn root_of(workspace: &tempfile::TempDir) -> String {
    workspace.path().to_string_lossy().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_of_an_unloaded_plan_loads_it_and_list_plans_shows_it() {
    // Given a plan nothing has loaded
    let workspace = a_committed_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_named(&workspace, "carve.jsonl", &[]);
    let entry = a_host_over_fake_language_servers();

    // When it is applied, and the root's plans are then listed
    let _: Vec<RestructureEvent> = stream_at(&entry, "Apply", an_apply_of(&workspace, &plan))
        .await
        .expect("the apply runs");
    let listed: PlansResponse = unary_at(
        &entry,
        "ListPlans",
        ListPlansRequest {
            workspace_root: root_of(&workspace),
        },
    )
    .await
    .expect("the root's plans are listed");

    // Then the applied plan is held
    assert_eq!(
        listed,
        PlansResponse {
            plans: vec![LoadedPlan {
                plan: "carve.jsonl".to_string(),
                ops: 0,
                dirty: false,
            }],
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unload_all_flushes_and_drops_every_plan_of_the_root() {
    // Given two loaded plans, each holding an operation loaded without an id
    let workspace = a_committed_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let first = a_plan_named(
        &workspace,
        "first.jsonl",
        &[&an_extraction_of("foo", "src/lib.rs")],
    );
    let second = a_plan_named(
        &workspace,
        "second.jsonl",
        &[&an_extraction_of("foo", "src/lib.rs")],
    );
    let entry = a_host_over_fake_language_servers();
    let _: PlansResponse = unary_at(
        &entry,
        "LoadPlans",
        LoadPlansRequest {
            workspace_root: root_of(&workspace),
            plans: vec!["first.jsonl".to_string(), "second.jsonl".to_string()],
        },
    )
    .await
    .expect("the plans load");

    // When every plan of the root is unloaded
    let remaining: PlansResponse = unary_at(
        &entry,
        "UnloadPlans",
        UnloadPlansRequest {
            workspace_root: root_of(&workspace),
            plans: Vec::new(),
            all: true,
        },
    )
    .await
    .expect("the plans unload");

    // Then none is held, and both files were written back with their ids
    assert_eq!(remaining, PlansResponse { plans: Vec::new() });
    for plan in [first, second] {
        let written =
            tddy_code_restructuring::Plan::parse(&std::fs::read_to_string(&plan).unwrap())
                .expect("the flushed plan parses");
        assert!(
            written.ops[0].id.is_some(),
            "{} was not flushed",
            plan.display()
        );
    }
}

/// The code issue `stale-repo-scoped-restructure-state-apply`: a plan that ran through the daemon
/// left its journal keyed by the *root*, so the next plan under that root was refused with
/// "a journal already exists" although it had never run.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_plan_applies_after_a_first_ran_under_the_same_root() {
    // Given a root where one plan has already run through the daemon and written its journal
    let workspace = a_workspace_whose_test_binary_reads_a_file_beside_it();
    let first = a_plan_named(
        &workspace,
        "first.jsonl",
        &[
            "{\"op\":\"move_test_binary_to_crate\",\"anchor\":{\"kind\":\"symbol\",\
           \"file\":\"crates/origin/tests/golden.rs\",\"path\":\"golden\"},\
           \"to\":\"crates/destination\"}",
        ],
    );
    let second = a_plan_named(&workspace, "second.jsonl", &[]);
    let entry = a_host_over_fake_language_servers();
    let _ =
        stream_at::<_, RestructureEvent>(&entry, "Apply", an_apply_of(&workspace, &first)).await;

    // When a different plan is applied under the same root
    let second_run: Result<Vec<RestructureEvent>, _> =
        stream_at(&entry, "Apply", an_apply_of(&workspace, &second)).await;

    // Then it runs — the first plan's journal is its own, not the root's
    assert_eq!(
        second_run.map_err(|status| status.message().to_string()),
        Ok(vec![RestructureEvent {
            event: Some(restructure_event::Event::Outcome(RunOutcome {
                applied: 0,
                total: 0,
                stopped_early: false,
            })),
        }])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dirty_plan_reaches_disk_within_the_flush_interval() {
    // Given a plan whose one operation carries no id
    let workspace = a_committed_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_named(
        &workspace,
        "carve.jsonl",
        &[&an_extraction_of("foo", "src/lib.rs")],
    );
    let entry = a_host_over_fake_language_servers();

    // When it is loaded — which gives the operation an id and so makes the plan dirty
    let _: PlansResponse = unary_at(
        &entry,
        "LoadPlans",
        LoadPlansRequest {
            workspace_root: root_of(&workspace),
            plans: vec!["carve.jsonl".to_string()],
        },
    )
    .await
    .expect("the plan loads");

    // Then, without any further request, the file carries the id within the flush interval — the
    // flush is eventual by design, so this waits for it rather than for an event nothing sends
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut flushed = false;
    while std::time::Instant::now() < deadline && !flushed {
        flushed = tddy_code_restructuring::Plan::parse(&std::fs::read_to_string(&plan).unwrap())
            .map(|written| written.ops[0].id.is_some())
            .unwrap_or(false);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(flushed, "the loaded plan was never written back");
}
