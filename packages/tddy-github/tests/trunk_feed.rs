//! The trunk poller reports each new trunk commit once, oldest first, and a force-push as a
//! rewrite; it reads conditionally, waits out GitHub's poll interval, backs off after a failure and
//! sweeps unconditionally every fifteen minutes (`#orchestra` 5/9). The transports it reads through
//! are tested in `conditional_read_over_http.rs`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_github::trunk_feed::{
    Conditional, PollOutcome, RepoSlug, TrunkChange, TrunkCommit, TrunkFeed, TrunkPoller,
    TrunkReadError, DEFAULT_POLL_INTERVAL_MS, ERROR_BACKOFF_MS, SWEEP_EVERY_MS,
};

type Answer = Result<Conditional<Vec<TrunkCommit>>, TrunkReadError>;

/// A feed that answers from a script and records the ETag each read sent.
#[derive(Default)]
struct AScriptedFeed {
    answers: Mutex<VecDeque<Answer>>,
    etags_sent: Mutex<Vec<Option<String>>>,
}

impl AScriptedFeed {
    fn answering(answers: Vec<Answer>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into()),
            etags_sent: Mutex::default(),
        })
    }

    fn etags_sent(&self) -> Vec<Option<String>> {
        self.etags_sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl TrunkFeed for AScriptedFeed {
    async fn newest_commits(&self, _repo: &RepoSlug, _branch: &str, etag: Option<&str>) -> Answer {
        self.etags_sent
            .lock()
            .unwrap()
            .push(etag.map(str::to_string));
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("the test scripted an answer for every read")
    }
}

fn commit(sha: &str, parent: &str) -> TrunkCommit {
    TrunkCommit {
        sha: sha.to_string(),
        author: "dev".to_string(),
        subject: format!("commit {sha}"),
        committed_at_unix_ms: 0,
        parent: parent.to_string(),
    }
}

/// A 200 answer: `newest_first` commits, ETag `etag`, no poll interval.
fn modified(newest_first: Vec<TrunkCommit>, etag: &str) -> Answer {
    Ok(Conditional::Modified {
        value: newest_first,
        etag: Some(etag.to_string()),
        poll_interval_ms: None,
    })
}

fn a_poller(feed: Arc<AScriptedFeed>) -> TrunkPoller {
    TrunkPoller::new(
        feed,
        RepoSlug {
            owner: "acme".to_string(),
            name: "app".to_string(),
        },
        "main".to_string(),
    )
}

#[tokio::test]
async fn the_first_poll_is_a_baseline_and_reports_nothing() {
    // Given a trunk with history
    let feed = AScriptedFeed::answering(vec![modified(
        vec![commit("b", "a"), commit("a", "")],
        "e1",
    )]);
    let mut poller = a_poller(feed);

    // When it is polled for the first time
    let outcome = poller.poll(1_000).await;

    // Then nothing already on the trunk is reported
    assert_eq!(
        outcome,
        PollOutcome {
            changes: vec![],
            next_poll_at_unix_ms: 1_000 + DEFAULT_POLL_INTERVAL_MS
        }
    );
}

#[tokio::test]
async fn new_commits_are_reported_once_oldest_first() {
    // Given a baseline at b, then two commits landing
    let feed = AScriptedFeed::answering(vec![
        modified(vec![commit("b", "a")], "e1"),
        modified(
            vec![commit("d", "c"), commit("c", "b"), commit("b", "a")],
            "e2",
        ),
        Ok(Conditional::NotModified {
            poll_interval_ms: None,
        }),
    ]);
    let mut poller = a_poller(Arc::clone(&feed));
    poller.poll(0).await;

    // When it polls twice more
    let landed = poller.poll(DEFAULT_POLL_INTERVAL_MS).await;
    let quiet = poller.poll(2 * DEFAULT_POLL_INTERVAL_MS).await;

    // Then the two commits are reported once, oldest first, and the read after sends the new ETag
    assert_eq!(
        landed.changes,
        vec![TrunkChange::Landed(vec![
            commit("c", "b"),
            commit("d", "c")
        ])]
    );
    assert_eq!(quiet.changes, vec![]);
    assert_eq!(
        feed.etags_sent(),
        vec![None, Some("e1".to_string()), Some("e2".to_string())]
    );
}

#[tokio::test]
async fn a_not_modified_answer_reports_nothing_and_waits_out_the_poll_interval() {
    // Given GitHub asks for 90 s between polls
    let feed = AScriptedFeed::answering(vec![
        modified(vec![commit("b", "a")], "e1"),
        Ok(Conditional::NotModified {
            poll_interval_ms: Some(90_000),
        }),
    ]);
    let mut poller = a_poller(feed);
    poller.poll(0).await;

    // When a poll is not modified
    let outcome = poller.poll(60_000).await;

    // Then nothing is reported and the next poll is 90 s on
    assert_eq!(
        outcome,
        PollOutcome {
            changes: vec![],
            next_poll_at_unix_ms: 150_000
        }
    );
}

#[tokio::test]
async fn a_failed_read_backs_off_before_the_next_poll() {
    // Given a read that fails
    let feed = AScriptedFeed::answering(vec![Err(TrunkReadError("502".to_string()))]);
    let mut poller = a_poller(feed);

    // When it is polled
    let outcome = poller.poll(5_000).await;

    // Then nothing is reported and the next poll waits out the backoff
    assert_eq!(
        outcome,
        PollOutcome {
            changes: vec![],
            next_poll_at_unix_ms: 5_000 + ERROR_BACKOFF_MS
        }
    );
}

#[tokio::test]
async fn a_tip_that_does_not_descend_from_the_last_one_is_a_rewrite() {
    // Given a baseline at c, then a force-push to x
    let feed = AScriptedFeed::answering(vec![
        modified(vec![commit("c", "b"), commit("b", "a")], "e1"),
        modified(vec![commit("x", "a"), commit("a", "")], "e2"),
    ]);
    let mut poller = a_poller(feed);
    poller.poll(0).await;

    // When it polls
    let outcome = poller.poll(DEFAULT_POLL_INTERVAL_MS).await;

    // Then the trunk is reported rewritten from c to x
    assert_eq!(
        outcome.changes,
        vec![TrunkChange::Rewritten {
            from: "c".to_string(),
            to: commit("x", "a")
        }]
    );
}

#[tokio::test]
async fn every_fifteen_minutes_the_read_ignores_its_etag() {
    // Given a poller with an ETag
    let feed = AScriptedFeed::answering(vec![
        modified(vec![commit("b", "a")], "e1"),
        modified(vec![commit("b", "a")], "e1"),
    ]);
    let mut poller = a_poller(Arc::clone(&feed));
    poller.poll(0).await;

    // When the sweep interval has passed
    poller.poll(SWEEP_EVERY_MS).await;

    // Then that read sends no ETag
    assert_eq!(feed.etags_sent(), vec![None, None]);
}
