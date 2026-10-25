//! Following a session is paged, and only paged: every answer is bounded, and a poll from the last
//! cursor returns exactly what is new (`#start-parity` 6/9).

use pretty_assertions::assert_eq;
use tddy_core::agent_activity::AgentActivityRecord;
use tddy_tool_engine::session_follow::{
    page_activity, session_follow_tool_defs, truncate_transcript_text, FOLLOW_PAGE_DEFAULT,
    FOLLOW_PAGE_MAX, READ_SESSION_ACTIVITY, READ_SESSION_TRANSCRIPT, TRANSCRIPT_ITEM_MAX_BYTES,
};

fn a_call(seq: u64, status: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: format!("call-{seq}"),
        tool_name: "Bash".to_string(),
        input: serde_json::json!({ "command": format!("echo {seq}") }),
        status: status.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: 1_000 + seq,
        completed_unix_ms: 2_000 + seq,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: seq,
        changed_paths: Vec::new(),
    }
}

fn calls(seqs: std::ops::RangeInclusive<u64>) -> Vec<AgentActivityRecord> {
    seqs.map(|seq| a_call(seq, "completed")).collect()
}

fn seqs(page: &[AgentActivityRecord]) -> Vec<u64> {
    page.iter().map(|r| r.activity_seq).collect()
}

#[test]
fn activity_pages_forward_without_repeats_or_gaps() {
    // Given 250 calls
    let records = calls(1..=250);

    // When read 100 at a time
    let first = page_activity(&records, "", Some(100), &[]).unwrap();
    let second = page_activity(&records, &first.next_cursor, Some(100), &[]).unwrap();
    let third = page_activity(&records, &second.next_cursor, Some(100), &[]).unwrap();

    // Then the pages are 100, 100 and 50, and together are every call once, in order
    assert_eq!(
        (first.items.len(), second.items.len(), third.items.len()),
        (100, 100, 50)
    );
    assert!(first.has_more && second.has_more && !third.has_more);
    let together: Vec<u64> = [first.items, second.items, third.items]
        .concat()
        .iter()
        .map(|r| r.activity_seq)
        .collect();
    assert_eq!(together, (1..=250).collect::<Vec<_>>());
}

#[test]
fn a_poll_from_the_last_cursor_returns_only_what_is_new() {
    // Given everything read so far
    let mut records = calls(1..=10);
    let read = page_activity(&records, "", None, &[]).unwrap();
    assert!(!read.has_more);

    // When three more calls happen and the follower polls from where it stopped
    records.extend(calls(11..=13));
    let poll = page_activity(&records, &read.next_cursor, None, &[]).unwrap();

    // Then it gets exactly those three
    assert_eq!(seqs(&poll.items), vec![11, 12, 13]);
}

#[test]
fn there_is_no_unbounded_read() {
    // Given more calls than the largest page
    let records = calls(1..=(FOLLOW_PAGE_MAX as u64 + 50));

    // When read with no limit, and with an enormous one
    let no_limit = page_activity(&records, "", None, &[]).unwrap();
    let enormous = page_activity(&records, "", Some(1_000_000), &[]).unwrap();

    // Then the first is the default page and the second is clamped to the maximum, saying so
    assert_eq!(no_limit.items.len(), FOLLOW_PAGE_DEFAULT);
    assert!(no_limit.has_more);
    assert_eq!(enormous.items.len(), FOLLOW_PAGE_MAX);
    assert_eq!(enormous.clamped_to, Some(FOLLOW_PAGE_MAX));
}

#[test]
fn a_status_filter_keeps_only_those_calls_and_still_pages_by_sequence() {
    // Given calls of mixed outcome
    let records = vec![
        a_call(1, "completed"),
        a_call(2, "error"),
        a_call(3, "running"),
        a_call(4, "error"),
    ];

    // When only errors are asked for
    let errors = page_activity(&records, "", None, &["error".to_string()]).unwrap();

    // Then
    assert_eq!(seqs(&errors.items), vec![2, 4]);
}

#[test]
fn a_cursor_that_is_not_a_position_is_refused() {
    // When
    let refused = page_activity(&calls(1..=3), "yesterday", None, &[]);

    // Then
    assert!(refused.unwrap_err().contains("cursor"));
}

#[test]
fn a_long_transcript_item_is_cut_on_a_character_boundary_and_marked() {
    // Given text longer than the limit, ending in multi-byte characters across it
    let long = format!(
        "{}{}",
        "a".repeat(TRANSCRIPT_ITEM_MAX_BYTES - 1),
        "é".repeat(10)
    );

    // When
    let (cut, truncated) = truncate_transcript_text(&long);

    // Then
    assert!(truncated);
    assert!(cut.len() <= TRANSCRIPT_ITEM_MAX_BYTES);
    assert!(long.starts_with(&cut));
}

#[test]
fn a_short_transcript_item_is_returned_whole() {
    assert_eq!(
        truncate_transcript_text("all of it"),
        ("all of it".to_string(), false)
    );
}

#[test]
fn both_tools_state_the_paging_contract_in_their_schemas() {
    // When
    let defs = session_follow_tool_defs();

    // Then
    for name in [READ_SESSION_ACTIVITY, READ_SESSION_TRANSCRIPT] {
        let def = defs
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} is defined"));
        let schema: serde_json::Value = serde_json::from_str(&def.input_schema_json).unwrap();
        for argument in ["session_id", "daemon_instance_id", "cursor", "limit"] {
            assert!(
                !schema["properties"][argument]["description"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty(),
                "{name}.{argument} is not described"
            );
        }
        assert_eq!(
            schema["properties"]["limit"]["maximum"],
            serde_json::json!(FOLLOW_PAGE_MAX)
        );
        assert_eq!(schema["required"], serde_json::json!(["session_id"]));
    }
}
