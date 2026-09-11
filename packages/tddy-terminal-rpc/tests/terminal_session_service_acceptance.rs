//! Acceptance tests for `terminal_session.TerminalSessionService` as this crate now serves it.
//!
//! Every test dispatches at the *registered coordinate* —
//! `entry.service.handle_rpc("terminal_session.TerminalSessionService", "<Method>", …)` with an
//! encoded request, decoding the answer back — rather than calling the handler behind it. The proto
//! was extracted into this crate and then never served: nine methods that compile prove nothing
//! about whether a client can reach any of them.

mod support;

use support::{
    a_history_request, a_start_request, a_watch_request, an_input, an_output_request,
    TerminalServiceHost, SCREEN_ID, SESSION_ID, SESSION_TOKEN,
};
use tddy_terminal_rpc::proto::terminal_session::{
    ClaimTerminalControlRequest, ClaimTerminalControlResponse, ListTerminalSessionsRequest,
    ListTerminalSessionsResponse, SendTerminalInputResponse, SessionTerminalOutput,
    StartTerminalSessionResponse, StopTerminalSessionRequest, StopTerminalSessionResponse,
    TerminalControlEvent, TerminalHistoryChunk,
};

#[test]
fn names_the_service_the_wiring_layer_registers() {
    // Given the entry a host's wiring layer builds
    let host = TerminalServiceHost::serving_main_terminal(b"");

    // When reading the coordinate it is registered at
    // Then it is the proto's own service name
    assert_eq!(
        host.service_name(),
        "terminal_session.TerminalSessionService"
    );
}

/// The name a host registers and the name a caller dials have to be one value: a mismatch is not a
/// type error but a runtime "unknown service" on the serving side. This pins the served entry to
/// the constant every caller addresses — `pty_relay`'s three Connect-HTTP dials, the coder's
/// wrapper, and both test harnesses — so a rename that reaches only one end fails here.
#[test]
fn registers_at_the_coordinate_every_caller_addresses() {
    // Given the entry a host's wiring layer builds
    let host = TerminalServiceHost::serving_main_terminal(b"");

    // When reading the coordinate it is registered at
    // Then it is the constant this crate publishes for its callers
    assert_eq!(
        host.service_name(),
        tddy_terminal_rpc::TERMINAL_SESSION_SERVICE
    );
}

/// And that constant is what the schema declares, which is what clients in other languages are
/// generated against — the one part of the coordinate the shared constant cannot keep honest.
#[test]
fn publishes_the_coordinate_its_schema_declares() {
    // Given the schema the nine methods are generated from
    let proto = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/terminal_session.proto"),
    )
    .expect("terminal_session.proto is readable");

    // When reading the package and service it declares
    let package = proto
        .lines()
        .find_map(|line| line.strip_prefix("package ")?.strip_suffix(';'))
        .expect("the proto declares a package");
    let service = proto
        .lines()
        .find_map(|line| line.strip_prefix("service ")?.strip_suffix(" {"))
        .expect("the proto declares a service");

    // Then together they are the coordinate this crate publishes
    assert_eq!(
        format!("{package}.{service}"),
        tddy_terminal_rpc::TERMINAL_SESSION_SERVICE
    );
}

// ---------------------------------------------------------------------------
// The four streaming / input methods
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_session_terminal_io_answers_at_the_registered_service() {
    // Given a session whose main terminal has produced ten bytes
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");

    // When a bidi client opens the stream with its first message
    let mut session = host.open_bidi(an_input()).await;

    // Then the output half replays the current last frame, anchored at its absolute offsets
    let frames = session.frames(1).await;
    assert_eq!(
        frames,
        vec![SessionTerminalOutput {
            data: b"6789".to_vec(),
            acked_input_offset: 0,
            start_offset: 6,
            end_offset: 10,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
        }]
    );
}

#[tokio::test]
async fn stream_terminal_output_answers_at_the_registered_service() {
    // Given a session whose main terminal has produced ten bytes
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");

    // When a browser client opens the output half
    let frames: Vec<SessionTerminalOutput> = host
        .stream_frames("StreamTerminalOutput", an_output_request(), 1)
        .await;

    // Then it is handed the current last frame, anchored at its absolute offsets
    assert_eq!(
        frames,
        vec![SessionTerminalOutput {
            data: b"6789".to_vec(),
            acked_input_offset: 0,
            start_offset: 6,
            end_offset: 10,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
        }]
    );
}

#[tokio::test]
async fn send_terminal_input_answers_at_the_registered_service() {
    // Given a session with a live main terminal
    let host = TerminalServiceHost::serving_main_terminal(b"");
    let typed = support::typing(b"ls -la", 6);

    // When the browser client sends a chunk of input
    let response: SendTerminalInputResponse = host.answer("SendTerminalInput", typed).await;

    // Then the input reached the PTY with its cumulative offset
    assert_eq!(response, SendTerminalInputResponse {});
    assert_eq!(
        host.terminal
            .as_ref()
            .expect("a live terminal")
            .inputs
            .lock()
            .unwrap()
            .clone(),
        vec![(bytes::Bytes::from_static(b"ls -la"), 6)]
    );
}

#[tokio::test]
async fn get_terminal_history_answers_at_the_registered_service() {
    // Given a session whose main terminal has produced ten bytes
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");

    // When the client fills history forward from the oldest retained byte
    let chunks: Vec<TerminalHistoryChunk> = host
        .all_frames("GetTerminalHistory", a_history_request())
        .await;

    // Then it is handed the first bounded chunk, tagged as the bottom of the retained history
    assert_eq!(
        chunks,
        vec![TerminalHistoryChunk {
            data: b"0123".to_vec(),
            start_offset: 0,
            end_offset: 4,
            at_oldest: true,
            at_end: false,
        }]
    );
}

// ---------------------------------------------------------------------------
// The three lifecycle methods
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_terminal_session_answers_at_the_registered_service() {
    // Given a session with a running main terminal
    let host = TerminalServiceHost::serving_main_terminal(b"");

    // When the client starts a shell terminal
    let response: StartTerminalSessionResponse =
        host.answer("StartTerminalSession", a_start_request()).await;

    // Then it is handed the new terminal's id
    assert_eq!(
        response,
        StartTerminalSessionResponse {
            terminal_id: "bash-1".to_string(),
        }
    );
}

#[tokio::test]
async fn stop_terminal_session_answers_at_the_registered_service() {
    // Given a session with a started shell terminal
    let host = TerminalServiceHost::serving_main_terminal(b"");
    let started: StartTerminalSessionResponse =
        host.answer("StartTerminalSession", a_start_request()).await;

    // When the client stops it
    let response: StopTerminalSessionResponse = host
        .answer(
            "StopTerminalSession",
            StopTerminalSessionRequest {
                session_token: SESSION_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                terminal_id: started.terminal_id,
                control_token: String::new(),
            },
        )
        .await;

    // Then the stop is acknowledged
    assert_eq!(
        response,
        StopTerminalSessionResponse {
            ok: true,
            message: String::new(),
        }
    );
}

#[tokio::test]
async fn list_terminal_sessions_answers_at_the_registered_service() {
    // Given a session holding its main terminal and one started shell
    let host = TerminalServiceHost::serving_main_terminal(b"");
    host.answer::<_, StartTerminalSessionResponse>("StartTerminalSession", a_start_request())
        .await;

    // When the client lists the session's terminals
    let response: ListTerminalSessionsResponse = host
        .answer(
            "ListTerminalSessions",
            ListTerminalSessionsRequest {
                session_token: SESSION_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
            },
        )
        .await;

    // Then both are reported, each with its kind and pid
    assert_eq!(
        response
            .terminals
            .iter()
            .map(|terminal| (
                terminal.terminal_id.as_str(),
                terminal.kind.as_str(),
                terminal.pid
            ))
            .collect::<Vec<_>>(),
        vec![("main", "claude-cli", 4242), ("bash-1", "bash", 5001)]
    );
}

// ---------------------------------------------------------------------------
// The control mutex
// ---------------------------------------------------------------------------

#[tokio::test]
async fn claim_terminal_control_answers_at_the_registered_service() {
    // Given a session whose terminal control is unheld
    let host = TerminalServiceHost::serving_main_terminal(b"");

    // When a screen claims control
    let response: ClaimTerminalControlResponse = host
        .answer(
            "ClaimTerminalControl",
            ClaimTerminalControlRequest {
                session_token: SESSION_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                screen_id: SCREEN_ID.to_string(),
                steal: false,
            },
        )
        .await;

    // Then it is granted the lease and handed the token to present on subsequent control RPCs
    assert_eq!(
        response,
        ClaimTerminalControlResponse {
            granted: true,
            control_token: "control-token-1".to_string(),
            current_holder_screen_id: String::new(),
        }
    );
}

#[tokio::test]
async fn watch_terminal_control_answers_at_the_registered_service() {
    // Given a session whose control lease is held by one screen
    let host = TerminalServiceHost::serving_main_terminal(b"");
    let claim: ClaimTerminalControlResponse = host
        .answer(
            "ClaimTerminalControl",
            ClaimTerminalControlRequest {
                session_token: SESSION_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                screen_id: SCREEN_ID.to_string(),
                steal: false,
            },
        )
        .await;

    // When that screen subscribes to ownership changes
    let events: Vec<TerminalControlEvent> = host
        .stream_frames(
            "WatchTerminalControl",
            a_watch_request(&claim.control_token),
            1,
        )
        .await;

    // Then the snapshot names it as the current controller
    assert_eq!(
        events,
        vec![TerminalControlEvent {
            holder_screen_id: SCREEN_ID.to_string(),
            you_are_controller: true,
        }]
    );
}

// ---------------------------------------------------------------------------
// The auth gate in front of all nine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_a_terminal_stream_whose_session_token_is_not_this_hosts() {
    // Given a session with a live main terminal
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");
    let mut request = an_output_request();
    request.session_token = "a-token-from-another-host".to_string();

    // When a caller opens the output half with a token this host does not know
    let status = host.refusal("StreamTerminalOutput", request).await;

    // Then it is told to re-authenticate, and never reaches the terminal
    assert_eq!(status.code(), tddy_rpc::Code::Unauthenticated);
}

#[tokio::test]
async fn refuses_a_terminal_stream_whose_identity_maps_to_no_os_user() {
    // Given a host that authenticates the token but has no OS user for the identity behind it
    let host = TerminalServiceHost::serving_main_terminal_for_unmapped_user(b"0123456789");

    // When that caller opens the output half
    let status = host
        .refusal("StreamTerminalOutput", an_output_request())
        .await;

    // Then it is a permission problem only an operator can fix, not a session to re-authenticate
    assert_eq!(status.code(), tddy_rpc::Code::PermissionDenied);
}

#[tokio::test]
async fn refuses_a_terminal_stream_for_a_session_with_no_live_terminal() {
    // Given a session whose terminal is not running
    let host = TerminalServiceHost::serving_no_terminal();

    // When the client opens the output half
    let status = host
        .refusal("StreamTerminalOutput", an_output_request())
        .await;

    // Then it is told the terminal is not there
    assert_eq!(status.code(), tddy_rpc::Code::NotFound);
}

#[tokio::test]
async fn refuses_to_stop_the_main_terminal_through_the_terminal_lifecycle() {
    // Given a session whose only terminal is the reserved main one
    let host = TerminalServiceHost::serving_main_terminal(b"");

    // When the client tries to stop it as if it were a started shell
    let status = host
        .refusal(
            "StopTerminalSession",
            StopTerminalSessionRequest {
                session_token: SESSION_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                terminal_id: "main".to_string(),
                control_token: String::new(),
            },
        )
        .await;

    // Then it is refused as the wrong request, not answered as a stop that did nothing: ending a
    // session's agent is its own RPC.
    assert_eq!(status.code(), tddy_rpc::Code::InvalidArgument);
}
