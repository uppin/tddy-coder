//! What this process records about its own activity while it serves a request.
//!
//! After serving a complete `Check` the entire log used to read `listening on …` and nothing else:
//! no request, no workspace root, no warm-or-cold, no outcome, no duration. An operator could not
//! tell a working daemon from a wedged one, could not see which roots were warm, and had no record
//! of a client that disconnected mid-run.
//!
//! **A logger-capture harness, deliberately, and only for the questions a pure function cannot
//! answer.** The wording of each line is pinned by unit tests beside the composers in
//! `src/activity.rs`; what this suite pins is *how many* lines a request produces and at which
//! level — which no test of a pure function can see. The first implementation of this logging
//! recorded two outcomes for every answered check, one for an intermediate path resolution and one
//! for the run itself, and its composers were all correct.
//!
//! `log::set_logger` succeeds once per process, and an integration test binary *is* its own
//! process: nothing else in the workspace shares this logger. The maximum level is `Info`, so what
//! this suite captures is exactly the journal an operator reading a daemon at its default level
//! gets — which is what makes "no line per operation" an assertion rather than an intention.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use tddy_index_daemon::proto::code_index::{
    CheckRequest, CodeIndexService, RestructureEvent, WarmRequest,
};
use tddy_index_daemon::{CodeIndexPorts, CodeIndexServiceImpl};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

// ─── The log this suite reads ──────────────────────────────────────────────────────────────

/// Every line the logger has been handed, in order, as `[target] message`.
fn the_log() -> &'static Mutex<Vec<String>> {
    static LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    LINES.get_or_init(|| Mutex::new(Vec::new()))
}

/// The logger itself, a `static` because `log::set_logger` takes a `&'static dyn Log` — this
/// crate's `log` dependency carries no `std` feature, so there is no boxed installer.
static CAPTURED_LOG: CapturedLog = CapturedLog;

struct CapturedLog;

impl log::Log for CapturedLog {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        the_log().lock().expect("the captured log").push(format!(
            "[{}] {}",
            record.target(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

/// A service whose log this suite can read, over the deterministic fake language server.
///
/// The logger is installed once for the whole binary, because `log::set_logger` succeeds once. The
/// tests still run in parallel: each reads only the lines naming *its own* workspace root, and a
/// `tempfile` root is unique per test.
fn a_daemon_whose_log_is_captured() -> CodeIndexServiceImpl {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        log::set_logger(&CAPTURED_LOG).expect("this binary's only logger");
        log::set_max_level(log::LevelFilter::Info);
    });

    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(tddy_code_restructuring::client_capabilities())
            .with_initialization_options(tddy_code_restructuring::server_settings()),
    );
    CodeIndexServiceImpl::new(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60)),
    })
}

/// Everything the log says about `named`, with each line's elapsed stamp elided.
///
/// The stamp holds a duration measured while the request ran, so no literal could match it; the
/// shape around it is compared exactly. Filtered by the root a test named, which is both what
/// keeps parallel tests out of each other's assertions and what makes those assertions about one
/// request.
fn the_log_about(named: &str) -> Vec<String> {
    the_log()
        .lock()
        .expect("the captured log")
        .iter()
        .filter(|line| line.contains(named))
        .map(|line| with_the_stamp_elided(line))
        .collect()
}

fn with_the_stamp_elided(line: &str) -> String {
    let Some((before, stamped)) = line.split_once("(+") else {
        return line.to_string();
    };
    let (_elapsed, after) = stamped
        .split_once(')')
        .unwrap_or_else(|| panic!("this line's elapsed stamp is never closed: {line}"));
    format!("{before}(+…){after}")
}

// ─── The workspace and the plan every test works against ───────────────────────────────────

/// A tree holding one twelve-line module and one three-line one, at a path that is not the process
/// directory and holds no `Cargo.toml` — so the root a request names is the root the log reports.
fn a_workspace_of_two_modules() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("src/big.rs"),
        "// a line\n".repeat(12),
    )
    .expect("the long module");
    std::fs::write(
        workspace.path().join("src/small.rs"),
        "// a line\n".repeat(3),
    )
    .expect("the short module");
    workspace
}

/// A plan renaming one symbol in each module, with an empty snapshot so nothing has to match the
/// tree. Two operations, because one line per operation at `INFO` is what this suite rules out.
fn a_plan_naming_both_modules(root: &Path) -> PathBuf {
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        r#"{"v":1,"snapshot":{}}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/big.rs","path":"Registry"},"name":"HostRegistry"}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/small.rs","path":"Clock"},"name":"HostClock"}
"#,
    )
    .expect("the plan");
    plan
}

/// A shallow check, which is answered from the plan text alone and therefore needs no index.
fn a_check_of(plan: &Path, root: &Path) -> CheckRequest {
    CheckRequest {
        workspace_root: root.to_string_lossy().to_string(),
        plan: plan.to_string_lossy().to_string(),
        deep: false,
        file_budget: 0,
    }
}

/// The journal of `named` once it holds `lines`, or the failure that it never did.
///
/// A dropped stream is the only disconnect signal this service gets, and it is noticed *inside* the
/// task serving the request — once the stream is gone there is no handle left to await, so the
/// journal is polled instead. Bounded, so a cancellation that is never recorded fails this test
/// rather than hanging it, and short, because the work being cancelled is one static check.
async fn the_log_about_once_it_holds(named: &str, lines: usize) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let journal = the_log_about(named);
            if journal.len() >= lines {
                return journal;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "the request never recorded {lines} line(s); it recorded {:?}",
            the_log_about(named)
        )
    })
}

/// Drain a served stream, so the request is over before its journal is read.
async fn drained(
    mut events: impl StreamExt<Item = Result<RestructureEvent, tddy_rpc::Status>> + Unpin,
) {
    while events.next().await.is_some() {}
}

// ─── The tests ─────────────────────────────────────────────────────────────────────────────

/// The two lines an operator reads per request, and only those two.
#[tokio::test(flavor = "multi_thread")]
async fn a_served_check_is_recorded_as_one_arrival_and_one_outcome() {
    // Given a daemon whose log is captured, and a two-operation plan for a root it holds no index
    // for
    let daemon = a_daemon_whose_log_is_captured();
    let workspace = a_workspace_of_two_modules();
    let root = workspace.path().to_string_lossy().to_string();
    let plan = a_plan_naming_both_modules(workspace.path());

    // When the check is served to the end of its stream
    let events = daemon
        .check(tddy_rpc::Request::new(a_check_of(&plan, workspace.path())))
        .await
        .expect("a shallow check of a sound plan is served")
        .into_inner();
    drained(events).await;

    // Then the whole journal of that request is its arrival — naming the method, the root and that
    // the root was cold — and its outcome with how long it took. Two operations and no line per
    // operation: a daemon serving a 55-minute capture must not write one per test at this level.
    assert_eq!(
        the_log_about(&root),
        vec![
            format!(
                "[tddy_index_daemon::activity] check arrived for `{root}`, which has no index yet"
            ),
            format!("[tddy_index_daemon::activity] check for `{root}`: answered (+…)"),
        ]
    );
}

/// A refusal is an outcome, and the code is what an operator acts on.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_check_is_recorded_with_the_status_its_caller_was_given() {
    // Given a daemon whose log is captured, and a check naming no plan at all
    let daemon = a_daemon_whose_log_is_captured();
    let workspace = a_workspace_of_two_modules();
    let root = workspace.path().to_string_lossy().to_string();

    // When it is served
    let refusal = daemon
        .check(tddy_rpc::Request::new(CheckRequest {
            workspace_root: root.clone(),
            plan: String::new(),
            deep: false,
            file_budget: 0,
        }))
        .await
        .expect_err("a check naming no plan is refused");

    // Then the caller's refusal and the log's account of it are the same thing
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert_eq!(
        the_log_about(&root),
        vec![
            format!(
                "[tddy_index_daemon::activity] check arrived for `{root}`, which has no index yet"
            ),
            format!(
                "[tddy_index_daemon::activity] check for `{root}`: refused as InvalidArgument \
                 (+…): the request names no plan"
            ),
        ]
    );
}

/// A request this process cannot even attribute to a tree is still a request that arrived: an
/// operator who saw nothing would read a client naming a relative root as a silent client.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_whose_root_cannot_be_resolved_is_recorded_as_the_refusal_it_got() {
    // Given a daemon whose log is captured, and a warm request naming a root relatively
    let daemon = a_daemon_whose_log_is_captured();
    let relative = "a/tree/named/relatively";

    // When it is served
    let refusal = daemon
        .warm(tddy_rpc::Request::new(WarmRequest {
            workspace_root: relative.to_string(),
        }))
        .await
        .expect_err("a relative root is refused");

    // Then the one line the request produced names it, the method and the code — there is no
    // arrival line, because there is no tree to have arrived for
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert_eq!(
        the_log_about(relative).len(),
        1,
        "a refused request wrote {:?}",
        the_log_about(relative)
    );
    assert!(
        the_log_about(relative)[0].starts_with(&format!(
            "[tddy_index_daemon::activity] warm for `{relative}`: refused as InvalidArgument (+…)"
        )),
        "the refusal was not recorded as one: {:?}",
        the_log_about(relative)
    );
}

/// The outcome the daemon otherwise leaves no record of at all.
///
/// The stream is dropped before it is read, which makes every send into it fail — and a failed send
/// is the *whole* disconnect signal this service has, since `tddy_rpc::RpcService` carries no
/// cancellation surface. So this is a client hanging up, without a slow operation to hang up in the
/// middle of.
///
/// **The single-threaded flavour is what makes that deterministic**, and it is the only test here
/// that needs it: the task serving the request is spawned inside `check` and cannot be polled until
/// this future yields, so the drop below is guaranteed to happen first. On a multi-threaded runtime
/// another worker could pick the task up and send its first progress line into a channel that is
/// still open — the send would succeed, nothing would be cancelled, and this test would pass or
/// fail on a race.
#[tokio::test]
async fn a_check_whose_caller_hangs_up_is_recorded_as_cancelled() {
    // Given a daemon whose log is captured, and a check of a two-operation plan
    let daemon = a_daemon_whose_log_is_captured();
    let workspace = a_workspace_of_two_modules();
    let root = workspace.path().to_string_lossy().to_string();
    let plan = a_plan_naming_both_modules(workspace.path());

    // When its caller takes the stream and goes away without reading it
    let events = daemon
        .check(tddy_rpc::Request::new(a_check_of(&plan, workspace.path())))
        .await
        .expect("a shallow check of a sound plan is served")
        .into_inner();
    drop(events);

    // Then the run is recorded as cancelled rather than answered: nothing was wrong with the
    // request, and an operator reading this knows the work stopped because nobody wanted it
    assert_eq!(
        the_log_about_once_it_holds(&root, 2).await,
        vec![
            format!(
                "[tddy_index_daemon::activity] check arrived for `{root}`, which has no index yet"
            ),
            format!(
                "[tddy_index_daemon::activity] check for `{root}`: cancelled (+…): its caller \
                 stopped listening"
            ),
        ]
    );
}
