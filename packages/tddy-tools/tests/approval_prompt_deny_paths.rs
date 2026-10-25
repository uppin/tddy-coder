//! Approval prompts a headless conversation raises are denied, with the reason, whenever the daemon
//! cannot give an allow: no remote route, a failed call, or a refusal.
//!
//! `PermissionServer` reads `TDDY_CONVERSATION_ID` and the `TDDY_REMOTE_*` route when it is
//! constructed, and both are process-wide, so each test runs serially and clears the route first.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};
use serial_test::serial;
use tddy_tools::server::PermissionServer;

/// Every variable a remote route is configured through, cleared before a test sets its own.
const ROUTE_VARS: [&str; 10] = [
    "TDDY_SANDBOX_TOOL_IPC",
    "TDDY_REMOTE_SESSION_ID",
    "TDDY_REMOTE_SESSION_TOKEN",
    "TDDY_REMOTE_DAEMON_URL",
    "TDDY_REMOTE_DAEMON_SOCKET",
    "TDDY_REMOTE_DAEMON_INSTANCE_ID",
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
    "TDDY_REMOTE_SERVER_IDENTITY",
];

/// Clear every route variable, then set the conversation the approval belongs to.
fn a_conversation_without_a_route() {
    for var in ROUTE_VARS {
        std::env::remove_var(var);
    }
    std::env::remove_var("TDDY_SOCKET");
    std::env::set_var("TDDY_CONVERSATION_ID", "conv-1");
}

/// The denial reply's message, after checking it is a denial at all.
fn denial_message(reply: &str) -> String {
    let reply: Value = serde_json::from_str(reply).expect("the reply is JSON");
    assert_eq!(reply["behavior"], "deny", "the approval is denied: {reply}");
    reply["message"]
        .as_str()
        .expect("a denial carries its reason")
        .to_string()
}

/// The address of a local port nothing listens on: bound, then released.
fn a_closed_local_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let address = listener.local_addr().expect("the port's address");
    drop(listener);
    format!("http://{address}")
}

/// A daemon that answers the first request it receives with `status` and `body`, then closes.
fn a_daemon_refusing_once(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let url = format!(
        "http://{}",
        listener.local_addr().expect("the port's address")
    );
    let body = body.to_string();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        read_one_request(&mut stream);
        let response = format!(
            "HTTP/1.1 {status} Refused\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    });
    url
}

/// Read a request's headers and its declared body, so the answer is not sent before it is read.
fn read_one_request(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    let mut received = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            return;
        }
        received.extend_from_slice(&chunk[..read]);
        let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&received[..end]).to_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while received.len() < end + 4 + length {
            let read = stream.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                return;
            }
            received.extend_from_slice(&chunk[..read]);
        }
        return;
    }
}

#[test]
#[serial]
fn a_tagged_approval_with_no_remote_route_is_denied_naming_the_missing_route() {
    // Given: the conversation's approval has no remote route to reach the daemon through
    a_conversation_without_a_route();
    let server = PermissionServer::new();

    // When: the conversation's claude asks permission to run a Bash command
    let reply = server.decide_for_approval("Bash", &json!({"command": "rm -rf /tmp/x"}));

    // Then: the approval is denied, and the message names the route that is missing
    let message = denial_message(&reply);
    assert!(
        message.contains("TDDY_REMOTE_SESSION_ID"),
        "the denial names the missing route: {message}"
    );
}

#[test]
#[serial]
fn a_tagged_approval_whose_transport_fails_is_denied_with_that_reason() {
    // Given: the daemon's address is set, but nothing listens on it
    a_conversation_without_a_route();
    std::env::set_var("TDDY_REMOTE_SESSION_ID", "session-1");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", a_closed_local_port());
    let server = PermissionServer::new();

    // When: the conversation's claude asks permission to run a Bash command
    let reply = server.decide_for_approval("Bash", &json!({"command": "rm -rf /tmp/x"}));

    // Then: the approval is denied, and the denial says the call could not be made
    let message = denial_message(&reply);
    assert!(
        message.starts_with("relay connection error"),
        "the denial carries the transport's failure: {message}"
    );
}

#[test]
#[serial]
fn a_tagged_approval_the_daemon_refuses_is_denied_with_the_refusal() {
    // Given: the daemon refuses the approval, because the caller does not own the conversation
    a_conversation_without_a_route();
    let refusal = "conversation 'conv-1' does not belong to the session presenting this credential";
    let daemon = a_daemon_refusing_once(
        403,
        &json!({"code": "permission_denied", "message": refusal}).to_string(),
    );
    std::env::set_var("TDDY_REMOTE_SESSION_ID", "session-1");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", daemon);
    let server = PermissionServer::new();

    // When: the conversation's claude asks permission to run a Bash command
    let reply = server.decide_for_approval("Bash", &json!({"command": "rm -rf /tmp/x"}));

    // Then: the approval is denied, carrying the daemon's refusal
    assert_eq!(denial_message(&reply), refusal);
}
