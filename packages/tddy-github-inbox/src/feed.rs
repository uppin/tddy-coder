//! One repository's feed: the activity events, the updated-pulls sweep and the pending-check poll.
//!
//! The events feed is read conditionally, no oftener than `X-Poll-Interval` allows. Each event it
//! has not shown before invalidates the rows it affects; the checks and comments themselves are
//! read when a consumer next asks. Check runs are not in the events feed, so a head whose runs are
//! unfinished is polled until they finish.

use std::collections::HashSet;

use serde_json::Value;
use tddy_github::conditional::{ConditionalAnswer, GithubReadError};

use crate::repo::{RepoInbox, RepoState};
use crate::rows::Tally;
use crate::{paths, RowKey};

/// What the feed remembers between reads.
#[derive(Default)]
pub(crate) struct FeedMemory {
    events_etag: Option<String>,
    /// Ids the last events answer held; `None` before the first answer, which is the baseline.
    seen_events: Option<HashSet<String>>,
    sweep_etag: Option<String>,
}

/// What a feed event says changed.
#[derive(Debug, PartialEq, Eq)]
enum Change {
    PullRequest { number: u64, head: Option<String> },
    Push { branch: String },
    Conversation { number: u64 },
}

struct FeedEvent {
    id: String,
    change: Option<Change>,
}

fn parse_events(body: &str) -> Result<Vec<FeedEvent>, String> {
    let json: Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    let items = json
        .as_array()
        .ok_or_else(|| "the events answer is not an array".to_string())?;
    Ok(items.iter().filter_map(parse_event).collect())
}

fn parse_event(event: &Value) -> Option<FeedEvent> {
    let id = match event.get("id")? {
        Value::String(id) => id.clone(),
        other => other.to_string(),
    };
    let payload = event.get("payload")?;
    let number = |path: &str| payload.pointer(path).and_then(Value::as_u64);
    let change = match event.get("type")?.as_str()? {
        "PullRequestEvent" => number("/number")
            .or_else(|| number("/pull_request/number"))
            .map(|number| Change::PullRequest {
                number,
                head: payload
                    .pointer("/pull_request/head/ref")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
        "PushEvent" => payload
            .get("ref")
            .and_then(Value::as_str)
            .map(|git_ref| Change::Push {
                branch: git_ref
                    .strip_prefix("refs/heads/")
                    .unwrap_or(git_ref)
                    .to_string(),
            }),
        // A comment on a plain issue is not a PR's conversation.
        "IssueCommentEvent" if payload.pointer("/issue/pull_request").is_some() => {
            number("/issue/number").map(|number| Change::Conversation { number })
        }
        "PullRequestReviewEvent" | "PullRequestReviewCommentEvent" => {
            number("/pull_request/number").map(|number| Change::Conversation { number })
        }
        _ => None,
    };
    Some(FeedEvent { id, change })
}

fn push_unique(keys: &mut Vec<RowKey>, key: RowKey) {
    if !keys.contains(&key) {
        keys.push(key);
    }
}

/// Invalidate what `changes` affect.
fn invalidate_for(state: &mut RepoState, changes: &[Change]) {
    let (mut announce, mut if_cached) = (Vec::new(), Vec::new());
    for change in changes {
        match change {
            Change::PullRequest { number, head } => {
                push_unique(&mut announce, RowKey::Pr { number: *number });
                if let Some(head) = head {
                    push_unique(
                        &mut announce,
                        RowKey::PrForHead {
                            branch: head.clone(),
                        },
                    );
                }
                for key in state.commit_rows_of(*number) {
                    push_unique(&mut if_cached, key);
                }
            }
            Change::Push { branch } => {
                push_unique(
                    &mut announce,
                    RowKey::BranchCommits {
                        branch: branch.clone(),
                    },
                );
                push_unique(
                    &mut announce,
                    RowKey::PrForHead {
                        branch: branch.clone(),
                    },
                );
            }
            Change::Conversation { number } => {
                push_unique(&mut announce, RowKey::Conversation { number: *number });
            }
        }
    }
    state.invalidate(&announce, &if_cached);
}

/// Advance one repository's feed to `now_ms`.
pub(crate) async fn step_repo(inbox: &RepoInbox, now_ms: u64, tally: &mut Tally) {
    let (feed_due, checks_due, sweep_due) = {
        let mut state = inbox.state();
        let sweep_due = state.pacing.sweep_due(now_ms);
        (
            state.pacing.feed_due(now_ms),
            state.pacing.pending_checks_due(now_ms),
            sweep_due,
        )
    };
    if feed_due {
        poll_events(inbox, now_ms, tally).await;
    }
    if checks_due {
        poll_pending_checks(inbox, now_ms, tally).await;
    }
    if sweep_due {
        sweep_updated_pulls(inbox, now_ms, tally).await;
    }
}

fn read_failed(inbox: &RepoInbox, what: &str, error: &GithubReadError) {
    log::warn!(
        "GitHub inbox: {what} of {}/{} failed: {}",
        inbox.repo.owner,
        inbox.repo.name,
        error.message
    );
}

async fn poll_events(inbox: &RepoInbox, now_ms: u64, tally: &mut Tally) {
    let reader = match inbox.readers().reader_for(&inbox.repo) {
        Ok(reader) => reader,
        Err(error) => {
            log::warn!(
                "GitHub inbox: no reader for {}/{}: {error:?}",
                inbox.repo.owner,
                inbox.repo.name
            );
            inbox.state().pacing.feed_failed(
                &GithubReadError {
                    status: None,
                    message: format!("{error:?}"),
                    retry_after_s: None,
                    rate_limit: None,
                },
                now_ms,
            );
            return;
        }
    };
    let etag = inbox.state().feed.events_etag.clone();
    tally.requests += 1;
    let answer = reader
        .get(&paths::events(&inbox.repo), etag.as_deref())
        .await;
    let mut state = inbox.state();
    match answer {
        Err(error) => {
            tally.observe(error.rate_limit);
            state.pacing.feed_failed(&error, now_ms);
            read_failed(inbox, "the events feed", &error);
        }
        Ok(answer) => {
            tally.observe(answer.rate_limit());
            state.pacing.feed_answered(&answer, now_ms);
            let ConditionalAnswer::Modified { body, etag, .. } = answer else {
                return;
            };
            match parse_events(&body) {
                Ok(events) => {
                    state.feed.events_etag = etag;
                    absorb_events(&mut state, events);
                }
                // The ETag is kept as it was, so the next poll reads the body again.
                Err(message) => log::warn!(
                    "GitHub inbox: events of {}/{} unreadable: {message}",
                    inbox.repo.owner,
                    inbox.repo.name
                ),
            }
        }
    }
}

/// Invalidate for the events not seen before, oldest first. The first answer is the baseline:
/// what already happened is not news.
fn absorb_events(state: &mut RepoState, newest_first: Vec<FeedEvent>) {
    let ids: HashSet<String> = newest_first.iter().map(|e| e.id.clone()).collect();
    let Some(seen) = state.feed.seen_events.replace(ids) else {
        return;
    };
    let news: Vec<Change> = newest_first
        .into_iter()
        .rev()
        .filter(|event| !seen.contains(&event.id))
        .filter_map(|event| event.change)
        .collect();
    invalidate_for(state, &news);
}

async fn poll_pending_checks(inbox: &RepoInbox, now_ms: u64, tally: &mut Tally) {
    let pending = inbox.state().pending_check_rows();
    if pending.is_empty() {
        return;
    }
    inbox.state().pacing.pending_checks_polled(now_ms);
    for key in pending {
        let _turn = inbox.take_turn(&key).await;
        match inbox.revalidate(&key, tally).await {
            Ok((_, true)) => inbox.state().announce_changed(&key),
            Ok((_, false)) => {}
            Err(error) => log::warn!(
                "GitHub inbox: checks {key:?} of {}/{} unread: {error:?}",
                inbox.repo.owner,
                inbox.repo.name
            ),
        }
    }
}

/// Catch up on what the events feed misses: when the recently-updated pulls list has changed,
/// every cached row of a pull request on it is invalidated.
async fn sweep_updated_pulls(inbox: &RepoInbox, now_ms: u64, tally: &mut Tally) {
    inbox.state().pacing.swept(now_ms);
    let Ok(reader) = inbox.readers().reader_for(&inbox.repo) else {
        return;
    };
    let etag = inbox.state().feed.sweep_etag.clone();
    tally.requests += 1;
    let answer = reader
        .get(&paths::updated_pulls(&inbox.repo), etag.as_deref())
        .await;
    let mut state = inbox.state();
    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => {
            tally.observe(error.rate_limit);
            state.pacing.observe_rate_limit(error.rate_limit);
            read_failed(inbox, "the updated-pulls sweep", &error);
            return;
        }
    };
    tally.observe(answer.rate_limit());
    state.pacing.observe_rate_limit(answer.rate_limit());
    let ConditionalAnswer::Modified { body, etag, .. } = answer else {
        return;
    };
    let pulls: Vec<(u64, Option<String>)> = match serde_json::from_str::<Value>(&body) {
        Ok(Value::Array(items)) => items
            .iter()
            .filter_map(|pr| {
                Some((
                    pr.get("number")?.as_u64()?,
                    pr.pointer("/head/ref")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                ))
            })
            .collect(),
        _ => {
            log::warn!(
                "GitHub inbox: the updated pulls of {}/{} are unreadable",
                inbox.repo.owner,
                inbox.repo.name
            );
            return;
        }
    };
    state.feed.sweep_etag = etag;
    let mut if_cached = Vec::new();
    for (number, head) in pulls {
        push_unique(&mut if_cached, RowKey::Pr { number });
        push_unique(&mut if_cached, RowKey::Conversation { number });
        if let Some(branch) = head {
            push_unique(&mut if_cached, RowKey::PrForHead { branch });
        }
        for key in state.commit_rows_of(number) {
            push_unique(&mut if_cached, key);
        }
    }
    state.invalidate(&[], &if_cached);
}
