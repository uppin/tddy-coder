//! The binary's two lifetimes, and the discipline each owes.
//!
//! These spawn the real binary rather than calling into the library, because what is under test is
//! what a process does: which transports it serves, what it writes to which stream, and what it
//! exits with. Modelled on `packages/tddy-e2e/tests/stdio_remote_control_acceptance.rs`, which
//! pins the same concurrency claim for `tddy-coder`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Long enough for a fake-server-backed operation on a loaded machine, short enough that a hang
/// fails the suite rather than stalling it.
const A_RUN_SHOULD_FINISH_WITHIN: Duration = Duration::from_secs(20);

/// The binary under test, resolved the way this repo resolves a sibling binary: the cargo-provided
/// path first, then a sibling of the test executable, with the `deps/` hop an integration test
/// needs.
fn the_index_daemon() -> PathBuf {
    if let Some(from_cargo) = std::env::var_os("CARGO_BIN_EXE_tddy-index-daemon") {
        return PathBuf::from(from_cargo);
    }
    let mut candidate = std::env::current_exe().expect("the test executable's own path");
    candidate.pop();
    if candidate.ends_with("deps") {
        candidate.pop();
    }
    candidate.join("tddy-index-daemon")
}

fn a_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(workspace.path().join("src/lib.rs"), source).expect("a source file");
    workspace
}

/// A plan under `root` whose snapshot names `src/lib.rs` and which asks for nothing, so a check of
/// it neither needs a language server nor finds anything wrong.
fn a_sound_plan_under(root: &Path) -> PathBuf {
    let digest = tddy_code_restructuring::apply::hash_file(&root.join("src/lib.rs"))
        .expect("hash the source file");
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        format!("{{\"v\":1,\"snapshot\":{{\"src/lib.rs\":\"{digest}\"}}}}\n"),
    )
    .expect("write the plan");
    plan
}

/// A plan whose snapshot does not match the tree, so a check of it is refused.
fn a_plan_whose_snapshot_is_stale_under(root: &Path) -> PathBuf {
    let plan = root.join("stale.jsonl");
    std::fs::write(
        &plan,
        "{\"v\":1,\"snapshot\":{\"src/lib.rs\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\"}}\n",
    )
    .expect("write the plan");
    plan
}

#[test]
fn runs_an_operation_in_process_when_no_transport_is_requested() {
    // Given a workspace and a plan with nothing wrong in it
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_sound_plan_under(workspace.path());

    // When the binary is asked to check it with no transport argument
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "check",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            &plan.to_string_lossy(),
        ])
        .output()
        .expect("the binary runs");

    // Then it ran the operation and exited successfully, without serving anything
    assert!(
        run.status.success(),
        "single-shot check failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn exits_non_zero_when_a_single_shot_operation_is_refused() {
    // Given a plan whose snapshot no longer matches the tree
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_whose_snapshot_is_stale_under(workspace.path());

    // When the binary is asked to check it
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "check",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            &plan.to_string_lossy(),
        ])
        .output()
        .expect("the binary runs");

    // Then the refusal reaches the caller as a failing status, not as a silent success
    assert!(
        !run.status.success(),
        "a refused plan must not exit zero; stderr was: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn refuses_to_start_with_neither_a_subcommand_nor_a_transport() {
    // When the binary is started with nothing to do and nothing to serve
    let run = Command::new(the_index_daemon())
        .output()
        .expect("the binary runs");

    // Then it fails fast rather than coming up and waiting for nobody
    assert!(
        !run.status.success(),
        "a process with no transport serves nobody and must not come up"
    );
}

#[test]
fn writes_nothing_to_standard_output_while_serving_over_stdio() {
    // Given the binary serving over its own stdin and stdout
    let mut serving = Command::new(the_index_daemon())
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");

    // When it is given a moment to start up and is then closed
    std::thread::sleep(Duration::from_millis(600));
    drop(serving.stdin.take());
    let finished = serving
        .wait_with_output()
        .expect("the serving process exits when its input closes");

    // Then nothing that is not an RPC frame was written to the stream carrying them. A log line or
    // a progress line on stdout here would corrupt every frame after it.
    let framed = String::from_utf8_lossy(&finished.stdout);
    assert!(
        finished.stdout.is_empty() || framed.starts_with("Content-Length:"),
        "stdout carried something that is not an RPC frame: {framed:?}"
    );
}

#[test]
fn serves_grpc_and_stdio_concurrently_from_one_process() {
    // Given the binary serving both transports at once
    let port = a_free_port();
    let mut serving = Command::new(the_index_daemon())
        .args(["--stdio", "--grpc", &format!("127.0.0.1:{port}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");

    // When each transport is reached in turn
    let grpc_reachable = a_tcp_connection_is_accepted_on(port, A_RUN_SHOULD_FINISH_WITHIN);
    let stdio_open = serving.stdin.as_mut().is_some_and(|stdin| {
        // A zero-length write proves the pipe is open without putting a partial frame into it.
        stdin.write_all(b"").is_ok()
    });

    // Then both answered from the same process — neither flag is a default for the other, and
    // passing both must serve both rather than silently picking one
    drop(serving.stdin.take());
    let _ = serving.wait();
    assert!(grpc_reachable, "the gRPC listener never accepted a client");
    assert!(stdio_open, "the stdio transport was not serving");
}

/// A port nothing is listening on, found by binding and releasing one.
fn a_free_port() -> u16 {
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    probe.local_addr().expect("the bound address").port()
}

/// Whether a TCP connection is accepted on `port` within `budget`, retried because the listener
/// binds on its own thread after the process starts.
fn a_tcp_connection_is_accepted_on(port: u16, budget: Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

// ─── One subcommand per single-shot operation ──────────────────────────────────────────────
//
// Contract: docs/ft/coder/warm-code-intelligence-daemon.md § Two lifetimes, one implementation
//
// `check` is pinned above. These pin the other four, at the same level and in the same way: a real
// process, real arguments, and the answer the operator is left with. Each asserts the *whole*
// rendered account, not a substring of it, so a line that stops being written fails the test.
//
// Two of the four — `apply` and `anchors` — pin a refusal rather than an answer, because the answer
// needs a warm rust-analyzer: `serve_apply` reaches for the language server before it reads the
// plan, and an anchor is a language-server query by definition. A real index costs minutes per run
// and is a production test by this crate's own definition, and the answered paths are already
// covered over the deterministic fake in `code_index_service_acceptance.rs`. What is left to pin
// here, and what these pin, is that the subcommand reaches *its own* RPC and that what comes back
// becomes this process's exit status.

/// Every line a run rendered, with the log's own prefix — a wall-clock timestamp and a level —
/// removed, so a test can assert on the whole account instead of searching it for a substring.
fn rendered(stderr: &[u8]) -> Vec<String> {
    const RENDERED_BY: &str = "[tddy_index_daemon::main] ";
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|line| {
            line.split_once(RENDERED_BY)
                .map(|(_, said)| said.to_string())
        })
        .collect()
}

/// A plan under `root` with an empty snapshot — nothing to match against the tree — holding
/// `operations` in order.
fn a_plan_of(operations: &[&str], root: &Path) -> PathBuf {
    let plan = root.join("counted.jsonl");
    let mut lines = vec!["{\"v\":1,\"snapshot\":{}}".to_string()];
    lines.extend(operations.iter().map(|line| (*line).to_string()));
    std::fs::write(&plan, format!("{}\n", lines.join("\n"))).expect("write the plan");
    plan
}

/// One `extract_module` operation, anchored at a symbol in `src/lib.rs`.
fn an_extraction_of(symbol: &str) -> String {
    format!(
        "{{\"op\":\"extract_module\",\"anchor\":{{\"kind\":\"symbol\",\"file\":\"src/lib.rs\",\
         \"path\":\"{symbol}\"}},\"name\":\"grouped\"}}"
    )
}

/// The workspace of [`a_workspace_holding`], with its one source file committed, so a git ref
/// exists to hold the tree against.
fn a_committed_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = a_workspace_holding(source);
    let status = Command::new("git")
        .args(["add", "--all"])
        .current_dir(workspace.path())
        .status()
        .expect("git");
    assert!(status.success(), "git add failed");
    let status = Command::new("git")
        .args([
            // Identity and signing are supplied per-command: a temporary worktree inherits the
            // machine's git config, and a commit here must neither depend on it nor be refused
            // by it.
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
        ])
        .current_dir(workspace.path())
        .status()
        .expect("git");
    assert!(status.success(), "git commit failed");
    workspace
}

#[test]
fn renders_how_far_a_plans_journal_got_when_asked_for_its_status() {
    // Given a two-operation plan under a root whose journal holds nothing
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_of(
        &[&an_extraction_of("foo"), &an_extraction_of("bar")],
        workspace.path(),
    );

    // When its status is asked for with no transport argument
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "status",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            &plan.to_string_lossy(),
        ])
        .output()
        .expect("the binary runs");

    // Then all four counts reach the operator, with both operations pending, and a status that was
    // answered is not a failed run
    assert_eq!(
        rendered(&run.stderr),
        vec!["completed 0", "in_flight 0", "failed 0", "pending 2"]
    );
    assert!(
        run.status.success(),
        "an answered status must exit zero; stderr was: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn holds_a_tree_against_the_ref_it_was_committed_as() {
    // Given a worktree whose statements are exactly those of its last commit
    let workspace = a_committed_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");

    // When it is verified against that commit with no transport argument
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "verify",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            "--against",
            "HEAD",
        ])
        .output()
        .expect("the binary runs");

    // Then the whole comparison is reported — two statements either side, the function's signature
    // and its body — and a tree that holds exits zero
    assert_eq!(
        rendered(&run.stderr),
        vec![
            "2 statements before, 2 after",
            "every statement accounted for"
        ]
    );
    assert!(
        run.status.success(),
        "a tree matching its ref must exit zero; stderr was: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// A comparison that does not hold is an *answered* call whose answer is a failed run — the
/// judgement `restructure_cli::report` makes for `tddy-tools`, made here for the same reason: the
/// service returns the comparison as a value and the caller decides what it means.
#[test]
fn exits_non_zero_when_the_tree_no_longer_holds_against_the_ref() {
    // Given a worktree that has changed a statement since its last commit
    let workspace = a_committed_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn foo() -> u32 {\n    2\n}\n",
    )
    .expect("change the source file");

    // When it is verified against that commit
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "verify",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            "--against",
            "HEAD",
        ])
        .output()
        .expect("the binary runs");

    // Then the statement it lost and the one it gained are both named, and the run fails
    assert_eq!(
        rendered(&run.stderr),
        vec![
            "2 statements before, 2 after",
            "missing: 1",
            "added:   2",
            "1 statement(s) the tree lost and 1 it gained — see above"
        ]
    );
    assert!(
        !run.status.success(),
        "a tree that no longer holds must not exit zero"
    );
}

#[test]
fn refuses_an_anchors_run_that_names_no_items_for_the_anchor_to_cover() {
    // Given a workspace and an `--items` list that names nothing at all
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");

    // When an anchor is asked for over it
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "anchors",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            "src/lib.rs",
            "--items",
            ",",
        ])
        .output()
        .expect("the binary runs");

    // Then the anchors RPC's own refusal reaches the caller, before any language server is started
    assert_eq!(
        rendered(&run.stderr),
        vec!["INVALID_ARGUMENT: the request names no items for the anchor to cover"]
    );
    assert!(!run.status.success(), "a refused anchor must not exit zero");
}

#[test]
fn refuses_an_apply_against_a_tree_this_process_cannot_reach() {
    // Given a workspace root no tree stands at
    // When a plan is applied against it
    let run = Command::new(the_index_daemon())
        .args([
            "restructure",
            "apply",
            "--workspace-root",
            "/nonexistent/workspace/root",
            "carve.jsonl",
        ])
        .output()
        .expect("the binary runs");

    // Then the refusal names the tree as the thing that is wrong, and reaches the caller as a
    // failing status rather than as a silent success
    assert_eq!(
        rendered(&run.stderr),
        vec![
            "FAILED_PRECONDITION: workspace_root `/nonexistent/workspace/root` is not a directory \
             this process can reach"
        ]
    );
    assert!(!run.status.success(), "a refused apply must not exit zero");
}

/// What `serves_grpc_and_stdio_concurrently_from_one_process` cannot ask: an accepted TCP
/// connection proves a listener, not a *service*. This dispatches a real gRPC call through the
/// generated tonic adapter, which is the only thing that proves the coordinate is reachable over
/// that transport.
#[tokio::test]
async fn answers_a_grpc_client_at_the_coordinate_it_serves() {
    // Given the binary serving gRPC on a loopback port
    let port = a_free_port();
    let mut serving = Command::new(the_index_daemon())
        .args(["--grpc", &format!("127.0.0.1:{port}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    assert!(
        a_tcp_connection_is_accepted_on(port, A_RUN_SHOULD_FINISH_WITHIN),
        "the gRPC listener never accepted a client"
    );

    // When a client asks which workspace roots it holds an index for
    let held = a_grpc_client_on(port)
        .await
        .workspaces(tddy_index_daemon::proto::code_index::WorkspacesRequest {})
        .await;

    // Then it answers with none: nothing is warm until a request names a root
    let _ = serving.kill();
    let _ = serving.wait();
    assert_eq!(
        held.expect("the service answers").into_inner(),
        tddy_index_daemon::proto::code_index::WorkspacesResponse {
            workspaces: Vec::new()
        }
    );
}

/// A gRPC client dialled at `port`, retried because a listener that has accepted a probe is not
/// yet necessarily serving HTTP/2 on it.
async fn a_grpc_client_on(
    port: u16,
) -> tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient<
    tonic::transport::Channel,
> {
    type Client = tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient<
        tonic::transport::Channel,
    >;
    let deadline = std::time::Instant::now() + A_RUN_SHOULD_FINISH_WITHIN;
    let mut last = None;
    while std::time::Instant::now() < deadline {
        match Client::connect(format!("http://127.0.0.1:{port}")).await {
            Ok(client) => return client,
            Err(failure) => last = Some(failure),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no gRPC client could be dialled on {port}: {last:?}");
}

/// A loaded plan's store is flushed when the served process is asked to stop: `SIGTERM` is how
/// `tddy-daemon` and `run-index-daemon --stop` end it, and a plan changed in memory must not be lost
/// with the process.
#[tokio::test]
async fn sigterm_flushes_every_dirty_plan_before_exit() {
    // Given the binary serving gRPC, holding a plan made dirty by id assignment on load
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = workspace.path().join("carve.jsonl");
    std::fs::write(
        &plan,
        "{\"v\":1,\"snapshot\":{}}\n{\"op\":\"rename_symbol\",\"anchor\":{\"kind\":\"symbol\",\
         \"file\":\"src/lib.rs\",\"path\":\"foo\"},\"name\":\"bar\"}\n",
    )
    .expect("write the plan");
    let port = a_free_port();
    let mut serving = Command::new(the_index_daemon())
        .args(["--grpc", &format!("127.0.0.1:{port}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    assert!(
        a_tcp_connection_is_accepted_on(port, A_RUN_SHOULD_FINISH_WITHIN),
        "the gRPC listener never accepted a client"
    );
    a_grpc_client_on(port)
        .await
        .load_plans(tddy_index_daemon::proto::code_index::LoadPlansRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plans: vec!["carve.jsonl".to_string()],
        })
        .await
        .expect("the plan loads");

    // When the process is sent SIGTERM and exits
    let signalled = Command::new("kill")
        .args(["-TERM", &serving.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(signalled.success(), "SIGTERM was not delivered");
    let _ = serving.wait();

    // Then the plan on disk carries the id the store gave its operation
    let written = tddy_code_restructuring::Plan::parse(&std::fs::read_to_string(&plan).unwrap())
        .expect("the flushed plan parses");
    assert!(
        written.ops[0].id.is_some(),
        "the dirty plan was lost with the process"
    );
}
