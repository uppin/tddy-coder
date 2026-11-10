//! Acceptance: the OpenCode usage binding shipped in `opencode.yaml`, evaluated by the
//! agent-agnostic `sqlite` format over a store with OpenCode's real `session_v2` schema
//! (`#usage-binding 5/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-opencode-sqlite.md

mod support;

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use support::{an_agent_home, the_builtin_binding, the_row, the_tree, AgentHome};
use tddy_usage::{account, UsageContext};

const WORKTREE: &str = "/work/repo";

/// `session_v2` as OpenCode creates it (copied from a real `opencode.db`, 2026-11-10).
const SESSION_V2_DDL: &str = r#"CREATE TABLE `session_v2` (
  `id` text PRIMARY KEY, `project_id` text NOT NULL, `workspace_id` text, `parent_id` text,
  `fork_session_id` text, `fork_boundary` text, `slug` text NOT NULL, `directory` text NOT NULL,
  `path` text, `title` text, `version` text NOT NULL, `share_url` text, `summary_additions` integer,
  `summary_deletions` integer, `summary_files` integer, `summary_diffs` text, `metadata` text,
  `cost` real DEFAULT 0 NOT NULL, `tokens_input` integer DEFAULT 0 NOT NULL,
  `tokens_output` integer DEFAULT 0 NOT NULL, `tokens_reasoning` integer DEFAULT 0 NOT NULL,
  `tokens_cache_read` integer DEFAULT 0 NOT NULL, `tokens_cache_write` integer DEFAULT 0 NOT NULL,
  `revert` text, `permission` text, `agent` text, `model` text, `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL, `time_idle` integer, `time_viewed` integer, `idle_outcome` text,
  `time_compacting` integer, `time_archived` integer, `time_suspended` integer,
  `resume_attempts` integer DEFAULT 0 NOT NULL)"#;

/// One `session_v2` row: what matters to accounting, everything else filled in.
struct ASession<'a> {
    id: &'a str,
    parent: Option<&'a str>,
    directory: &'a str,
    updated: i64,
    tokens: (i64, i64, i64, i64, i64),
    cost: f64,
}

fn a_session(id: &str) -> ASession<'_> {
    ASession {
        id,
        parent: None,
        directory: WORKTREE,
        updated: 1,
        tokens: (0, 0, 0, 0, 0),
        cost: 0.0,
    }
}

/// The store OpenCode keeps under its XDG data dir in `home`.
fn the_store(home: &AgentHome) -> PathBuf {
    home.path().join(".local/share/opencode/opencode.db")
}

fn a_store_holding(home: &AgentHome, sessions: &[ASession<'_>]) -> PathBuf {
    let path = the_store(home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&path).expect("create the store");
    db.execute(SESSION_V2_DDL, []).expect("create session_v2");
    for s in sessions {
        let (input, output, reasoning, cache_read, cache_write) = s.tokens;
        db.execute(
            "INSERT INTO session_v2 (id, project_id, parent_id, slug, directory, version, cost,
               tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write,
               agent, model, time_created, time_updated)
             VALUES (?1, 'p', ?2, ?1, ?3, '1', ?4, ?5, ?6, ?7, ?8, ?9, 'build', 'claude-sonnet-4-5', 1, ?10)",
            rusqlite::params![
                s.id, s.parent, s.directory, s.cost, input, output, reasoning, cache_read,
                cache_write, s.updated
            ],
        )
        .expect("insert a session");
    }
    path
}

fn an_opencode_session(home: &AgentHome) -> UsageContext {
    UsageContext {
        conversation_id: None,
        worktree: Some(Path::new(WORKTREE).to_path_buf()),
        agent_label: "opencode".to_string(),
        ..home.session("unused")
    }
}

#[test]
fn the_worktrees_newest_root_session_is_the_main_conversation() {
    // Given — an older and a newer session in this worktree, and one in another checkout
    let home = an_agent_home();
    a_store_holding(
        &home,
        &[
            ASession {
                updated: 10,
                ..a_session("ses_old")
            },
            ASession {
                updated: 20,
                ..a_session("ses_new")
            },
            ASession {
                directory: "/work/other",
                updated: 30,
                ..a_session("ses_elsewhere")
            },
        ],
    );

    // When
    let records = account(
        &the_builtin_binding("opencode.yaml"),
        &an_opencode_session(&home),
    );

    // Then
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["ses_new"]);
}

#[test]
fn a_spawned_session_and_its_own_child_nest_under_their_parents() {
    // Given
    let home = an_agent_home();
    a_store_holding(
        &home,
        &[
            a_session("ses_root"),
            ASession {
                parent: Some("ses_root"),
                updated: 50,
                ..a_session("ses_child")
            },
            ASession {
                parent: Some("ses_child"),
                updated: 60,
                ..a_session("ses_grandchild")
            },
        ],
    );

    // When
    let records = account(
        &the_builtin_binding("opencode.yaml"),
        &an_opencode_session(&home),
    );

    // Then
    assert_eq!(
        the_tree(&records),
        vec![
            ("ses_root".to_string(), None),
            ("ses_child".to_string(), Some("ses_root".to_string())),
            ("ses_grandchild".to_string(), Some("ses_child".to_string())),
        ]
    );
}

#[test]
fn tokens_cache_reasoning_and_cost_come_from_the_session_row() {
    // Given
    let home = an_agent_home();
    a_store_holding(
        &home,
        &[ASession {
            tokens: (120, 30, 7, 9000, 400),
            cost: 0.0421,
            ..a_session("ses_root")
        }],
    );

    // When
    let records = account(
        &the_builtin_binding("opencode.yaml"),
        &an_opencode_session(&home),
    );

    // Then
    let main = the_row(&records, "ses_root");
    assert_eq!(
        (
            main.input_tokens,
            main.output_tokens,
            main.reasoning_tokens,
            main.cache_read_tokens,
            main.cache_write_tokens,
            main.cost_usd,
            main.model.as_str(),
        ),
        (120, 30, 7, 9000, 400, Some(0.0421), "claude-sonnet-4-5")
    );
}

#[test]
fn a_locked_database_reports_nothing_rather_than_a_zero_row() {
    // Given — a writer holding an exclusive lock, as a running agent may
    let home = an_agent_home();
    let path = a_store_holding(
        &home,
        &[ASession {
            tokens: (5, 5, 0, 0, 0),
            ..a_session("ses_root")
        }],
    );
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.execute_batch("BEGIN EXCLUSIVE;").unwrap();

    // When
    let records = account(
        &the_builtin_binding("opencode.yaml"),
        &an_opencode_session(&home),
    );

    // Then — unavailable, not "zero tokens": the caller keeps its last snapshot
    assert!(
        records.iter().all(|r| r.total_tokens != 0),
        "a locked store must not report a zero row: {records:#?}"
    );
    writer.execute_batch("ROLLBACK;").unwrap();
}

#[test]
fn a_missing_database_reports_the_main_conversation_as_a_zero_row() {
    // Given — OpenCode has not run yet
    let home = an_agent_home();

    // When
    let records = account(
        &the_builtin_binding("opencode.yaml"),
        &an_opencode_session(&home),
    );

    // Then
    assert_eq!(
        records
            .iter()
            .map(|r| (r.agent.as_str(), r.total_tokens))
            .collect::<Vec<_>>(),
        vec![("opencode", 0)]
    );
}
