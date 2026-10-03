//! Before every turn, the conversation takes in the caller's current files — and tells the model.
//!
//! Recording ports for the reset and the sync share one event log, each event noting how many model
//! requests the `wiremock` provider had seen at that moment, so the order reset → sync → first model
//! call is asserted directly.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-03-agent-worktree-caller-sync.md

use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    sync_notice, sync_refusal, CodebaseAccess, FileCounts, LineCounts, MessageId, ResetTarget,
    SubagentConfig, SubagentError, SubagentRegistry, SubagentSession, SyncAnswer, TurnRequest,
    WorktreeReset, WorktreeResetPort, WorktreeSync, WorktreeSyncPort,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `(port, model requests seen when it was asked)`, in the order asked.
type EventLog = Arc<Mutex<Vec<(&'static str, usize)>>>;

struct RecordingSyncs {
    events: EventLog,
    answer: SyncAnswer,
    provider: Arc<MockServer>,
}

#[async_trait]
impl WorktreeSyncPort for RecordingSyncs {
    async fn sync(&self) -> Result<SyncAnswer, SubagentError> {
        let seen = self.provider.received_requests().await.unwrap().len();
        self.events.lock().unwrap().push(("sync", seen));
        Ok(self.answer.clone())
    }
}

struct RecordingResets {
    events: EventLog,
    provider: Arc<MockServer>,
}

#[async_trait]
impl WorktreeResetPort for RecordingResets {
    async fn reset(&self, _target: ResetTarget) -> Result<Option<WorktreeReset>, SubagentError> {
        let seen = self.provider.received_requests().await.unwrap().len();
        self.events.lock().unwrap().push(("reset", seen));
        Ok(Some(WorktreeReset {
            to: "c1".to_string(),
            dropped_commits: vec![],
        }))
    }
}

fn a_def(base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "coder".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Write],
        max_turns: 4,
        replaces: Vec::new(),
        usage_notes: None,
    }
}

fn a_merge_of(paths: &[&str]) -> WorktreeSync {
    WorktreeSync {
        commit: "7d1e0aa".to_string(),
        files: FileCounts {
            created: 0,
            updated: paths.len() as u32,
            removed: 0,
        },
        lines: LineCounts {
            added: 3,
            removed: 1,
        },
        paths: paths.iter().map(|p| p.to_string()).collect(),
        more_paths: 0,
    }
}

/// The daemon's side: every write commits (`c1`, `c2`, …), every read answers one line; `reads`
/// counts the reads that actually reached the codebase.
fn a_codebase(reads: Arc<AtomicUsize>) -> CodebaseAccess {
    let writes = Arc::new(AtomicUsize::new(0));
    CodebaseAccess::managed(
        move |tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let answer = match tool.as_str() {
                "Read" => {
                    reads.fetch_add(1, Ordering::SeqCst);
                    json!({ "content": "fn a() {}", "truncated": false, "total_lines": 1 })
                }
                _ => {
                    let n = writes.fetch_add(1, Ordering::SeqCst) + 1;
                    json!({
                        "bytes_written": 2,
                        "worktreeChange": {
                            "commit": format!("c{n}"),
                            "files": { "created": 1, "updated": 0, "removed": 0 },
                            "lines": { "added": 1, "removed": 0 }
                        }
                    })
                }
            };
            Box::pin(async move { answer.to_string() })
        },
    )
}

fn a_call(tool: &str, args: serde_json::Value, call_id: &str) -> serde_json::Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Working.",
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": { "name": tool, "arguments": args.to_string() }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_final_answer() -> serde_json::Value {
    json!({
        "choices": [{
            "message": { "role": "assistant", "content": "done" },
            "finish_reason": "stop"
        }]
    })
}

/// A conversation whose provider answers `responses` in order, then final answers forever.
struct AConversation {
    session: Box<dyn SubagentSession>,
    events: EventLog,
    provider: Arc<MockServer>,
    reads: Arc<AtomicUsize>,
}

async fn a_conversation(
    responses: Vec<serde_json::Value>,
    sync_answer: SyncAnswer,
) -> AConversation {
    let provider = Arc::new(MockServer::start().await);
    for response in responses {
        Mock::given(method("POST"))
            .and(path_matcher("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .up_to_n_times(1)
            .mount(&provider)
            .await;
    }
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer()))
        .mount(&provider)
        .await;
    let events: EventLog = Arc::new(Mutex::new(Vec::new()));
    let reads = Arc::new(AtomicUsize::new(0));
    let session = SubagentRegistry::from_defs(vec![a_def(&provider.uri())])
        .create(
            "coder",
            SubagentConfig::new(a_codebase(Arc::clone(&reads)))
                .with_worktree_reset(Arc::new(RecordingResets {
                    events: Arc::clone(&events),
                    provider: Arc::clone(&provider),
                }))
                .with_worktree_sync(Arc::new(RecordingSyncs {
                    events: Arc::clone(&events),
                    answer: sync_answer,
                    provider: Arc::clone(&provider),
                })),
        )
        .expect("the def must resolve");
    AConversation {
        session,
        events,
        provider,
        reads,
    }
}

/// A conversation whose first turn wrote `a.txt` and finished, its ports answering `sync_answer`.
async fn a_conversation_after_one_turn(sync_answer: SyncAnswer) -> AConversation {
    let mut conversation = a_conversation(
        vec![a_call(
            "WRITE",
            json!({ "path": "a.txt", "contents": "a\n" }),
            "call_1",
        )],
        sync_answer,
    )
    .await;
    conversation
        .session
        .take_turn(TurnRequest::prompting("write a.txt"))
        .await
        .expect("the first turn runs");
    conversation.events.lock().unwrap().clear();
    conversation
}

impl AConversation {
    async fn requests_so_far(&self) -> usize {
        self.provider.received_requests().await.unwrap().len()
    }

    fn events(&self) -> Vec<(&'static str, usize)> {
        self.events.lock().unwrap().clone()
    }

    /// The messages of the model request at `index`.
    async fn messages_sent_in_request(&self, index: usize) -> Vec<serde_json::Value> {
        let requests = self.provider.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[index].body).unwrap();
        body["messages"].as_array().unwrap().clone()
    }
}

#[tokio::test]
async fn a_prompt_takes_in_the_callers_files_before_the_first_model_call() {
    // Given
    let mut conversation =
        a_conversation_after_one_turn(SyncAnswer::Merged(a_merge_of(&["src/lib.rs"]))).await;
    let before = conversation.requests_so_far().await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::prompting("now the tests"))
        .await
        .expect("the turn runs");

    // Then
    assert_eq!(conversation.events(), vec![("sync", before)]);
}

#[tokio::test]
async fn a_resume_takes_them_in_too() {
    // Given
    let mut conversation =
        a_conversation_after_one_turn(SyncAnswer::Merged(a_merge_of(&["src/lib.rs"]))).await;
    let before = conversation.requests_so_far().await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming())
        .await
        .expect("the turn runs");

    // Then
    assert_eq!(conversation.events(), vec![("sync", before)]);
}

#[tokio::test]
async fn a_rewind_resets_the_worktree_before_it_syncs() {
    // Given
    let mut conversation =
        a_conversation_after_one_turn(SyncAnswer::Merged(a_merge_of(&["src/lib.rs"]))).await;
    // The def has no system prompt, so the first turn's prompt is the conversation's first message
    let the_first_prompt = MessageId::from("m1".to_string());
    let before = conversation.requests_so_far().await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(the_first_prompt))
        .await
        .expect("the turn runs");

    // Then
    assert_eq!(
        conversation.events(),
        vec![("reset", before), ("sync", before)]
    );
}

/// Guards the opt-out: `syncWorktree: false` asks for no sync.
#[tokio::test]
async fn a_turn_without_sync_asks_for_none() {
    // Given
    let mut conversation =
        a_conversation_after_one_turn(SyncAnswer::Merged(a_merge_of(&["src/lib.rs"]))).await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::prompting("go on").without_sync())
        .await
        .expect("the turn runs");

    // Then
    assert_eq!(conversation.events(), vec![]);
}

#[tokio::test]
async fn a_merge_is_announced_last_before_the_turn_runs() {
    // Given
    let merge = a_merge_of(&["src/lib.rs", "README.md"]);
    let mut conversation = a_conversation_after_one_turn(SyncAnswer::Merged(merge.clone())).await;
    let next_request = conversation.requests_so_far().await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::prompting("now the tests"))
        .await
        .expect("the turn runs");

    // Then
    let sent = conversation.messages_sent_in_request(next_request).await;
    let last_two: Vec<(serde_json::Value, serde_json::Value)> = sent[sent.len() - 2..]
        .iter()
        .map(|message| (message["role"].clone(), message["content"].clone()))
        .collect();
    assert_eq!(
        last_two,
        vec![
            (json!("user"), json!("now the tests")),
            (json!("user"), json!(sync_notice(&merge))),
        ]
    );
}

/// Guards the quiet case: nothing merged, nothing announced.
#[tokio::test]
async fn nothing_merged_announces_nothing() {
    // Given
    let mut conversation = a_conversation_after_one_turn(SyncAnswer::Nothing).await;
    let next_request = conversation.requests_so_far().await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::prompting("now the tests"))
        .await
        .expect("the turn runs");

    // Then
    let sent = conversation.messages_sent_in_request(next_request).await;
    assert_eq!(sent.last().unwrap()["content"], json!("now the tests"));
}

#[tokio::test]
async fn the_outcome_reports_the_merge() {
    // Given
    let merge = a_merge_of(&["src/lib.rs"]);
    let mut conversation = a_conversation_after_one_turn(SyncAnswer::Merged(merge.clone())).await;

    // When
    let outcome = conversation
        .session
        .take_turn(TurnRequest::prompting("now the tests"))
        .await
        .expect("the turn runs");

    // Then
    assert_eq!(outcome.worktree_sync, Some(merge));
}

#[tokio::test]
async fn a_conflict_refuses_the_turn_before_any_model_call_naming_the_paths() {
    // Given
    let conflicted = vec!["src/lib.rs".to_string(), "README.md".to_string()];
    let mut conversation =
        a_conversation_after_one_turn(SyncAnswer::Conflicted(conflicted.clone())).await;
    let before = conversation.requests_so_far().await;

    // When
    let refused = conversation
        .session
        .take_turn(TurnRequest::prompting("now the tests"))
        .await;

    // Then
    assert_eq!(
        (
            refused.map(|_| ()).map_err(|e| e.to_string()),
            conversation.requests_so_far().await
        ),
        (Err(sync_refusal(&conflicted).to_string()), before)
    );
}

#[tokio::test]
async fn a_merge_lets_the_subagent_read_a_file_again() {
    // Given — two identical reads in the first turn; a third would be refused as a repeat unless
    // the merge cleared what the conversation remembers having read
    let read = || json!({ "path": "src/lib.rs" });
    let mut conversation = a_conversation(
        vec![
            a_call("READ", read(), "call_1"),
            a_call("READ", read(), "call_2"),
            a_final_answer(),
            a_call("READ", read(), "call_3"),
        ],
        SyncAnswer::Merged(a_merge_of(&["src/lib.rs"])),
    )
    .await;
    conversation
        .session
        .take_turn(TurnRequest::prompting("read it").without_sync())
        .await
        .expect("the first turn runs");

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming())
        .await
        .expect("the resumed turn runs");

    // Then
    assert_eq!(conversation.reads.load(Ordering::SeqCst), 3);
}
