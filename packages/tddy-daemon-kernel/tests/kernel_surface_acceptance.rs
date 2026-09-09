//! The kernel's surface as a *consumer* sees it.
//!
//! An integration test is compiled as its own crate, so everything below is reached exactly the way
//! `tddy-host-service` and `tddy-worktree-service` will reach it: through `pub` items only, from a
//! crate that has never heard of `tddy-daemon`. That is the whole point of the file. The inline
//! `#[cfg(test)] mod tests` in `src/lib.rs` can see private items and would keep passing if the
//! surface were too narrow to build anything with — these cannot.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_RUNNING};
use tddy_daemon_kernel::{
    now_unix_ms, trim_to_option, AgentActivityHub, SessionUserResolver, SessionsBaseResolver,
    HOST_DOCUMENT_FRAME_BYTES,
};

/// The extraction is only real if these symbols are not re-exports. A kernel that reached back into
/// `tddy-daemon` would compile just as happily and prove nothing — every subsystem crate importing
/// from it would still be dragging the 23,099-line RPC entry point along transitively — so the
/// absence of that edge is asserted rather than assumed.
#[test]
fn declares_no_dependency_on_the_daemon_it_was_extracted_from() {
    // Given
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("the kernel's own manifest is readable");

    // When
    let dependencies = dependency_names(&manifest);

    // Then
    assert!(
        !dependencies.iter().any(|name| name == "tddy-daemon"),
        "the kernel declares {dependencies:?}; a `tddy-daemon` edge would make every symbol here a \
         re-export and leave the subsystems it exists to free still bound to the daemon"
    );
}

#[test]
fn authenticates_a_session_through_a_user_resolver_a_consumer_built_itself() {
    // Given — the shape `auth::build_auth_entries` hands every service
    let resolve_user: SessionUserResolver =
        Arc::new(|token| (token == "a-live-token").then(|| "octocat".to_string()));

    // When
    let owner = (resolve_user)("a-live-token");

    // Then
    assert_eq!(owner.as_deref(), Some("octocat"));
}

#[test]
fn has_no_owner_for_a_token_the_user_resolver_does_not_know() {
    // Given
    let resolve_user: SessionUserResolver =
        Arc::new(|token| (token == "a-live-token").then(|| "octocat".to_string()));

    // When
    let owner = (resolve_user)("an-expired-token");

    // Then
    assert_eq!(owner, None);
}

#[test]
fn locates_a_users_sessions_through_a_base_resolver_a_consumer_built_itself() {
    // Given
    let resolve_sessions_base: SessionsBaseResolver =
        Arc::new(|os_user| Some(PathBuf::from("/srv/tddy").join(os_user)));

    // When
    let base = (resolve_sessions_base)("octocat");

    // Then
    assert_eq!(base, Some(PathBuf::from("/srv/tddy/octocat")));
}

/// The sandbox subsystem holds an `Arc` of the hub and publishes into it from spawned work while the
/// activity handlers subscribe elsewhere. Both halves have to be reachable from outside the daemon,
/// or the subsystem cannot leave it.
#[test]
fn delivers_a_record_published_through_one_arc_to_a_subscriber_holding_another() {
    // Given
    let hub = Arc::new(AgentActivityHub::default());
    let mut activity = hub.subscribe("session-a");

    // When — a second handle publishes, as a spawned publisher would
    Arc::clone(&hub).publish("session-a", a_running_record("Read"));

    // Then
    let seen = activity
        .try_recv()
        .expect("the subscriber sees what the other handle published");
    assert_eq!(seen.tool_name, "Read");
}

#[test]
fn pairs_a_tool_calls_start_with_its_terminal_row_through_the_pending_stack() {
    // Given a call whose `PreToolUse` has been seen but whose `PostToolUse` has not
    let hub = AgentActivityHub::default();
    hub.push_pending("session-a", "call-1");

    // When the terminal row arrives
    let paired = hub.pop_pending("session-a");

    // Then
    assert_eq!(paired.as_deref(), Some("call-1"));
}

/// Stamped once when the call starts and again when it ends, so the two must be ordered. The
/// implementation this consolidates truncated with a bare `as u64`, which turns a far-future clock
/// into a timestamp in the *past* — a completed row that predates its own start.
#[test]
fn stamps_a_calls_completion_no_earlier_than_its_start() {
    // Given
    let started_unix_ms = now_unix_ms();

    // When
    let completed_unix_ms = now_unix_ms();

    // Then
    assert!(
        completed_unix_ms >= started_unix_ms,
        "a call that completed at {completed_unix_ms} cannot have started at {started_unix_ms}"
    );
}

#[test]
fn frames_a_document_a_consumer_reads_at_the_shared_frame_size() {
    // Given a document just over two frames
    let document = vec![0_u8; HOST_DOCUMENT_FRAME_BYTES * 2 + 7];

    // When the consumer frames it the way both document readers do
    let frame_sizes: Vec<usize> = document
        .chunks(HOST_DOCUMENT_FRAME_BYTES)
        .map(<[u8]>::len)
        .collect();

    // Then
    assert_eq!(
        frame_sizes,
        vec![HOST_DOCUMENT_FRAME_BYTES, HOST_DOCUMENT_FRAME_BYTES, 7]
    );
}

#[test]
fn reads_a_request_field_a_client_left_blank_as_absent() {
    // Given the branch a client sent as whitespace rather than omitting
    // When
    let branch = trim_to_option("   ");

    // Then
    assert_eq!(branch, None);
}

#[test]
fn reads_a_request_field_with_content_as_its_trimmed_value() {
    // Given
    // When
    let branch = trim_to_option("  main  ");

    // Then
    assert_eq!(branch, Some("main".to_string()));
}

/// Every key in the manifest's `[dependencies]` and `[dev-dependencies]` tables.
///
/// Hand-rolled rather than pulling in a TOML parser: the only shape it has to understand is
/// `name = …` under a `[…dependencies]` header, and adding a dependency to check a crate's
/// dependency list would be its own small joke.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_dependencies = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_dependencies = line.trim_matches(['[', ']']).ends_with("dependencies");
            continue;
        }
        if in_dependencies {
            if let Some((name, _)) = line.split_once('=') {
                names.push(name.trim().to_string());
            }
        }
    }
    names
}

/// A record with only the tool name set to something meaningful. Every other field is at its zero
/// value, which is what `AgentActivityRecord` means by "not yet known".
fn a_running_record(tool: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: format!("call-for-{tool}"),
        tool_name: tool.to_string(),
        input: serde_json::Value::Null,
        status: STATUS_RUNNING.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: 0,
        completed_unix_ms: 0,
        source: "sandbox".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}
