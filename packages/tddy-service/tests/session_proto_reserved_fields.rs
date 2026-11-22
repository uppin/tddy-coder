//! `#workqueue` 1/14: `StartSessionRequest.pr_stack_base_session_id` (field 31) seeded a stack from
//! an existing session's branch for the removed `pr-stack` recipe. Its number is reserved, never
//! reused, so an old client's field 31 can never be read as something else.

use std::path::Path;

fn message_block<'a>(proto: &'a str, message: &str) -> &'a str {
    let header = format!("message {message} {{");
    let start = proto
        .find(&header)
        .unwrap_or_else(|| panic!("session.proto declares {message}"));
    let body = &proto[start + header.len()..];
    let mut depth = 1usize;
    for (at, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &body[..at];
                }
            }
            _ => {}
        }
    }
    panic!("{message} is closed");
}

#[test]
fn start_session_request_reserves_the_removed_stack_seed_field() {
    // Given
    let proto =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/session.proto"))
            .expect("session.proto");

    // When
    let request = message_block(&proto, "StartSessionRequest");

    // Then
    assert!(
        !request.contains("pr_stack_base_session_id"),
        "StartSessionRequest still declares pr_stack_base_session_id"
    );
    assert!(
        request
            .lines()
            .any(|line| line.trim_start().starts_with("reserved") && line.contains("31")),
        "StartSessionRequest reserves field 31"
    );
}
