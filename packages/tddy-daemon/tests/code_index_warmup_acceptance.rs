//! What the daemon owes the session header's indexing indicator: once a session's worktree exists,
//! start warming its code index in the background through the index daemon this daemon manages;
//! and over `code_navigation.WatchCodeIndex`, follow that worktree's index — **whoever** started its
//! load (a session start, the code pane, the agent's code tools) — without ever starting one, nor
//! the index daemon itself. With nothing to follow the watch ends at once with no message.
//!
//! Feature docs: docs/ft/web/session-drawer.md (Start progress), docs/ft/web/session-code-pane.md
//! (Indexing indicator) and docs/ft/web/session-code-pane.md
//!
//! The index daemon is real process management over a stand-in (see `support`): the registry
//! starts a shell script that points the socket it was told to bind at a `code_index` server this
//! test hosts, which holds one root's progress the test moves. So the lazy start, the readiness
//! wait and the dial are the production ones.
//!
//! The warm is started by calling `code_index_warmup::warm_for_session` directly — the seam a
//! started session's worktree reaches — rather than by starting a session: the session start lives
//! in `tddy-session-lifecycle`, and what that crate streams while it starts is pinned there
//! (`start_phase_acceptance.rs`). The watch reads the session's worktree from its `.session.yaml`,
//! so no warm — and no record of one — is needed for a session to be watched.

#![cfg(unix)]

mod support;

use std::sync::Arc;

use prost::Message;
use support::{
    a_project_with_a_session_on_its_worktree, a_scripted_index, an_index_daemon_host_serving,
    indexing_at, the_index_failed, the_index_is_ready, the_web_sees_failed,
    the_web_sees_indexing_at, the_web_sees_ready, AProjectWithASession, AN_EVENT_IS_DUE_WITHIN,
    TEST_TOKEN, THE_SESSION,
};
use tddy_daemon::index_daemon::IndexDaemonRegistry;
use tddy_daemon_rpc::code_index_warmup::warm_for_session;
use tddy_daemon_rpc::code_navigation::{
    build_code_navigation_entry, CodeNavigationServiceImpl, IndexChannelSource,
};
use tddy_index_daemon::proto::code_index as index;
use tddy_service::proto::code_navigation::{CodeIndexProgress, WatchCodeIndexRequest};
use tddy_worktree_service::test_util::test_service;
use tempfile::TempDir;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// Starting a warm

/// Warm `project`'s session worktree for [`THE_SESSION`] and expect a warm to have started.
fn a_warm_of(
    project: &AProjectWithASession,
    registry: &IndexDaemonRegistry,
) -> tokio::task::JoinHandle<()> {
    let port = Arc::new(registry.clone()) as Arc<dyn IndexChannelSource>;
    warm_for_session(Some(&port), THE_SESSION, &project.worktree())
        .expect("a session on a Rust worktree starts a warm")
}

// ---------------------------------------------------------------------------------------------
// Watching the session's code index

/// The service as `runtime::build` registers it: authorising through `worktree_service`, and
/// forwarding to `index_daemon` when there is one.
fn the_navigation_entry_over(
    worktree_service: tddy_worktree_service::WorktreeServiceImpl,
    index_daemon: Option<IndexDaemonRegistry>,
) -> tddy_rpc::ServiceEntry {
    build_code_navigation_entry(CodeNavigationServiceImpl::new(
        Arc::new(worktree_service),
        index_daemon.map(|registry| Arc::new(registry) as Arc<dyn IndexChannelSource>),
    ))
}

/// A daemon freshly started — after a restart, say — over `project`'s user and `registry`: it has
/// warmed nothing and holds no progress of any session.
fn a_freshly_started_daemon_over(
    project: &AProjectWithASession,
    registry: IndexDaemonRegistry,
) -> tddy_rpc::ServiceEntry {
    the_navigation_entry_over(project.worktree_service.clone(), Some(registry))
}

/// Open `WatchCodeIndex` for [`THE_SESSION`] at the registered coordinate.
async fn watch_code_index_at(entry: &tddy_rpc::ServiceEntry) -> AWatch {
    AWatch {
        stream: try_watch_code_index_at(entry)
            .await
            .expect("WatchCodeIndex opens a stream for the session"),
    }
}

/// Ask for `WatchCodeIndex` of [`THE_SESSION`] at the registered coordinate, and get the stream or
/// the refusal.
async fn try_watch_code_index_at(
    entry: &tddy_rpc::ServiceEntry,
) -> Result<mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>, tddy_rpc::Status> {
    let request = WatchCodeIndexRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: THE_SESSION.to_string(),
    };
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    match entry
        .service
        .handle_rpc(entry.name, "WatchCodeIndex", &message)
        .await
    {
        tddy_rpc::RpcResult::ServerStream(opened) => opened,
        tddy_rpc::RpcResult::Unary(_) => panic!("WatchCodeIndex is a server stream"),
    }
}

/// An open `WatchCodeIndex` stream, which runs until the header leaves.
struct AWatch {
    stream: mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
}

impl AWatch {
    /// The watch ends without telling the header anything — so no indicator shows.
    async fn ends_with_nothing(&mut self) {
        let item = tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, self.stream.recv())
            .await
            .expect("WatchCodeIndex neither delivered nor ended within the timeout");
        assert!(
            item.is_none(),
            "expected the watch to end empty, got {item:?}"
        );
    }

    /// The next progress the header is told.
    async fn next_progress(&mut self) -> CodeIndexProgress {
        let item = tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, self.stream.recv())
            .await
            .expect("WatchCodeIndex sent nothing within the timeout")
            .expect(
                "WatchCodeIndex is still open: it follows the worktree until the header leaves",
            );
        let payload = item.expect("WatchCodeIndex delivers progress, not an error");
        CodeIndexProgress::decode(payload.as_slice()).expect("progress decodes")
    }
}

/// Why the scripted index's language server went away, in the failure tests.
const THE_SERVER_WENT_AWAY: &str =
    "the language server holding the worktree went away before its crate graph was loaded";

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_session_s_rust_worktree_handed_to_the_warm_up_is_warmed_through_the_index_daemon() {
    // Given a session's Rust worktree and a managed index daemon whose index of it is loaded
    let project = a_project_with_a_session_on_its_worktree();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    index.reports(the_index_is_ready());

    // When the session's worktree is handed to the warm-up
    let warming = a_warm_of(&project, &host.registry());
    tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, warming)
        .await
        .expect("the warm finishes within the timeout")
        .expect("the warm's task does not panic");

    // Then the index daemon was asked to warm exactly that worktree
    assert_eq!(
        index.warms_it_was_asked_for(),
        vec![index::WarmRequest {
            workspace_root: project.worktree_path.clone(),
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_index_daemon_no_warm_starts() {
    // Given a daemon configured without an `index_daemon:` section
    let project = a_project_with_a_session_on_its_worktree();

    // When the session's worktree is handed to the warm-up
    let warming = warm_for_session(None, THE_SESSION, &project.worktree());

    // Then nothing is started
    assert!(warming.is_none(), "no warm starts without an index daemon");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_follows_a_load_another_caller_started_and_never_warms() {
    // Given a session whose worktree's index another caller — the agent's code tool, say — has
    // started loading, now 40% through indexing
    let project = a_project_with_a_session_on_its_worktree();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let entry = a_freshly_started_daemon_over(&project, host.a_running_registry().await);
    index.reports(indexing_at(40));

    // When the session header watches the session's code index, and the load then finishes
    let mut watching = watch_code_index_at(&entry).await;
    let joined_at = watching.next_progress().await;
    index.reports(the_index_is_ready());
    let then = watching.next_progress().await;

    // Then it is told the phases it joined at, then that the index is ready
    assert_eq!(
        (joined_at, then),
        (the_web_sees_indexing_at(40), the_web_sees_ready())
    );
    // And it followed the session's worktree without asking for a warm of it
    assert_eq!(
        index.roots_it_was_asked_to_watch(),
        vec![index::WatchRootRequest {
            workspace_root: project.worktree_path.clone(),
        }]
    );
    assert_eq!(index.warms_it_was_asked_for(), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_for_a_session_after_a_restart_follows_its_worktree() {
    // Given a session started before this daemon restarted — only its `.session.yaml` remains,
    // nothing this daemon warmed — whose worktree's index is loaded
    let project = a_project_with_a_session_on_its_worktree();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    index.reports(the_index_is_ready());
    let entry = a_freshly_started_daemon_over(&project, host.a_running_registry().await);

    // When the session header watches the session's code index
    let mut watching = watch_code_index_at(&entry).await;
    let first = watching.next_progress().await;

    // Then it is told at once that the index is ready
    assert_eq!(first, the_web_sees_ready());
    // And what it followed is the worktree the session's record names
    assert_eq!(
        index.roots_it_was_asked_to_watch(),
        vec![index::WatchRootRequest {
            workspace_root: project.worktree_path.clone(),
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_reports_an_index_that_failed_with_its_reason() {
    // Given a session whose worktree's index is loading
    let project = a_project_with_a_session_on_its_worktree();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let entry = a_freshly_started_daemon_over(&project, host.a_running_registry().await);
    index.reports(indexing_at(40));

    // When the session header watches it, and the language server then goes away mid-load
    let mut watching = watch_code_index_at(&entry).await;
    let _joined_at = watching.next_progress().await;
    index.reports(the_index_failed(THE_SERVER_WENT_AWAY));
    let then = watching.next_progress().await;

    // Then the header is told the index failed, and why
    assert_eq!(then, the_web_sees_failed(THE_SERVER_WENT_AWAY));
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_while_no_index_daemon_runs_ends_with_nothing_and_starts_none() {
    // Given a session's Rust worktree and an index daemon nothing has started yet
    let project = a_project_with_a_session_on_its_worktree();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let registry = host.registry();
    let entry = a_freshly_started_daemon_over(&project, registry.clone());

    // When the session header watches the session's code index
    let mut watching = watch_code_index_at(&entry).await;

    // Then the watch ends with nothing to show
    watching.ends_with_nothing().await;
    // And watching did not start the index daemon
    assert!(registry.running().await.is_none());
    assert_eq!(index.roots_it_was_asked_to_watch(), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_for_a_session_whose_worktree_is_not_created_yet_ends_with_nothing() {
    // Given a session whose record names no worktree yet, and a running index daemon
    let project = a_project_with_a_session_on_its_worktree().whose_session_has_no_worktree_yet();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let entry = a_freshly_started_daemon_over(&project, host.a_running_registry().await);

    // When the session header watches the session's code index
    let mut watching = watch_code_index_at(&entry).await;

    // Then the watch ends with nothing to show, having followed no root
    watching.ends_with_nothing().await;
    assert_eq!(index.roots_it_was_asked_to_watch(), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_of_a_worktree_without_a_cargo_manifest_ends_with_nothing() {
    // Given a session whose worktree is not a Rust worktree, and a running index daemon
    let project = a_project_with_a_session_on_its_worktree().without_a_cargo_manifest();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let entry = a_freshly_started_daemon_over(&project, host.a_running_registry().await);

    // When the session header watches the session's code index
    let mut watching = watch_code_index_at(&entry).await;

    // Then the watch ends with nothing to show, having followed no root
    watching.ends_with_nothing().await;
    assert_eq!(index.roots_it_was_asked_to_watch(), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_without_an_index_daemon_section_ends_with_nothing() {
    // Given a daemon configured without an `index_daemon:` section
    let project = a_project_with_a_session_on_its_worktree();
    let entry = the_navigation_entry_over(project.worktree_service.clone(), None);

    // When the session header watches the session's code index
    let mut watching = watch_code_index_at(&entry).await;

    // Then the watch ends with nothing to show
    watching.ends_with_nothing().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_of_a_session_whose_record_is_unreadable_is_a_failed_precondition() {
    // Given a session whose `.session.yaml` is not a session record, and a running index daemon
    let project = a_project_with_a_session_on_its_worktree().whose_session_record_is_unreadable();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let entry = a_freshly_started_daemon_over(&project, host.a_running_registry().await);

    // When the session header watches the session's code index
    let refusal = try_watch_code_index_at(&entry)
        .await
        .map(|_| ())
        .expect_err("an unreadable record is refused");

    // Then it is refused as a failed precondition naming the record — the parser's own detail
    // follows the prefix and is not this daemon's to pin
    assert_eq!(refusal.code, tddy_rpc::Code::FailedPrecondition);
    assert!(
        refusal
            .message
            .starts_with("the session's .session.yaml cannot be read: "),
        "{}",
        refusal.message
    );
    assert_eq!(index.roots_it_was_asked_to_watch(), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn another_users_watch_of_the_session_is_refused_and_shows_no_progress() {
    // Given a session, its owner's, whose worktree's index is 40% through indexing
    let _the_owners_session = a_project_with_a_session_on_its_worktree();
    let index = a_scripted_index();
    let host = an_index_daemon_host_serving(index.clone()).await;
    index.reports(indexing_at(40));

    // When another user, whose sessions do not include it, watches the session's code index
    let others_sessions = TempDir::new().expect("the other user's data directory");
    let entry = the_navigation_entry_over(
        test_service(others_sessions.path().to_path_buf(), "another-user"),
        Some(host.registry()),
    );
    let refusal = try_watch_code_index_at(&entry).await;

    // Then it is refused as a session that is not theirs
    assert_eq!(
        refusal.map(|_| ()).map_err(|status| status.code),
        Err(tddy_rpc::Code::NotFound)
    );
    // And no worktree's progress was followed for them
    assert_eq!(index.roots_it_was_asked_to_watch(), vec![]);
}
