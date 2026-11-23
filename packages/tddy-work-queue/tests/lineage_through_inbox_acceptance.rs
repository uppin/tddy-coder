//! Line edges are read from GitHub, never stored (`#workqueue` 9/14): retargeting a PR moves its line
//! edge with no write to the queue. Through a real inbox over a scripted GitHub.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError};
use tddy_github_inbox::{paths, EventKind, GithubInbox, InboxError, RepoReaders, RepoSlug};
use tddy_work_queue::lineage::{line_edges_through_inbox, LineEndpoint};

const T0: u64 = 1_795_000_000_000;

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
        let n = self
            .answers
            .lock()
            .unwrap()
            .get(&path)
            .map_or(0, VecDeque::len);
        self.answers
            .lock()
            .unwrap()
            .entry(path.clone())
            .or_default()
            .push_back(ConditionalAnswer::Modified {
                body: body.into(),
                etag: Some(format!("etag-{path}-{n}")),
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
        let mut answers = self.answers.lock().unwrap();
        let queue = answers
            .get_mut(path)
            .unwrap_or_else(|| panic!("unscripted request to {path}"));
        Ok(if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            queue.front().cloned().unwrap()
        })
    }
}

struct ReadsThrough(Arc<AScriptedGithub>);

impl RepoReaders for ReadsThrough {
    fn reader_for(&self, _repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        Ok(self.0.clone())
    }
}

fn a_pull_json(number: u64, head: &str, base: &str) -> String {
    format!(
        r#"{{"number":{number},"html_url":"https://github.com/acme/app/pull/{number}","title":"PR {number}","body":"","state":"open","draft":false,"base":{{"ref":"{base}"}},"head":{{"ref":"{head}","sha":"sha-{number}"}},"mergeable":true,"mergeable_state":"clean","additions":1,"deletions":0,"changed_files":1}}"#
    )
}

#[tokio::test]
async fn retargeting_a_pr_on_github_moves_its_line_edge_without_a_queue_write() {
    // Given — n2's PR #8 is based on feat/one (n1's branch), then retargeted onto master
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::default()
            .answering(paths::pulls_for_head(&repo, "feat/one"), "[]")
            .answering(
                paths::pulls_for_head(&repo, "feat/two"),
                format!("[{}]", a_pull_json(8, "feat/two", "feat/one")),
            )
            .answering(
                paths::pulls_for_head(&repo, "feat/two"),
                format!("[{}]", a_pull_json(8, "feat/two", "master")),
            )
            .answering(paths::events(&repo), "[]")
            .answering(
                paths::events(&repo),
                r#"[{"id":"ev-1","type":"PullRequestEvent","payload":{"action":"edited","number":8,"pull_request":{"number":8,"head":{"ref":"feat/two","sha":"sha-8"}}}}]"#,
            ),
    );
    let inbox = GithubInbox::new(Arc::new(ReadsThrough(github.clone())));
    let _watching = inbox.subscribe(&repo, &[EventKind::Pr]);
    let queue = WorkQueue {
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
    };
    let before = line_edges_through_inbox(&inbox, &repo, &queue, "master").await;

    // When — GitHub reports the retarget
    inbox.step(T0).await;
    inbox.step(T0 + 60_000).await;
    let after = line_edges_through_inbox(&inbox, &repo, &queue, "master").await;

    // Then
    let n2 = |edges: &[(String, LineEndpoint)]| {
        edges
            .iter()
            .find(|(id, _)| id == "n2")
            .cloned()
            .map(|(_, e)| e)
    };
    assert_eq!(
        (n2(&before), n2(&after)),
        (
            Some(LineEndpoint::Item("n1".to_string())),
            Some(LineEndpoint::Trunk("master".to_string()))
        )
    );
}
