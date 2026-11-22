//! The daemon's GitHub inbox (`#workqueue` 2/14) against a scripted GitHub.
//!
//! The scripted GitHub answers each path from a queue and records every request with the ETag it
//! carried, so each test states exactly what reached GitHub. Time is the `now` handed to `step`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError, RateLimit};
use tddy_github::trunk_feed::{Conditional, TrunkFeed};
use tddy_github_inbox::{
    paths, EventKind, GithubInbox, InboxError, InboxEvent, InboxSubscription, InboxTrunkFeed,
    RepoReaders, RepoSlug, RowKey, LOW_QUOTA_REMAINING,
};

const T0: u64 = 1_795_000_000_000;
const A_MINUTE: u64 = 60_000;

fn the_repo() -> RepoSlug {
    RepoSlug {
        owner: "acme".to_string(),
        name: "app".to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// A scripted GitHub
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct AScriptedGithub {
    answers: Mutex<HashMap<String, VecDeque<ConditionalAnswer>>>,
    requests: Mutex<Vec<(String, Option<String>)>>,
    latency: Option<Duration>,
}

impl AScriptedGithub {
    fn new() -> Self {
        Self::default()
    }

    fn slow(mut self, latency: Duration) -> Self {
        self.latency = Some(latency);
        self
    }

    fn answering(self, path: String, answer: ConditionalAnswer) -> Self {
        self.answers
            .lock()
            .unwrap()
            .entry(path)
            .or_default()
            .push_back(answer);
        self
    }

    fn requests_to(&self, path: &str) -> Vec<Option<String>> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, etag)| etag.clone())
            .collect()
    }
}

#[async_trait]
impl ConditionalRead for AScriptedGithub {
    async fn get(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        self.requests
            .lock()
            .unwrap()
            .push((path.to_string(), if_none_match.map(str::to_string)));
        if let Some(latency) = self.latency {
            tokio::time::sleep(latency).await;
        }
        let mut answers = self.answers.lock().unwrap();
        let queue = answers
            .get_mut(path)
            .unwrap_or_else(|| panic!("unscripted request to {path}"));
        // The last scripted answer repeats, so a test scripts only what changes.
        Ok(if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            queue
                .front()
                .cloned()
                .unwrap_or_else(|| panic!("no answer left for {path}"))
        })
    }
}

struct ReadsThrough(Arc<AScriptedGithub>);

impl RepoReaders for ReadsThrough {
    fn reader_for(&self, _repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        Ok(self.0.clone())
    }
}

fn an_inbox_over(github: &Arc<AScriptedGithub>) -> Arc<GithubInbox> {
    Arc::new(GithubInbox::new(Arc::new(ReadsThrough(github.clone()))))
}

fn modified(body: impl Into<String>, etag: &str) -> ConditionalAnswer {
    ConditionalAnswer::Modified {
        body: body.into(),
        etag: Some(etag.to_string()),
        poll_interval_ms: Some(A_MINUTE),
        rate_limit: None,
    }
}

fn not_modified() -> ConditionalAnswer {
    ConditionalAnswer::NotModified {
        poll_interval_ms: Some(A_MINUTE),
        rate_limit: None,
    }
}

fn a_pull_json(number: u64, head: &str) -> String {
    format!(
        r#"{{"number":{number},"html_url":"https://github.com/acme/app/pull/{number}","title":"PR {number}","body":"","state":"open","base":{{"ref":"master"}},"head":{{"ref":"{head}","sha":"sha-{number}"}},"mergeable":true,"mergeable_state":"clean","additions":1,"deletions":0,"changed_files":1}}"#
    )
}

fn events_json(events: &[String]) -> String {
    format!("[{}]", events.join(","))
}

fn a_pull_request_event(number: u64, head: &str) -> String {
    format!(
        r#"{{"id":"ev-pr-{number}","type":"PullRequestEvent","payload":{{"action":"synchronize","number":{number},"pull_request":{{"number":{number},"head":{{"ref":"{head}","sha":"sha-{number}-new"}}}}}}}}"#
    )
}

fn an_issue_comment_event(number: u64) -> String {
    format!(
        r#"{{"id":"ev-comment-{number}","type":"IssueCommentEvent","payload":{{"action":"created","issue":{{"number":{number},"pull_request":{{}}}}}}}}"#
    )
}

fn check_runs_json(status: &str, conclusion: Option<&str>) -> String {
    let conclusion = conclusion.map_or("null".to_string(), |c| format!("\"{c}\""));
    format!(
        r#"{{"total_count":1,"check_runs":[{{"id":1,"name":"build","status":"{status}","conclusion":{conclusion},"started_at":"2026-11-22T10:00:00Z","completed_at":null,"details_url":"https://github.com/acme/app/runs/1"}}]}}"#
    )
}

fn drained(subscription: &mut InboxSubscription) -> Vec<InboxEvent> {
    std::iter::from_fn(|| subscription.try_next()).collect()
}

// ---------------------------------------------------------------------------------------------
// The feed
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn two_subscribers_of_one_repository_share_one_feed() {
    // Given
    let repo = the_repo();
    let github =
        Arc::new(AScriptedGithub::new().answering(paths::events(&repo), modified("[]", "e1")));
    let inbox = an_inbox_over(&github);
    let _pr_watcher = inbox.subscribe(&repo, &[EventKind::Pr]);
    let _checks_watcher = inbox.subscribe(&repo, &[EventKind::Checks]);

    // When
    inbox.step(T0).await;

    // Then
    assert_eq!(github.requests_to(&paths::events(&repo)), vec![None]);
}

#[tokio::test]
async fn a_not_modified_feed_answer_changes_nothing_and_waits_out_the_poll_interval() {
    // Given
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::new()
            .answering(paths::events(&repo), modified("[]", "e1"))
            .answering(paths::events(&repo), not_modified()),
    );
    let inbox = an_inbox_over(&github);
    let mut watcher = inbox.subscribe(&repo, &[EventKind::Pr, EventKind::Comments]);

    // When
    inbox.step(T0).await;
    inbox.step(T0 + 1_000).await;
    inbox.step(T0 + A_MINUTE).await;

    // Then — one read per poll interval, the second conditional, and nothing to report
    assert_eq!(
        github.requests_to(&paths::events(&repo)),
        vec![None, Some("e1".to_string())]
    );
    assert_eq!(drained(&mut watcher), vec![]);
}

#[tokio::test]
async fn a_pull_request_event_invalidates_only_that_pull_requests_rows() {
    // Given
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::new()
            .answering(
                paths::pull(&repo, 1),
                modified(a_pull_json(1, "feat/one"), "p1"),
            )
            .answering(
                paths::pull(&repo, 2),
                modified(a_pull_json(2, "feat/two"), "p2"),
            )
            .answering(paths::events(&repo), modified("[]", "e1"))
            .answering(
                paths::events(&repo),
                modified(events_json(&[a_pull_request_event(1, "feat/one")]), "e2"),
            ),
    );
    let inbox = an_inbox_over(&github);
    let mut watcher = inbox.subscribe(&repo, &[EventKind::Pr]);
    inbox.pr(&repo, 1).await.expect("pr 1");
    inbox.pr(&repo, 2).await.expect("pr 2");
    inbox.step(T0).await;

    // When
    inbox.step(T0 + A_MINUTE).await;
    inbox.pr(&repo, 1).await.expect("pr 1 again");
    inbox.pr(&repo, 2).await.expect("pr 2 again");

    // Then — #1 was told about and re-read; #2 was neither
    let heard = drained(&mut watcher);
    assert!(
        heard.contains(&InboxEvent {
            repo: repo.clone(),
            kind: EventKind::Pr,
            row: RowKey::Pr { number: 1 },
        }),
        "the PR subscriber hears #1 changed: {heard:?}"
    );
    assert!(
        !heard
            .iter()
            .any(|event| event.row == RowKey::Pr { number: 2 }),
        "nothing is said about #2: {heard:?}"
    );
    assert_eq!(github.requests_to(&paths::pull(&repo, 1)).len(), 2);
    assert_eq!(github.requests_to(&paths::pull(&repo, 2)).len(), 1);
}

#[tokio::test]
async fn a_stale_row_is_revalidated_with_its_stored_etag() {
    // Given
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::new()
            .answering(
                paths::pull(&repo, 1),
                modified(a_pull_json(1, "feat/one"), "p1"),
            )
            .answering(paths::pull(&repo, 1), not_modified())
            .answering(paths::events(&repo), modified("[]", "e1"))
            .answering(
                paths::events(&repo),
                modified(events_json(&[a_pull_request_event(1, "feat/one")]), "e2"),
            ),
    );
    let inbox = an_inbox_over(&github);
    let _watcher = inbox.subscribe(&repo, &[EventKind::Pr]);
    let first = inbox.pr(&repo, 1).await.expect("pr 1");
    inbox.step(T0).await;
    inbox.step(T0 + A_MINUTE).await;

    // When
    let revalidated = inbox.pr(&repo, 1).await.expect("pr 1 revalidated");

    // Then — the stored ETag went out, and a 304 keeps the value
    assert_eq!(
        github.requests_to(&paths::pull(&repo, 1)),
        vec![None, Some("p1".to_string())]
    );
    assert_eq!(revalidated.value, first.value);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_reads_of_one_row_make_one_request() {
    // Given — a GitHub slow enough that every read is in flight at once
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::new()
            .slow(Duration::from_millis(100))
            .answering(
                paths::pull(&repo, 1),
                modified(a_pull_json(1, "feat/one"), "p1"),
            ),
    );
    let inbox = an_inbox_over(&github);

    // When
    let mut reads = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let (inbox, repo) = (inbox.clone(), repo.clone());
        reads.spawn(async move { inbox.pr(&repo, 1).await.map(|row| row.value.number) });
    }
    let answers: Vec<_> = reads.join_all().await;

    // Then
    assert_eq!(answers, vec![Ok(1); 10]);
    assert_eq!(github.requests_to(&paths::pull(&repo, 1)), vec![None]);
}

#[tokio::test]
async fn a_checks_subscriber_hears_a_pending_check_complete_and_not_a_new_comment() {
    // Given
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::new()
            .answering(
                paths::check_runs(&repo, "sha-1"),
                modified(check_runs_json("in_progress", None), "c1"),
            )
            .answering(
                paths::check_runs(&repo, "sha-1"),
                modified(check_runs_json("completed", Some("success")), "c2"),
            )
            .answering(paths::events(&repo), modified("[]", "e1"))
            .answering(
                paths::events(&repo),
                modified(events_json(&[an_issue_comment_event(1)]), "e2"),
            ),
    );
    let inbox = an_inbox_over(&github);
    let mut checks_watcher = inbox.subscribe(&repo, &[EventKind::Checks]);
    inbox.checks_at(&repo, "sha-1").await.expect("checks");

    // When
    inbox.step(T0).await;
    inbox.step(T0 + A_MINUTE).await;

    // Then
    assert_eq!(
        drained(&mut checks_watcher),
        vec![InboxEvent {
            repo: repo.clone(),
            kind: EventKind::Checks,
            row: RowKey::ChecksAt {
                sha: "sha-1".to_string()
            },
        }]
    );
}

#[tokio::test]
async fn checks_are_polled_only_while_some_head_has_pending_checks() {
    // Given — every check at the only head has completed
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::new()
            .answering(
                paths::check_runs(&repo, "sha-1"),
                modified(check_runs_json("completed", Some("success")), "c1"),
            )
            .answering(paths::events(&repo), modified("[]", "e1"))
            .answering(paths::events(&repo), not_modified()),
    );
    let inbox = an_inbox_over(&github);
    let _checks_watcher = inbox.subscribe(&repo, &[EventKind::Checks]);
    inbox.checks_at(&repo, "sha-1").await.expect("checks");

    // When
    inbox.step(T0).await;
    inbox.step(T0 + A_MINUTE).await;

    // Then — only the read itself reached the check-runs endpoint
    assert_eq!(
        github.requests_to(&paths::check_runs(&repo, "sha-1")).len(),
        1
    );
}

#[tokio::test]
async fn a_low_remaining_quota_slows_the_feed_until_the_reset_and_reads_still_answer_from_cache() {
    // Given — the feed's answer says the quota is nearly spent, resetting in an hour
    let repo = the_repo();
    let reset_at_unix_s = T0 / 1000 + 3600;
    let github = Arc::new(
        AScriptedGithub::new()
            .answering(
                paths::pull(&repo, 1),
                modified(a_pull_json(1, "feat/one"), "p1"),
            )
            .answering(
                paths::events(&repo),
                ConditionalAnswer::Modified {
                    body: "[]".to_string(),
                    etag: Some("e1".to_string()),
                    poll_interval_ms: Some(A_MINUTE),
                    rate_limit: Some(RateLimit {
                        remaining: LOW_QUOTA_REMAINING - 1,
                        reset_at_unix_s,
                    }),
                },
            )
            .answering(paths::events(&repo), not_modified()),
    );
    let inbox = an_inbox_over(&github);
    let _watcher = inbox.subscribe(&repo, &[EventKind::Pr]);
    inbox.pr(&repo, 1).await.expect("pr 1");
    inbox.step(T0).await;

    // When
    inbox.step(T0 + 2 * A_MINUTE).await;
    let cached = inbox.pr(&repo, 1).await;
    inbox.step(reset_at_unix_s * 1000).await;

    // Then
    assert_eq!(cached.map(|row| row.value.number), Ok(1));
    assert_eq!(github.requests_to(&paths::pull(&repo, 1)).len(), 1);
    assert_eq!(
        github.requests_to(&paths::events(&repo)),
        vec![None, Some("e1".to_string())],
        "no feed read between the low-quota answer and the reset"
    );
}

#[tokio::test]
async fn repeated_reads_of_an_unchanged_pr_for_head_row_make_no_further_requests() {
    // Given — five open views of one branch
    let repo = the_repo();
    let github = Arc::new(AScriptedGithub::new().answering(
        paths::pulls_for_head(&repo, "feat/one"),
        modified(format!("[{}]", a_pull_json(1, "feat/one")), "h1"),
    ));
    let inbox = an_inbox_over(&github);

    // When
    for _ in 0..5 {
        inbox
            .pr_for_head(&repo, "feat/one")
            .await
            .expect("pr for head");
    }

    // Then
    assert_eq!(
        github
            .requests_to(&paths::pulls_for_head(&repo, "feat/one"))
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------------------------
// The trunk watch's source
// ---------------------------------------------------------------------------------------------

fn a_commits_json(sha: &str, parent: &str, subject: &str) -> String {
    format!(
        r#"[{{"sha":"{sha}","commit":{{"author":{{"name":"dev","date":"2026-11-22T10:00:00Z"}},"committer":{{"name":"dev","date":"2026-11-22T10:00:00Z"}},"message":"{subject}\n\nbody"}},"parents":[{{"sha":"{parent}"}}]}}]"#
    )
}

#[tokio::test]
async fn the_trunk_feed_reads_branch_commits_through_the_inbox_row() {
    // Given
    let repo = the_repo();
    let github = Arc::new(AScriptedGithub::new().answering(
        paths::branch_commits(&repo, "master"),
        modified(a_commits_json("c1", "c0", "land the thing"), "b1"),
    ));
    let feed = InboxTrunkFeed::new(an_inbox_over(&github));

    // When — a first read, then a read carrying the ETag it was given
    let first = feed
        .newest_commits(&repo, "master", None)
        .await
        .expect("first read");
    let second = feed
        .newest_commits(&repo, "master", Some("b1"))
        .await
        .expect("second read");

    // Then — the row answered both; GitHub was asked once
    match first {
        Conditional::Modified { value, etag, .. } => {
            assert_eq!(
                (
                    value.iter().map(|c| c.sha.as_str()).collect::<Vec<_>>(),
                    etag
                ),
                (vec!["c1"], Some("b1".to_string()))
            );
        }
        other => panic!("the first read carries the commits: {other:?}"),
    }
    assert!(matches!(second, Conditional::NotModified { .. }));
    assert_eq!(
        github
            .requests_to(&paths::branch_commits(&repo, "master"))
            .len(),
        1
    );
}
