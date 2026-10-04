use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_rpc::{Code, Status};

use super::*;
use crate::cli_session_manager::CliSessionManager;
use crate::test_util::{test_config, TEST_TOKEN, TEST_USER};

/// The instance id this daemon answers to — and the one the split session records as holding
/// its codebase, which is what makes the read below local rather than a peer forward.
const THIS_HOST: &str = "the-codebase-host";

/// The workspace session a split session's agent fetches its guidance from.
const CODEBASE_SESSION: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";

/// The agent half, recorded on the codebase session as the pairing that makes it a split one.
const AGENT_SESSION: &str = "bbbbbbbb-bbbb-7bbb-8bbb-bbbbbbbbbbbb";

/// The budget this host is configured to allow one context read.
///
/// One second because `spawn_worker_request_timeout_secs` is whole seconds and the refusal
/// quotes them: the shortest budget an operator can configure, so the message asserted below is
/// one production can really produce.
const A_ONE_SECOND_READ_BUDGET_SECS: u64 = 1;

/// How long this test waits for the fetch itself, which is *not* the behaviour under test: ten
/// times the budget above, so a context read bounded by nothing at all fails saying what never
/// happened instead of hanging the suite.
const A_CALLS_OWN_PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

/// A split session whose codebase half lives on this very daemon, with a checkout to read.
struct ASplitSession {
    _data_dir: tempfile::TempDir,
    _checkout: tempfile::TempDir,
    service: DaemonSessionHost,
}

/// This daemon holding the codebase of one split session, allowing a context read `budget_secs`.
fn a_split_session_whose_codebase_read_may_take(budget_secs: u64) -> ASplitSession {
    let data_dir = tempfile::tempdir().expect("a data dir");
    let checkout = tempfile::tempdir().expect("a checkout");
    std::fs::write(checkout.path().join("CLAUDE.md"), b"# the project's rules")
        .expect("the guidance file");
    a_codebase_session_in(data_dir.path(), checkout.path());

    let mut config = test_config();
    config.daemon_instance_id = Some(THIS_HOST.to_string());
    config.spawn_worker_request_timeout_secs = budget_secs;
    let base = data_dir.path().to_path_buf();
    let sessions_base: tddy_daemon_kernel::SessionsBaseResolver =
        Arc::new(move |_| Some(base.clone()));
    let users: tddy_daemon_kernel::SessionUserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| TEST_USER.to_string()));
    let service = DaemonSessionHost::new(
        config,
        sessions_base,
        data_dir.path().to_path_buf(),
        users,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    );
    ASplitSession {
        _data_dir: data_dir,
        _checkout: checkout,
        service,
    }
}

/// The `workspace` session a split start records on the codebase host: the checkout it holds,
/// and the agent half it is paired with — the pairing is what makes the `claude` allow-list the
/// row this session is served.
fn a_codebase_session_in(data_dir: &Path, checkout: &Path) {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(data_dir, CODEBASE_SESSION);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    std::fs::write(
        session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
        format!(
            "session_id: {CODEBASE_SESSION}\n\
             project_id: 019d105b-ac0f-78d3-9a89-409731145a40\n\
             created_at: 2026-09-11T09:00:00Z\n\
             updated_at: 2026-09-11T09:00:00Z\n\
             status: active\n\
             session_type: workspace\n\
             repo_path: {checkout}\n\
             agent_daemon_instance_id: the-agent-host\n\
             agent_session_id: {AGENT_SESSION}\n",
            checkout = checkout.display()
        ),
    )
    .expect("the session metadata");
}

/// Drive one fetch on a runtime whose only blocking thread is held by a read that never
/// returns, and give back the refusal it answered with.
///
/// The held read is released once the fetch has answered, so the queued read drains and the
/// runtime shuts down rather than the test leaking a parked thread.
fn the_refusal_when_the_read_cannot_start<Call, Answer, Fetched>(call: Call) -> Status
where
    Call: FnOnce() -> Answer,
    Answer: Future<Output = Result<Fetched, Status>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .max_blocking_threads(1)
        .build()
        .expect("a runtime with exactly one blocking thread");
    let (release, held) = std::sync::mpsc::channel::<()>();
    runtime.spawn_blocking(move || {
        let _ = held.recv();
    });

    let answer = runtime.block_on(async {
        tokio::time::timeout(A_CALLS_OWN_PATIENCE, call())
            .await
            .expect("the fetch never answered: the context read was bounded by no deadline")
    });

    release.send(()).expect("the held read to be releasable");
    answer
        .err()
        .expect("a read that cannot start inside the budget must refuse the start")
}

#[test]
fn refuses_a_split_start_whose_codebase_read_does_not_return_inside_the_hosts_budget() {
    // Given this host holding the codebase, allowing one context read a second
    let split = a_split_session_whose_codebase_read_may_take(A_ONE_SECOND_READ_BUDGET_SECS);

    // When the start fetches the project's guidance while that read cannot start
    let refusal = the_refusal_when_the_read_cannot_start(|| {
        split.service.split_context_from_codebase_host(
            TEST_TOKEN,
            CODEBASE_SESSION,
            THIS_HOST,
            "claude",
            "start",
        )
    });

    // Then the start is refused, naming the read that stalled and the key an operator raises
    assert_eq!(refusal.code(), Code::DeadlineExceeded);
    assert_eq!(
        refusal.message,
        format!(
            "cannot start a split session without the guidance held beside its codebase: \
             reading the context manifest from session {CODEBASE_SESSION} on daemon \
             {THIS_HOST} failed: StreamContextManifest: timed out after 1s \
             (spawn_worker_request_timeout_secs)"
        )
    );
}
