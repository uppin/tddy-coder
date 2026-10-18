//! What `code_navigation.CodeSession` owes the web's code pane: one bidirectional stream per open
//! pane, authorised like every navigation request, which **warms** the pane's worktree — and warms
//! it again when its index is let go while the pane is open — streams its index's progress for as
//! long as it is open, and answers the pane's queries — **holding** a hover, definition or
//! references asked while the index loads until it is ready (or answering the failure, when it
//! fails), never holding a probe, and dropping a held query the pane cancels. An open session is use
//! of the index daemon and of its worktree's root, so neither is let go for sitting idle while the
//! pane is open. A worktree that is not a Rust worktree has no index: nothing is warmed, and every
//! query is answered with why.
//!
//! Feature docs: docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md
//!
//! Every test drives the stream the way a bidi transport does — `start_bidi_stream` at the
//! registered coordinate, encoded `CodeSessionRequest` frames pushed into the input channel,
//! encoded `CodeSessionEvent`s read off the output — so a handler that compiles but is unreachable
//! fails here rather than in the browser.
//!
//! The index daemon is real process management over a stand-in (see `support`), and what answers
//! is an in-memory index holding one root's progress, which each test moves: its navigation answers
//! are empty and `LOADING` until the test reports the root `READY`.

#![cfg(unix)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use prost::Message;
use support::{
    a_checkout_outside_the_project, a_project_with_a_session_on_its_worktree, a_scripted_index,
    an_index_daemon_host_serving, canonical, indexing_at, the_call_to_foo, the_fn_keyword,
    the_index_failed, the_index_is_ready, the_index_s_call_to_foo, the_root_is_idle,
    the_web_s_location_of_foo, the_web_s_reference_to_foo, the_web_sees_indexing_at,
    the_web_sees_ready, AProjectWithASession, AScriptedIndex, AN_EVENT_IS_DUE_WITHIN, TEST_TOKEN,
    THE_INDEX_S_HOVER_OF_FOO,
};
use tddy_daemon::index_daemon::IndexDaemonRegistry;
use tddy_daemon_rpc::code_navigation::{
    build_code_navigation_entry, CodeNavigationServiceImpl, IndexChannelSource,
    CODE_NAVIGATION_SERVICE,
};
use tddy_index_daemon::proto::code_index as index;
use tddy_service::proto::code_navigation::code_query_answer::Result as Answered;
use tddy_service::proto::code_navigation::code_session_event::Event;
use tddy_service::proto::code_navigation::code_session_request::Frame;
use tddy_service::proto::code_navigation::{
    CancelCodeQuery, CodeIndexProgress, CodeIndexState, CodeQuery, CodeQueryAnswer, CodeQueryHeld,
    CodeQueryKind, CodeSessionEvent, CodeSessionRequest, Definability, DefinitionResponse,
    HoverResponse, OpenCodeSession, ProbeAnswer, ReferencesResponse, SourcePosition,
};
use tddy_task::TaskId;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// The service under test

fn the_entry_for(
    project: &AProjectWithASession,
    index_daemon: IndexDaemonRegistry,
) -> tddy_rpc::ServiceEntry {
    build_code_navigation_entry(CodeNavigationServiceImpl::new(
        Arc::new(project.worktree_service.clone()),
        Some(Arc::new(index_daemon) as Arc<dyn IndexChannelSource>),
    ))
}

/// The `open` frame the pane sends for `worktree_path` of `project`.
fn opening(project: &AProjectWithASession, worktree_path: &str) -> CodeSessionRequest {
    opening_as(TEST_TOKEN, project, worktree_path)
}

/// The `open` frame a pane signed in with `session_token` sends for `worktree_path` of `project`.
fn opening_as(
    session_token: &str,
    project: &AProjectWithASession,
    worktree_path: &str,
) -> CodeSessionRequest {
    CodeSessionRequest {
        frame: Some(Frame::Open(OpenCodeSession {
            session_token: session_token.to_string(),
            project_id: project.project_id.clone(),
            worktree_path: worktree_path.to_string(),
        })),
    }
}

/// The pane's `open` of its own, listed worktree.
fn opening_its_worktree(project: &AProjectWithASession) -> CodeSessionRequest {
    opening(project, &project.worktree_path)
}

/// A `kind` query about the call to `foo` in `src/main.rs`, numbered `request_id`.
fn a_query(request_id: u64, kind: CodeQueryKind) -> CodeSessionRequest {
    a_query_at(request_id, kind, the_call_to_foo())
}

/// A `kind` query about `position` in `src/main.rs`, numbered `request_id`.
fn a_query_at(
    request_id: u64,
    kind: CodeQueryKind,
    position: SourcePosition,
) -> CodeSessionRequest {
    CodeSessionRequest {
        frame: Some(Frame::Query(CodeQuery {
            request_id,
            kind: kind as i32,
            rel_path: "src/main.rs".to_string(),
            position: Some(position),
        })),
    }
}

fn a_hover(request_id: u64) -> CodeSessionRequest {
    a_query(request_id, CodeQueryKind::Hover)
}

fn a_probe(request_id: u64) -> CodeSessionRequest {
    a_query(request_id, CodeQueryKind::Probe)
}

fn a_definition(request_id: u64) -> CodeSessionRequest {
    a_query(request_id, CodeQueryKind::Definition)
}

fn a_references(request_id: u64) -> CodeSessionRequest {
    a_query(request_id, CodeQueryKind::References)
}

fn a_cancel_of(request_id: u64) -> CodeSessionRequest {
    CodeSessionRequest {
        frame: Some(Frame::Cancel(CancelCodeQuery { request_id })),
    }
}

fn encoded(frame: &CodeSessionRequest) -> tddy_rpc::RpcMessage {
    tddy_rpc::RpcMessage::new(
        frame.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    )
}

/// Start `CodeSession` at the registered coordinate with `first` already in its input, as a bidi
/// transport does: the frame is queued before the handler is handed the stream.
async fn start_a_code_session(
    entry: &tddy_rpc::ServiceEntry,
    first: CodeSessionRequest,
) -> (
    mpsc::Sender<tddy_rpc::RpcMessage>,
    Result<tddy_rpc::BidiStreamOutput, tddy_rpc::Status>,
) {
    let (input, input_rx) = mpsc::channel(16);
    input
        .send(encoded(&first))
        .await
        .expect("the decoder takes the first frame");
    let started = entry
        .service
        .start_bidi_stream(
            CODE_NAVIGATION_SERVICE,
            "CodeSession",
            tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
            input_rx,
        )
        .await;
    (input, started)
}

/// A pane's open code session: the frames it sends and the events it receives.
struct ACodeSession {
    input: mpsc::Sender<tddy_rpc::RpcMessage>,
    output: mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
}

/// Open a code session with `open` and expect it to be accepted.
async fn a_code_session_opened_with(
    entry: &tddy_rpc::ServiceEntry,
    open: CodeSessionRequest,
) -> ACodeSession {
    let (input, started) = start_a_code_session(entry, open).await;
    let output = match started.expect("the code session opens").output {
        tddy_rpc::ResponseBody::Streaming(rx) => rx,
        tddy_rpc::ResponseBody::Complete(_) => panic!("CodeSession answered without a live stream"),
    };
    ACodeSession { input, output }
}

/// How a code session whose first frame is `first` ends. Whether the refusal comes back from the
/// start or as the stream's first item is the transport's detail, not the pane's: both reach the
/// browser as the call's status.
async fn how_a_code_session_ends(
    entry: &tddy_rpc::ServiceEntry,
    first: CodeSessionRequest,
) -> tddy_rpc::Status {
    let (_input, started) = start_a_code_session(entry, first).await;
    let mut output = match started {
        Err(refused) => return refused,
        Ok(opened) => match opened.output {
            tddy_rpc::ResponseBody::Streaming(rx) => rx,
            tddy_rpc::ResponseBody::Complete(_) => {
                panic!("CodeSession answered without a live stream")
            }
        },
    };
    let first_item = tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, output.recv())
        .await
        .expect("the code session neither delivered nor ended within the timeout")
        .expect("the code session ends with a status, not silently");
    first_item.expect_err("the code session ends before delivering any event")
}

impl ACodeSession {
    async fn sends(&self, frame: CodeSessionRequest) {
        self.input
            .send(encoded(&frame))
            .await
            .expect("the code session is still reading frames");
    }

    /// The next event, whatever its kind.
    async fn next_event(&mut self) -> Event {
        let item = tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, self.output.recv())
            .await
            .expect("the code session sent nothing within the timeout")
            .expect("the code session is still open");
        let payload = item.expect("the code session delivers events, not an error");
        CodeSessionEvent::decode(payload.as_slice())
            .expect("the event decodes")
            .event
            .expect("an event carries a kind")
    }

    /// The next event, which must be progress.
    async fn next_progress(&mut self) -> CodeIndexProgress {
        let event = self.next_event().await;
        let Event::Progress(progress) = event else {
            panic!("expected progress, got {event:?}");
        };
        progress
    }

    /// The next event about a query — `held` or `answer` — passing over progress, which arrives
    /// whenever the index moves and is not what a query waits for.
    async fn next_reply(&mut self) -> Event {
        loop {
            let event = self.next_event().await;
            if !matches!(event, Event::Progress(_)) {
                return event;
            }
        }
    }

    /// How the session ends, passing over progress: the status its stream ends with.
    async fn ends_with(&mut self) -> tddy_rpc::Status {
        loop {
            let item = tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, self.output.recv())
                .await
                .expect("the code session neither delivered nor ended within the timeout")
                .expect("the code session ends with a status, not silently");
            let payload = match item {
                Err(status) => return status,
                Ok(payload) => payload,
            };
            let event = CodeSessionEvent::decode(payload.as_slice())
                .expect("the event decodes")
                .event;
            assert!(
                matches!(event, Some(Event::Progress(_))),
                "expected only progress before the end, got {event:?}"
            );
        }
    }

    /// Progress events, from the next one up to and including the first that says `READY`.
    async fn progress_until_ready(&mut self) -> Vec<CodeIndexProgress> {
        let mut seen = Vec::new();
        loop {
            let progress = self.next_progress().await;
            let ready = progress.state() == CodeIndexState::Ready;
            seen.push(progress);
            if ready {
                return seen;
            }
        }
    }
}

fn held(request_id: u64) -> Event {
    Event::Held(CodeQueryHeld { request_id })
}

fn answered(request_id: u64, result: Answered) -> Event {
    Event::Answer(CodeQueryAnswer {
        request_id,
        result: Some(result),
    })
}

/// A loaded index's hover of `foo`.
fn the_hover_of_foo() -> Answered {
    Answered::Hover(HoverResponse {
        markdown: Some(THE_INDEX_S_HOVER_OF_FOO.to_string()),
        state: CodeIndexState::Ready as i32,
    })
}

/// A loaded index's definition of `foo`.
fn the_definition_of_foo() -> Answered {
    Answered::Definition(DefinitionResponse {
        locations: vec![the_web_s_location_of_foo()],
        state: CodeIndexState::Ready as i32,
    })
}

/// A loaded index's references to `foo`.
fn the_references_to_foo() -> Answered {
    Answered::References(ReferencesResponse {
        locations: vec![the_web_s_reference_to_foo()],
        state: CodeIndexState::Ready as i32,
    })
}

/// The progress a worktree with no code index is reported with: `IDLE`, and nothing else.
fn no_index() -> CodeIndexProgress {
    CodeIndexProgress {
        state: CodeIndexState::None as i32,
        error: NOT_A_RUST_WORKTREE.to_string(),
        ..Default::default()
    }
}

/// What every query on a worktree that is not a Rust worktree is answered.
const NOT_A_RUST_WORKTREE: &str =
    "this worktree has no Cargo.toml at its root, so it is not a Rust worktree and has no code index";

/// Why the scripted index's language server went away, in the failure tests.
const THE_SERVER_WENT_AWAY: &str =
    "the language server holding the worktree went away before its crate graph was loaded";

fn a_probe_answer(
    definability: Definability,
    locations: Vec<tddy_service::proto::code_navigation::CodeLocation>,
) -> Answered {
    Answered::Probe(ProbeAnswer {
        definability: definability as i32,
        locations,
    })
}

/// The index's hover request for the call to `foo`, rooted at `project`'s worktree.
fn the_hover_of_foo_asked_in(project: &AProjectWithASession) -> index::HoverRequest {
    index::HoverRequest {
        workspace_root: project.worktree_path.clone(),
        file: "src/main.rs".to_string(),
        position: Some(the_index_s_call_to_foo()),
    }
}

/// A project's session worktree, a managed index daemon in front of `index`, and the service.
async fn a_pane_s_daemon_over(
    index: &AScriptedIndex,
) -> (
    AProjectWithASession,
    IndexDaemonRegistry,
    tddy_rpc::ServiceEntry,
    support::AnIndexDaemonHost,
) {
    a_pane_s_daemon_letting_its_index_daemon_go_after(index, Duration::from_secs(600)).await
}

/// [`a_pane_s_daemon_over`], whose index daemon is stopped once it has sat unused for
/// `idle_timeout` — which also sets how often an open session says it is still in use.
async fn a_pane_s_daemon_letting_its_index_daemon_go_after(
    index: &AScriptedIndex,
    idle_timeout: Duration,
) -> (
    AProjectWithASession,
    IndexDaemonRegistry,
    tddy_rpc::ServiceEntry,
    support::AnIndexDaemonHost,
) {
    let project = a_project_with_a_session_on_its_worktree();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let registry = host.registry_letting_its_process_go_after(idle_timeout);
    let entry = the_entry_for(&project, registry.clone());
    (project, registry, entry, host)
}

/// An idle timeout short enough to outlast in a test: the registry derives the keep-alive beat from
/// it (a third of it), so an open session says it is in use several times within one timeout.
const A_SHORT_IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// How often the tests' idle reaper looks, as the daemon's ticker does — only more often.
const THE_REAPER_LOOKS_EVERY: Duration = Duration::from_millis(100);

/// Every process the idle reaper stops while it looks over `span`.
async fn what_the_reaper_stops_over(registry: &IndexDaemonRegistry, span: Duration) -> Vec<TaskId> {
    let until = tokio::time::Instant::now() + span;
    let mut stopped = Vec::new();
    while tokio::time::Instant::now() < until {
        stopped.extend(registry.reap_idle().await);
        tokio::time::sleep(THE_REAPER_LOOKS_EVERY).await;
    }
    stopped
}

/// The process the idle reaper stops first, looking for up to [`AN_EVENT_IS_DUE_WITHIN`] — or
/// `None`, when it stops nothing by then.
async fn what_the_reaper_stops_first(registry: &IndexDaemonRegistry) -> Option<TaskId> {
    let until = tokio::time::Instant::now() + AN_EVENT_IS_DUE_WITHIN;
    while tokio::time::Instant::now() < until {
        if let Some(stopped) = registry.reap_idle().await {
            return Some(stopped);
        }
        tokio::time::sleep(THE_REAPER_LOOKS_EVERY).await;
    }
    None
}

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn opening_a_code_session_warms_its_worktree_and_streams_progress_until_ready() {
    // Given a project's worktree whose index is 10% through indexing
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));

    // When the pane opens a code session on it, and the index moves on to 60% and then is ready
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    let joined_at = session.next_progress().await;
    index.reports(indexing_at(60));
    let moved_to = session.next_progress().await;
    index.reports(the_index_is_ready());
    let then = session.progress_until_ready().await;

    // Then the pane is told where the index stood, each change, and that it is ready
    assert_eq!(
        (joined_at, moved_to, then),
        (
            the_web_sees_indexing_at(10),
            the_web_sees_indexing_at(60),
            vec![the_web_sees_ready()]
        )
    );
    // And opening the session is what asked the index daemon to warm exactly that worktree
    assert_eq!(
        index.warms_it_was_asked_for(),
        vec![index::WarmRequest {
            workspace_root: project.worktree_path.clone(),
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hover_asked_while_the_index_loads_is_held_and_answered_once_it_is_ready() {
    // Given a pane's open code session on a worktree whose index is still loading
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the pane hovers the call to `foo`
    session.sends(a_hover(7)).await;

    // Then the hover is held rather than answered empty
    assert_eq!(session.next_reply().await, held(7));

    // And when the index becomes ready, the held hover is answered with the loaded index's text
    index.reports(the_index_is_ready());
    assert_eq!(session.next_reply().await, answered(7, the_hover_of_foo()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_held_hover_is_never_answered() {
    // Given a pane's open code session whose hover is held while the index loads
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.sends(a_hover(7)).await;
    assert_eq!(session.next_reply().await, held(7));

    // When the pane cancels it — the pointer left — and the index then becomes ready
    session.sends(a_cancel_of(7)).await;
    index.reports(the_index_is_ready());
    session.progress_until_ready().await;

    // Then the next reply the pane gets is to its next query, not to the cancelled hover
    session.sends(a_probe(8)).await;
    assert_eq!(
        session.next_reply().await,
        answered(
            8,
            a_probe_answer(Definability::Definable, vec![the_web_s_location_of_foo()])
        )
    );
    // And the cancelled hover was never asked of the index again
    assert_eq!(
        index.hovers_it_was_asked(),
        vec![the_hover_of_foo_asked_in(&project)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_probe_while_the_index_loads_answers_unknown_without_waiting() {
    // Given a pane's open code session on a worktree whose index is still loading
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the pane's pointer dwells on the call to `foo` and it probes for a definition
    session.sends(a_probe(3)).await;

    // Then it is told at once that the index cannot say yet — never held
    assert_eq!(
        session.next_reply().await,
        answered(3, a_probe_answer(Definability::Unknown, vec![]))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_probe_on_a_symbol_with_a_definition_answers_definable_with_its_location() {
    // Given a pane's open code session on a worktree whose index is loaded
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(the_index_is_ready());
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the pane probes the call to `foo`
    session.sends(a_probe(3)).await;

    // Then it is definable, and the answer carries where — as a worktree-relative location — so a
    // click can go there without asking again
    assert_eq!(
        session.next_reply().await,
        answered(
            3,
            a_probe_answer(Definability::Definable, vec![the_web_s_location_of_foo()])
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_code_session_on_a_worktree_the_project_does_not_list_is_refused() {
    // Given a project, a checkout that is not one of its worktrees, and a managed index daemon
    let index = a_scripted_index();
    let (project, registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    let elsewhere = a_checkout_outside_the_project();

    // When a code session is opened on that checkout
    let refusal =
        how_a_code_session_ends(&entry, opening(&project, &canonical(elsewhere.path()))).await;

    // Then it is refused exactly as the worktree service refuses it
    assert_eq!(
        (refusal.code(), refusal.message()),
        (
            tddy_rpc::Code::FailedPrecondition,
            "worktree_path is not a worktree of this project"
        )
    );
    // And nothing was warmed — the index daemon was not even started for it
    assert_eq!(index.warms_it_was_asked_for(), vec![]);
    assert!(registry.running().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_query_before_open_ends_the_session() {
    // Given a project's worktree and a managed index daemon
    let index = a_scripted_index();
    let (_project, registry, entry, _host) = a_pane_s_daemon_over(&index).await;

    // When a code session's first frame is a query rather than `open`
    let refusal = how_a_code_session_ends(&entry, a_hover(1)).await;

    // Then the session ends as an invalid argument saying why, and nothing was warmed or started
    assert_eq!(
        (refusal.code(), refusal.message()),
        (
            tddy_rpc::Code::InvalidArgument,
            "a code session's first frame must be `open`"
        )
    );
    assert_eq!(index.warms_it_was_asked_for(), vec![]);
    assert!(registry.running().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_definition_asked_while_the_index_loads_is_held_and_answered_once_it_is_ready() {
    // Given a pane's open code session on a worktree whose index is still loading
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the pane asks where `foo` is defined, and the index then becomes ready
    session.sends(a_definition(4)).await;
    let first = session.next_reply().await;
    index.reports(the_index_is_ready());
    let then = session.next_reply().await;

    // Then the definition is held, then answered with the loaded index's location
    assert_eq!(
        (first, then),
        (held(4), answered(4, the_definition_of_foo()))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn references_asked_while_the_index_loads_are_held_and_answered_once_it_is_ready() {
    // Given a pane's open code session on a worktree whose index is still loading
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the pane asks for the references to `foo`, and the index then becomes ready
    session.sends(a_references(5)).await;
    let first = session.next_reply().await;
    index.reports(the_index_is_ready());
    let then = session.next_reply().await;

    // Then the query is held, then answered with the loaded index's references
    assert_eq!(
        (first, then),
        (held(5), answered(5, the_references_to_foo()))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_hover_on_an_index_that_then_fails_is_answered_with_the_failure() {
    // Given a pane's open code session whose hover is held while the index loads
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.sends(a_hover(7)).await;
    assert_eq!(session.next_reply().await, held(7));

    // When the language server goes away before the index is loaded
    index.reports(the_index_failed(THE_SERVER_WENT_AWAY));

    // Then the held hover is answered with the failure and its reason, rather than left waiting
    assert_eq!(
        session.next_reply().await,
        answered(
            7,
            Answered::Error(format!("the code index failed: {THE_SERVER_WENT_AWAY}"))
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_probe_where_nothing_is_defined_answers_none() {
    // Given a pane's open code session on a worktree whose index is loaded
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(the_index_is_ready());
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the pane probes the `fn` keyword, where nothing is defined
    session
        .sends(a_query_at(3, CodeQueryKind::Probe, the_fn_keyword()))
        .await;

    // Then it is told there is no definition there, so nothing is underlined
    assert_eq!(
        session.next_reply().await,
        answered(3, a_probe_answer(Definability::None, vec![]))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_query_reusing_a_held_query_s_id_replaces_it() {
    // Given a pane's open code session whose hover 7 is held while the index loads
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.sends(a_hover(7)).await;
    assert_eq!(session.next_reply().await, held(7));

    // When the pane asks a probe under the same id, and the index then becomes ready
    session.sends(a_probe(7)).await;
    let the_probe_s_answer = session.next_reply().await;
    index.reports(the_index_is_ready());
    session.progress_until_ready().await;

    // Then the probe is answered under that id
    assert_eq!(
        the_probe_s_answer,
        answered(7, a_probe_answer(Definability::Unknown, vec![]))
    );
    // And the hover it replaced is never answered: the next reply is to the pane's next query
    session.sends(a_probe(8)).await;
    assert_eq!(
        session.next_reply().await,
        answered(
            8,
            a_probe_answer(Definability::Definable, vec![the_web_s_location_of_foo()])
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_index_let_go_while_the_pane_is_open_is_warmed_again() {
    // Given a pane's open code session on a worktree whose index is loaded
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(the_index_is_ready());
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.progress_until_ready().await;

    // When the index lets the root's server go for sitting unused
    index.reports(the_root_is_idle());
    let told = session.next_progress().await;

    // Then the pane is told the root is idle
    assert_eq!(told.state(), CodeIndexState::Idle);
    // And the session — the pane still being open — has asked the index daemon to warm it again
    let a_warm_of_the_worktree = index::WarmRequest {
        workspace_root: project.worktree_path.clone(),
    };
    assert_eq!(
        index.warms_it_was_asked_for(),
        vec![a_warm_of_the_worktree.clone(), a_warm_of_the_worktree]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hover_asked_after_the_index_was_let_go_is_held_until_it_is_ready_again() {
    // Given a pane's open code session whose loaded index was then let go for sitting unused
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(the_index_is_ready());
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.progress_until_ready().await;
    index.reports(the_root_is_idle());
    session.next_progress().await;

    // When the pane hovers the call to `foo`, and the warmed-again index becomes ready
    session.sends(a_hover(9)).await;
    let first = session.next_reply().await;
    index.reports(indexing_at(50));
    index.reports(the_index_is_ready());
    let then = session.next_reply().await;

    // Then the hover is held through the reload and answered once the index is ready
    assert_eq!((first, then), (held(9), answered(9, the_hover_of_foo())));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_code_session_whose_index_stops_following_its_worktree_ends_unavailable() {
    // Given a pane's open code session on a worktree whose index is loading
    let index = a_scripted_index();
    let (project, _registry, entry, _host) = a_pane_s_daemon_over(&index).await;
    index.reports(indexing_at(10));
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;

    // When the index daemon stops following the worktree
    index.stops_following();
    let ending = session.ends_with().await;

    // Then the session ends as unavailable, saying why
    assert_eq!(
        (ending.code(), ending.message()),
        (
            tddy_rpc::Code::Unavailable,
            "the index daemon stopped following the worktree"
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_code_session_opened_with_a_token_nobody_holds_is_refused() {
    // Given a project's worktree and a managed index daemon
    let index = a_scripted_index();
    let (project, registry, entry, _host) = a_pane_s_daemon_over(&index).await;

    // When a code session is opened on it with a session token nobody signed in with
    let refusal = how_a_code_session_ends(
        &entry,
        opening_as("not-a-session-token", &project, &project.worktree_path),
    )
    .await;

    // Then it is refused as unauthenticated, and nothing was warmed or started
    assert_eq!(
        (refusal.code(), refusal.message()),
        (
            tddy_rpc::Code::Unauthenticated,
            "invalid or expired session"
        )
    );
    assert_eq!(index.warms_it_was_asked_for(), vec![]);
    assert!(registry.running().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_code_session_on_a_worktree_that_is_not_a_rust_worktree_has_no_index() {
    // Given a project's worktree with no Cargo.toml at its root, and a managed index daemon
    let index = a_scripted_index();
    let project = a_project_with_a_session_on_its_worktree().without_a_cargo_manifest();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let registry = host.registry();
    let entry = the_entry_for(&project, registry.clone());

    // When the pane opens a code session on it and hovers a position
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    let told = session.next_progress().await;
    session.sends(a_hover(2)).await;
    let reply = session.next_reply().await;

    // Then the pane is told the worktree has no index and why, and the hover is answered with why
    assert_eq!(
        (told, reply),
        (
            no_index(),
            answered(2, Answered::Error(NOT_A_RUST_WORKTREE.to_string()))
        )
    );
    // And nothing was warmed or followed — the index daemon was not even started for it
    assert_eq!(index.warms_it_was_asked_for(), vec![]);
    assert!(registry.running().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_open_code_session_keeps_the_index_daemon_running_and_lets_it_go_once_closed() {
    // Given a daemon that stops its index daemon once it has sat unused for a second
    let index = a_scripted_index();
    let (project, registry, entry, _host) =
        a_pane_s_daemon_letting_its_index_daemon_go_after(&index, A_SHORT_IDLE_TIMEOUT).await;
    index.reports(the_index_is_ready());
    // And a pane's open code session on a loaded worktree, which asks nothing more
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.progress_until_ready().await;
    let serving = registry
        .running()
        .await
        .expect("opening the session started the index daemon")
        .task_id
        .clone();

    // When the session stays open for well over the idle timeout, the idle reaper looking all along
    let stopped_while_open =
        what_the_reaper_stops_over(&registry, A_SHORT_IDLE_TIMEOUT * 5 / 2).await;
    // And the pane then closes it
    drop(session);
    let stopped_once_closed = what_the_reaper_stops_first(&registry).await;

    // Then the index daemon was not stopped while the session was open, and is once it closed
    assert_eq!(
        (stopped_while_open, stopped_once_closed),
        (vec![], Some(serving))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_open_code_session_keeps_its_loaded_root_in_use_by_warming_it_again() {
    // Given a pane's open code session on a loaded worktree, under a daemon whose keep-alive beat
    // is a third of a second
    let index = a_scripted_index();
    let (project, _registry, entry, _host) =
        a_pane_s_daemon_letting_its_index_daemon_go_after(&index, A_SHORT_IDLE_TIMEOUT).await;
    index.reports(the_index_is_ready());
    let mut session = a_code_session_opened_with(&entry, opening_its_worktree(&project)).await;
    session.progress_until_ready().await;

    // When the session stays open for two beats, its root never being let go
    let warms = index.the_first_warms(3, AN_EVENT_IS_DUE_WITHIN).await;

    // Then each beat warmed that worktree again — which is what the index daemon counts as using
    // its root — after the warm opening the session asked for
    let a_warm_of_the_worktree = index::WarmRequest {
        workspace_root: project.worktree_path.clone(),
    };
    assert_eq!(
        warms,
        vec![
            a_warm_of_the_worktree.clone(),
            a_warm_of_the_worktree.clone(),
            a_warm_of_the_worktree
        ]
    );
}
