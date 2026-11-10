//! Acceptance: the Codex usage binding shipped in `codex.yaml`, evaluated by the agent-agnostic
//! engine over rollout files shaped the way Codex CLI 0.167 writes them (`#usage-binding 4/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-codex-rollout.md

mod support;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pretty_assertions::assert_eq;
use support::{an_agent_home, the_builtin_binding, the_row, the_tree, AgentHome};
use tddy_usage::{account, UsageContext};

const WORKTREE: &str = "/work/repo";

/// One thread's cumulative `total_token_usage`, as a `token_count` event reports it.
#[derive(Debug, Clone, Copy)]
struct Totals {
    input: u64,
    cached: u64,
    output: u64,
    reasoning: u64,
}

fn totals(input: u64, cached: u64, output: u64, reasoning: u64) -> Totals {
    Totals {
        input,
        cached,
        output,
        reasoning,
    }
}

/// A rollout file under `<home>/.codex/sessions/2026/11/10/`.
struct Rollout {
    path: PathBuf,
}

impl AgentHome {
    /// A thread `thread` started in `cwd`, spawned by `parent` when given.
    fn a_rollout(&self, thread: &str, cwd: &str, parent: Option<&str>) -> Rollout {
        let dir = self.path().join(".codex/sessions/2026/11/10");
        fs::create_dir_all(&dir).expect("mkdir sessions");
        let path = dir.join(format!("rollout-2026-11-10T22-13-04-{thread}.jsonl"));
        let mut payload = serde_json::json!({ "id": thread, "session_id": thread, "cwd": cwd });
        if let Some(parent) = parent {
            payload["parent_thread_id"] = parent.into();
            payload["thread_source"] = "guardian_review".into();
        }
        let rollout = Rollout { path };
        rollout.append(serde_json::json!({ "type": "session_meta", "payload": payload }));
        rollout
    }
}

impl Rollout {
    fn turn_on(self, model: &str) -> Self {
        self.append(serde_json::json!({ "type": "turn_context", "payload": { "model": model, "cwd": WORKTREE } }));
        self
    }

    fn token_count(self, totals: Totals) -> Self {
        self.append(serde_json::json!({
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": totals.input,
                        "cached_input_tokens": totals.cached,
                        "cache_write_input_tokens": 0,
                        "output_tokens": totals.output,
                        "reasoning_output_tokens": totals.reasoning,
                        "total_tokens": totals.input + totals.output,
                    },
                    "last_token_usage": { "input_tokens": 1, "output_tokens": 1 }
                }
            }
        }));
        self
    }

    /// Set the file's modification time `ago` before now.
    fn modified_ago(self, ago: Duration) -> Self {
        let file = fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .expect("open");
        file.set_modified(SystemTime::now() - ago)
            .expect("set mtime");
        self
    }

    fn append(&self, line: serde_json::Value) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .expect("open rollout");
        writeln!(file, "{line}").expect("write");
    }
}

/// A session in `WORKTREE` reading this home — Codex mints its own thread ids, so the session has
/// no conversation id to give.
fn a_codex_session(home: &AgentHome) -> UsageContext {
    UsageContext {
        conversation_id: None,
        worktree: Some(Path::new(WORKTREE).to_path_buf()),
        agent_label: "codex".to_string(),
        ..home.session("unused")
    }
}

#[test]
fn a_thread_reports_its_last_cumulative_token_count() {
    // Given
    let home = an_agent_home();
    home.a_rollout("thr-1", WORKTREE, None)
        .turn_on("gpt-6-astra")
        .token_count(totals(100, 0, 10, 0))
        .token_count(totals(250, 0, 30, 0));

    // When
    let records = account(&the_builtin_binding("codex.yaml"), &a_codex_session(&home));

    // Then
    let main = the_row(&records, "thr-1");
    assert_eq!(
        (main.input_tokens, main.output_tokens, main.model.as_str()),
        (250, 30, "gpt-6-astra")
    );
}

#[test]
fn fresh_input_is_total_input_less_the_cached_part() {
    // Given — the real thread's last totals
    let home = an_agent_home();
    home.a_rollout("thr-1", WORKTREE, None)
        .token_count(totals(58_823_019, 57_833_728, 208_721, 58_680));

    // When
    let records = account(&the_builtin_binding("codex.yaml"), &a_codex_session(&home));

    // Then
    let main = the_row(&records, "thr-1");
    assert_eq!(
        (main.input_tokens, main.cache_read_tokens),
        (58_823_019 - 57_833_728, 57_833_728)
    );
}

#[test]
fn reasoning_is_reported_apart_from_output() {
    // Given
    let home = an_agent_home();
    home.a_rollout("thr-1", WORKTREE, None)
        .token_count(totals(0, 0, 208_721, 58_680));

    // When
    let records = account(&the_builtin_binding("codex.yaml"), &a_codex_session(&home));

    // Then
    let main = the_row(&records, "thr-1");
    assert_eq!(
        (main.output_tokens, main.reasoning_tokens, main.total_tokens),
        (208_721 - 58_680, 58_680, 208_721)
    );
}

#[test]
fn a_spawned_thread_is_a_child_of_the_thread_that_spawned_it() {
    // Given — a thread, the thread it spawned, and one that one spawned
    let home = an_agent_home();
    home.a_rollout("thr-1", WORKTREE, None)
        .token_count(totals(10, 0, 1, 0));
    home.a_rollout("thr-2", WORKTREE, Some("thr-1"))
        .token_count(totals(20, 0, 2, 0));
    home.a_rollout("thr-3", WORKTREE, Some("thr-2"))
        .token_count(totals(30, 0, 3, 0));

    // When
    let records = account(&the_builtin_binding("codex.yaml"), &a_codex_session(&home));

    // Then
    assert_eq!(
        the_tree(&records),
        vec![
            ("thr-1".to_string(), None),
            ("thr-2".to_string(), Some("thr-1".to_string())),
            ("thr-3".to_string(), Some("thr-2".to_string())),
        ]
    );
}

#[test]
fn the_main_thread_is_the_newest_root_rollout_in_the_sessions_worktree() {
    // Given — an older thread in this worktree, a newer one, and one in another checkout
    let home = an_agent_home();
    home.a_rollout("old", WORKTREE, None)
        .token_count(totals(1, 0, 1, 0))
        .modified_ago(Duration::from_secs(3600));
    home.a_rollout("new", WORKTREE, None)
        .token_count(totals(2, 0, 2, 0))
        .modified_ago(Duration::from_secs(60));
    home.a_rollout("elsewhere", "/work/other", None)
        .token_count(totals(3, 0, 3, 0));

    // When
    let records = account(&the_builtin_binding("codex.yaml"), &a_codex_session(&home));

    // Then
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["new"]);
}

#[test]
fn a_child_sharing_its_parents_cwd_is_never_taken_as_the_main_thread() {
    // Given — the spawned thread is the newest file in the worktree
    let home = an_agent_home();
    home.a_rollout("thr-1", WORKTREE, None)
        .token_count(totals(10, 0, 1, 0))
        .modified_ago(Duration::from_secs(600));
    home.a_rollout("thr-2", WORKTREE, Some("thr-1"))
        .token_count(totals(20, 0, 2, 0));

    // When
    let records = account(&the_builtin_binding("codex.yaml"), &a_codex_session(&home));

    // Then
    assert_eq!(records.first().map(|r| r.id.as_str()), Some("thr-1"));
}
