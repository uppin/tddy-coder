//! Acceptance tests: the deadline a blocking context read is bounded by.
//!
//! The three context methods answer a *split* session's agent host asking the host that holds the
//! codebase for its guidance, and the answer comes off a filesystem that can genuinely stall — a
//! network mount, a device that stopped responding. Unbounded, such a read leaves the RPC waiting
//! for exactly as long as the stall lasts: the caller has no answer to retry, nothing to report,
//! and an operator has nothing to raise. Bounded, the call comes back `DEADLINE_EXCEEDED` naming
//! the key that governs it.
//!
//! **How a read is made to stall here.** The read runs on the runtime's blocking pool, so these
//! tests give the runtime exactly one blocking thread and hand it a read that does not return until
//! the assertion has been made. The handler's own read is then queued behind it and cannot start
//! before the budget expires — deterministically, where a slow filesystem or a `sleep` would be a
//! wall-clock race that passes or fails with the load on the machine.
//!
//! Deliberately not under test here: which duration the daemon supplies (that is its wiring,
//! `svc_session_files_ports.rs`), and the forwarded path — a call routed to a peer is bounded by
//! the peer-forwarding deadline instead, before it ever reaches this crate.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session_files::{
    ContextManifestRequest, ReadContextFileBatchRequest, ReadContextFileRequest,
    SessionFilesService,
};
use tddy_session_files::service::{SessionContextScope, SessionContextScopes};
use tddy_session_files::{SessionFilesPorts, SessionFilesServiceImpl};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const OS_USER: &str = "tddy-test-user";
const SESSION_ID: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
const TOKEN: &str = "a-valid-session-token";
const AGENT: &str = "claude";

/// The budget this host is configured to allow one context read.
///
/// One second because `spawn_worker_request_timeout_secs` is whole seconds and the refusal quotes
/// them: this is the shortest budget an operator can actually configure
/// (`DaemonConfig::spawn_worker_request_timeout` clamps zero to one), so the message these tests
/// assert is a message production can really produce.
const A_ONE_SECOND_READ_BUDGET: Duration = Duration::from_secs(1);

/// A host serving one session's guidance out of a real checkout, under a configured read budget.
struct AContextHost {
    _root: tempfile::TempDir,
    service: SessionFilesServiceImpl,
}

/// A host whose checkout holds a `CLAUDE.md` the allow-list names, so the only reason a read of it
/// can fail is the budget.
fn a_context_host_allowing_a_read_to_take(budget: Duration) -> AContextHost {
    let root = tempfile::tempdir().expect("a temp dir");
    let worktree_root = root.path().join("checkout");
    std::fs::create_dir_all(&worktree_root).expect("the checkout");
    std::fs::write(worktree_root.join("CLAUDE.md"), b"# the project's rules")
        .expect("the guidance file");

    let ports = SessionFilesPorts {
        os_users: Arc::new(|token: &str| {
            if token == TOKEN {
                Ok(OS_USER.to_string())
            } else {
                Err(Status::unauthenticated("invalid or expired session"))
            }
        }),
        tddy_data_dir: root.path().join("data"),
        staging_base_dir: root.path().join("staging"),
        max_attachment_bytes: 4 * 1024 * 1024,
        daemon_instance_id: "the-codebase-host".to_string(),
        context_scopes: Arc::new(TheCheckoutAt(worktree_root)),
        context_read_deadline: budget,
    };
    AContextHost {
        _root: root,
        service: SessionFilesServiceImpl::new(ports),
    }
}

/// The one checkout this host serves, under Claude's compiled-in allow-list row.
struct TheCheckoutAt(PathBuf);

impl SessionContextScopes for TheCheckoutAt {
    fn scope_for(&self, _: &str, _: &str, _: &str) -> Result<SessionContextScope, Status> {
        Ok(SessionContextScope {
            worktree_root: self.0.clone(),
            globs: tddy_session_files::context_files::context_globs_for_session_type("claude-cli"),
        })
    }
}

fn a_manifest_request() -> ContextManifestRequest {
    ContextManifestRequest {
        session_token: TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: String::new(),
        agent: AGENT.to_string(),
    }
}

fn a_read_of(rel_path: &str) -> ReadContextFileRequest {
    ReadContextFileRequest {
        session_token: TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: String::new(),
        agent: AGENT.to_string(),
        rel_path: rel_path.to_string(),
    }
}

fn a_batch_read_of(rel_paths: &[&str]) -> ReadContextFileBatchRequest {
    ReadContextFileBatchRequest {
        session_token: TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: String::new(),
        agent: AGENT.to_string(),
        rel_paths: rel_paths.iter().map(|p| (*p).to_string()).collect(),
    }
}

/// How long this suite waits for the handler itself, which is *not* the behaviour under test: it is
/// ten times the budget above, so a context read that is bounded by nothing at all fails here
/// saying what never happened instead of hanging the suite forever.
const A_CALLS_OWN_PATIENCE: Duration = Duration::from_secs(10);

/// Drive one handler on a runtime whose only blocking thread is held by a read that never returns,
/// and give back the refusal the handler answered with.
///
/// The held read is released once the call has answered, so the queued read drains and the runtime
/// shuts down rather than the suite leaking a parked thread.
fn the_refusal_when_the_read_cannot_start<Call, Answer, Served>(call: Call) -> Status
where
    Call: FnOnce() -> Answer,
    Answer: Future<Output = Result<Served, Status>>,
    Served: std::fmt::Debug,
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
            .expect("the handler never answered: the context read was bounded by no deadline")
    });

    release.send(()).expect("the held read to be releasable");
    answer.expect_err("a read that cannot start inside the budget must be refused")
}

// ---------------------------------------------------------------------------
// The deadline
// ---------------------------------------------------------------------------

#[test]
fn refuses_a_context_manifest_whose_read_does_not_return_inside_the_hosts_budget() {
    // Given a host allowing a read one second, and a read that never returns
    let host = a_context_host_allowing_a_read_to_take(A_ONE_SECOND_READ_BUDGET);

    // When
    let refusal = the_refusal_when_the_read_cannot_start(|| {
        host.service
            .stream_context_manifest(Request::new(a_manifest_request()))
    });

    // Then — the message names the key an operator raises, because nothing else tells them
    assert_eq!(refusal.code(), Code::DeadlineExceeded);
    assert_eq!(
        refusal.message,
        "StreamContextManifest: timed out after 1s (spawn_worker_request_timeout_secs)"
    );
}

#[test]
fn refuses_a_context_file_read_that_does_not_return_inside_the_hosts_budget() {
    // Given
    let host = a_context_host_allowing_a_read_to_take(A_ONE_SECOND_READ_BUDGET);

    // When
    let refusal = the_refusal_when_the_read_cannot_start(|| {
        host.service
            .stream_read_context_file(Request::new(a_read_of("CLAUDE.md")))
    });

    // Then
    assert_eq!(refusal.code(), Code::DeadlineExceeded);
    assert_eq!(
        refusal.message,
        "StreamReadContextFile: timed out after 1s (spawn_worker_request_timeout_secs)"
    );
}

#[test]
fn refuses_a_context_file_batch_whose_read_does_not_return_inside_the_hosts_budget() {
    // Given
    let host = a_context_host_allowing_a_read_to_take(A_ONE_SECOND_READ_BUDGET);

    // When
    let refusal = the_refusal_when_the_read_cannot_start(|| {
        host.service
            .stream_read_context_file_batch(Request::new(a_batch_read_of(&["CLAUDE.md"])))
    });

    // Then
    assert_eq!(refusal.code(), Code::DeadlineExceeded);
    assert_eq!(
        refusal.message,
        "StreamReadContextFileBatch: timed out after 1s (spawn_worker_request_timeout_secs)"
    );
}
