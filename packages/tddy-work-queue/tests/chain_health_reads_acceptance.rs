//! An item's chain health, read through a real inbox over a scripted GitHub and a real clone
//! (`#workqueue` 12/14): leaks from PR commits, the behind count from remote tips, `mergeable` from
//! the PR.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError};
use tddy_github_inbox::{paths, GithubInbox, InboxError, RepoReaders, RepoSlug};
use tddy_work_queue::chain_health::{Leak, LineState};
use tddy_work_queue::chain_health_reads::queue_chain_health;
use tddy_work_queue::lineage::line_edges_through_inbox;

fn the_repo() -> RepoSlug {
    RepoSlug {
        owner: "acme".to_string(),
        name: "app".to_string(),
    }
}

#[derive(Default)]
struct AScriptedGithub {
    answers: Mutex<HashMap<String, VecDeque<ConditionalAnswer>>>,
}

impl AScriptedGithub {
    fn answering(self, path: String, body: impl Into<String>) -> Self {
        self.answers
            .lock()
            .unwrap()
            .entry(path.clone())
            .or_default()
            .push_back(ConditionalAnswer::Modified {
                body: body.into(),
                etag: Some(format!("etag-{path}")),
                poll_interval_ms: Some(60_000),
                rate_limit: None,
            });
        self
    }
}

#[async_trait]
impl ConditionalRead for AScriptedGithub {
    async fn get(
        &self,
        path: &str,
        _if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        let answers = self.answers.lock().unwrap();
        Ok(answers
            .get(path)
            .unwrap_or_else(|| panic!("unscripted request to {path}"))
            .front()
            .cloned()
            .unwrap())
    }
}

struct ReadsThrough(Arc<AScriptedGithub>);

impl RepoReaders for ReadsThrough {
    fn reader_for(&self, _repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        Ok(self.0.clone())
    }
}

fn a_pull_json(number: u64, head: &str, base: &str, mergeable: bool) -> String {
    format!(
        r#"{{"number":{number},"html_url":"https://github.com/acme/app/pull/{number}","title":"PR {number}","body":"","state":"open","draft":false,"base":{{"ref":"{base}"}},"head":{{"ref":"{head}","sha":"sha-{number}"}},"mergeable":{mergeable},"mergeable_state":"clean","additions":1,"deletions":0,"changed_files":1}}"#
    )
}

fn a_commit_json(sha: &str, subject: &str) -> String {
    format!(
        r#"{{"sha":"{sha}","commit":{{"author":{{"name":"dev","date":"2026-11-22T09:00:00Z"}},"message":"{subject}"}}}}"#
    )
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "dev")
        .env("GIT_AUTHOR_EMAIL", "dev@example.com")
        .env("GIT_COMMITTER_NAME", "dev")
        .env("GIT_COMMITTER_EMAIL", "dev@example.com")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(dir: &Path, file: &str, subject: &str) {
    std::fs::write(dir.join(file), subject).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", subject]);
}

/// main ← feat/one (`one`, then `one-more`) and feat/two cut from feat/one after `one` — so feat/two
/// is one commit behind its parent. Returns a clone of the pushed origin.
fn a_clone_where_two_is_behind_one(root: &Path) -> std::path::PathBuf {
    let origin = root.join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--bare", "-b", "main"]);
    git(root, &["clone", origin.to_str().unwrap(), "work"]);
    let work = root.join("work");
    git(&work, &["checkout", "-b", "main"]);
    commit(&work, "base.txt", "base");
    git(&work, &["push", "-u", "origin", "main"]);
    git(&work, &["checkout", "-b", "feat/one"]);
    commit(&work, "one.txt", "one");
    git(&work, &["checkout", "-b", "feat/two"]);
    commit(&work, "two.txt", "two");
    git(&work, &["checkout", "feat/one"]);
    commit(&work, "one-more.txt", "one-more");
    git(&work, &["push", "origin", "feat/one", "feat/two"]);
    work
}

fn the_queue() -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".into(),
                branch: Some("feat/one".into()),
                ..WorkItem::default()
            },
            WorkItem {
                node_id: "n2".into(),
                branch: Some("feat/two".into()),
                parents: vec!["n1".into()],
                ..WorkItem::default()
            },
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn an_item_behind_its_parent_with_the_parents_commit_in_its_pr_is_flagged_on_both() {
    // Given — PR #7 (n1) carries `one`; PR #8 (n2, unmergeable) carries n1's `one` and its own `two`
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::default()
            .answering(
                paths::pulls_for_head(&repo, "feat/one"),
                format!("[{}]", a_pull_json(7, "feat/one", "main", true)),
            )
            .answering(
                paths::pulls_for_head(&repo, "feat/two"),
                format!("[{}]", a_pull_json(8, "feat/two", "feat/one", false)),
            )
            .answering(
                paths::pr_commits(&repo, 7),
                format!("[{}]", a_commit_json("one-sha", "one")),
            )
            .answering(
                paths::pr_commits(&repo, 8),
                format!(
                    "[{},{}]",
                    a_commit_json("one-sha", "one"),
                    a_commit_json("two-sha", "two")
                ),
            ),
    );
    let inbox = GithubInbox::new(Arc::new(ReadsThrough(github)));
    let scratch = tempfile::tempdir().unwrap();
    let clone = a_clone_where_two_is_behind_one(scratch.path());
    let queue = the_queue();
    let line = line_edges_through_inbox(&inbox, &repo, &queue, "main").await;

    // When
    let health = queue_chain_health(&inbox, &repo, &queue, &line, Some(clone)).await;

    // Then
    let of = |id: &str| health.iter().find(|h| h.item_id == id).unwrap().clone();
    assert_eq!(
        (of("n2").leaks, of("n2").line),
        (
            vec![Leak {
                sha: "one-sha".to_string(),
                ancestor: "feat/one".to_string()
            }],
            Some(LineState::Conflicting { by: 1 })
        )
    );
    assert_eq!(
        (of("n1").leaks, of("n1").line),
        (Vec::new(), Some(LineState::Linear))
    );
}

#[tokio::test]
async fn without_a_checkout_the_line_is_unknown_rather_than_guessed() {
    // Given
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::default()
            .answering(
                paths::pulls_for_head(&repo, "feat/one"),
                format!("[{}]", a_pull_json(7, "feat/one", "main", true)),
            )
            .answering(
                paths::pulls_for_head(&repo, "feat/two"),
                format!("[{}]", a_pull_json(8, "feat/two", "feat/one", true)),
            )
            .answering(paths::pr_commits(&repo, 7), "[]")
            .answering(paths::pr_commits(&repo, 8), "[]"),
    );
    let inbox = GithubInbox::new(Arc::new(ReadsThrough(github)));
    let queue = the_queue();
    let line = line_edges_through_inbox(&inbox, &repo, &queue, "main").await;

    // When
    let health = queue_chain_health(&inbox, &repo, &queue, &line, None).await;

    // Then
    assert_eq!(
        health.into_iter().map(|h| h.line).collect::<Vec<_>>(),
        vec![Some(LineState::Unknown), Some(LineState::Unknown)]
    );
}
