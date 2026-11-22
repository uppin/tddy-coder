//! A queue's GitHub reads go through the daemon's inbox (`#workqueue` 6/14; the PR-status leg and the
//! queue-status path moved here from n2 by the operator). The scripted GitHub counts requests, so each
//! test pins that a second reader of the same PR costs nothing.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError};
use tddy_github_inbox::{paths, GithubInbox, InboxError, RepoReaders, RepoSlug};
use tddy_work_queue::assess::PrLiveStatus;
use tddy_work_queue::inbox_reads::{
    assemble_views_through_inbox, item_check_summaries, pr_status_view_through_inbox,
    ItemCheckSummary,
};

fn the_repo() -> RepoSlug {
    RepoSlug {
        owner: "acme".to_string(),
        name: "app".to_string(),
    }
}

#[derive(Default)]
struct AScriptedGithub {
    answers: Mutex<HashMap<String, VecDeque<ConditionalAnswer>>>,
    requests: Mutex<Vec<String>>,
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

    fn requests_to(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|p| *p == path)
            .count()
    }
}

#[async_trait]
impl ConditionalRead for AScriptedGithub {
    async fn get(
        &self,
        path: &str,
        _if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        self.requests.lock().unwrap().push(path.to_string());
        let answers = self.answers.lock().unwrap();
        let queue = answers
            .get(path)
            .unwrap_or_else(|| panic!("unscripted request to {path}"));
        Ok(queue.front().cloned().unwrap())
    }
}

struct ReadsThrough(Arc<AScriptedGithub>);

impl RepoReaders for ReadsThrough {
    fn reader_for(&self, _repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        Ok(self.0.clone())
    }
}

fn an_inbox_over(github: &Arc<AScriptedGithub>) -> GithubInbox {
    GithubInbox::new(Arc::new(ReadsThrough(github.clone())))
}

fn a_pull_json(number: u64, head: &str) -> String {
    format!(
        r#"{{"number":{number},"html_url":"https://github.com/acme/app/pull/{number}","title":"PR {number}","body":"","state":"open","draft":false,"base":{{"ref":"master"}},"head":{{"ref":"{head}","sha":"sha-{number}"}},"mergeable":true,"mergeable_state":"clean","additions":1,"deletions":0,"changed_files":1}}"#
    )
}

/// `feat/one` has PR #7 (head `sha-7`), `feat/two` has PR #8.
fn a_github_with_two_prs() -> AScriptedGithub {
    let repo = the_repo();
    AScriptedGithub::default()
        .answering(
            paths::pulls_for_head(&repo, "feat/one"),
            format!("[{}]", a_pull_json(7, "feat/one")),
        )
        .answering(
            paths::pulls_for_head(&repo, "feat/two"),
            format!("[{}]", a_pull_json(8, "feat/two")),
        )
}

fn a_queue_on(branches: &[(&str, &str)]) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: branches
            .iter()
            .map(|(id, branch)| WorkItem {
                node_id: id.to_string(),
                branch: Some(branch.to_string()),
                ..WorkItem::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_second_reader_of_a_branchs_pr_status_makes_no_github_request() {
    // Given
    let github = Arc::new(a_github_with_two_prs());
    let inbox = an_inbox_over(&github);

    // When — two readers (two open views, or GetPrStatus then QueryBranch)
    let first = pr_status_view_through_inbox(&inbox, &the_repo(), "feat/one").await;
    let second = pr_status_view_through_inbox(&inbox, &the_repo(), "feat/one").await;

    // Then
    assert_eq!(
        (
            first.exists,
            first.number,
            first.state.as_str(),
            second.number
        ),
        (true, 7, "open", 7)
    );
    assert_eq!(
        github.requests_to(&paths::pulls_for_head(&the_repo(), "feat/one")),
        1
    );
}

#[tokio::test]
async fn the_queue_status_reads_each_items_pr_through_the_inbox_once() {
    // Given
    let github = Arc::new(a_github_with_two_prs());
    let inbox = an_inbox_over(&github);
    let dirs = tempfile::tempdir().unwrap();
    let queue = a_queue_on(&[("n1", "feat/one"), ("n2", "feat/two")]);

    // When — the status tool asked twice
    let mut views = Vec::new();
    for _ in 0..2 {
        views = assemble_views_through_inbox(dirs.path(), dirs.path(), &queue, &inbox, &the_repo())
            .await
            .expect("views");
    }

    // Then
    assert_eq!(
        views.iter().map(|v| v.pr.clone()).collect::<Vec<_>>(),
        vec![
            PrLiveStatus::Open {
                number: 7,
                base: "master".to_string()
            },
            PrLiveStatus::Open {
                number: 8,
                base: "master".to_string()
            },
        ]
    );
    assert_eq!(
        (
            github.requests_to(&paths::pulls_for_head(&the_repo(), "feat/one")),
            github.requests_to(&paths::pulls_for_head(&the_repo(), "feat/two")),
        ),
        (1, 1)
    );
}

#[tokio::test]
async fn a_prd_item_summarises_its_running_failed_and_succeeded_checks() {
    // Given — PR #7's head has one running, one failed and one passed check
    let repo = the_repo();
    let github = Arc::new(a_github_with_two_prs().answering(
        paths::check_runs(&repo, "sha-7"),
        r#"{"total_count":3,"check_runs":[
            {"id":1,"name":"build","status":"in_progress","conclusion":null,"started_at":"2026-11-22T10:00:00Z","completed_at":null,"details_url":""},
            {"id":2,"name":"lint","status":"completed","conclusion":"failure","started_at":"2026-11-22T10:00:00Z","completed_at":"2026-11-22T10:01:00Z","details_url":""},
            {"id":3,"name":"test","status":"completed","conclusion":"success","started_at":"2026-11-22T10:00:00Z","completed_at":"2026-11-22T10:02:00Z","details_url":""}]}"#,
    ));
    let inbox = an_inbox_over(&github);
    let queue = a_queue_on(&[("n1", "feat/one")]);

    // When
    let summaries = item_check_summaries(&inbox, &repo, &queue).await;

    // Then — counted, and each run kept for the row's expanded list
    assert_eq!(
        summaries
            .iter()
            .map(|s| ItemCheckSummary {
                runs: Vec::new(),
                ..s.clone()
            })
            .collect::<Vec<_>>(),
        vec![ItemCheckSummary {
            item_id: "n1".to_string(),
            running: 1,
            failed: 1,
            succeeded: 1,
            runs: Vec::new(),
        }]
    );
    assert_eq!(
        summaries[0]
            .runs
            .iter()
            .map(|run| run.name.as_str())
            .collect::<Vec<_>>(),
        vec!["build", "lint", "test"]
    );
}
