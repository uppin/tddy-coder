//! What `code_navigation.CodeSession` owes the web's code pane: one bidirectional stream per open
//! pane, authorised like every navigation request, which **warms** the pane's worktree, streams its
//! index's progress for as long as it is open, and answers the pane's queries — **holding** a hover,
//! definition or references asked while the index loads until it is ready, never holding a probe,
//! and dropping a held query the pane cancels.
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

use prost::Message;
use support::{
    a_checkout_outside_the_project, a_project_with_a_session_on_its_worktree, a_scripted_index,
    an_index_daemon_host_serving, canonical, indexing_at, the_call_to_foo, the_index_is_ready,
    the_web_s_location_of_foo, the_web_sees_indexing_at, the_web_sees_ready, AProjectWithASession,
    AScriptedIndex, AN_EVENT_IS_DUE_WITHIN, TEST_TOKEN, THE_INDEX_S_HOVER_OF_FOO,
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
    CodeQueryKind, CodeSessionEvent, CodeSessionRequest, Definability, HoverResponse,
    OpenCodeSession, ProbeAnswer,
};
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
    CodeSessionRequest {
        frame: Some(Frame::Open(OpenCodeSession {
            session_token: TEST_TOKEN.to_string(),
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
    CodeSessionRequest {
        frame: Some(Frame::Query(CodeQuery {
            request_id,
            kind: kind as i32,
            rel_path: "src/main.rs".to_string(),
            position: Some(the_call_to_foo()),
        })),
    }
}

fn a_hover(request_id: u64) -> CodeSessionRequest {
    a_query(request_id, CodeQueryKind::Hover)
}

fn a_probe(request_id: u64) -> CodeSessionRequest {
    a_query(request_id, CodeQueryKind::Probe)
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
        position: Some(index::SourcePosition { line: 2, column: 5 }),
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
    let project = a_project_with_a_session_on_its_worktree();
    let host = an_index_daemon_host_serving(index.clone()).await;
    let registry = host.registry();
    let entry = the_entry_for(&project, registry.clone());
    (project, registry, entry, host)
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
    // And the index was asked twice: once while loading, and again once ready
    assert_eq!(
        index.hovers_it_was_asked(),
        vec![
            the_hover_of_foo_asked_in(&project),
            the_hover_of_foo_asked_in(&project)
        ]
    );
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

    // Then the session ends as an invalid argument, and nothing was warmed or started
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert_eq!(index.warms_it_was_asked_for(), vec![]);
    assert!(registry.running().await.is_none());
}
