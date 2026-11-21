//! Acceptance: the daemon watches each project's remote trunk centrally — one poller however many
//! orchestrators of the project subscribe — publishes each new trunk commit as an activity of every
//! subscribed orchestrator's own session, and stops polling once no live session is subscribed
//! (`#orchestra` 5/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Watching sessions.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_daemon_kernel::session_notification::SessionNotificationSource;
use tddy_github::trunk_feed::{
    Conditional, RepoSlug, TrunkCommit, TrunkFeed, TrunkPoller, TrunkReadError,
    DEFAULT_POLL_INTERVAL_MS,
};
use tddy_session_activity::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotificationBus,
};
use tddy_session_activity::trunk_watch::{
    SubscriberLiveness, TrunkSource, TrunkSubscription, TrunkWatcher,
};

const PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3a";
const ORCHESTRATOR_A: &str = "01a1fdd8-0000-7000-8000-00000000000a";
const ORCHESTRATOR_B: &str = "01a1fdd8-0000-7000-8000-00000000000b";

type Answer = Result<Conditional<Vec<TrunkCommit>>, TrunkReadError>;

fn commit(sha: &str, parent: &str, subject: &str) -> TrunkCommit {
    TrunkCommit {
        sha: sha.to_string(),
        author: "dev".to_string(),
        subject: subject.to_string(),
        committed_at_unix_ms: 0,
        parent: parent.to_string(),
    }
}

/// One project's GitHub: answers from a script, counting the reads.
#[derive(Default)]
struct AGitHub {
    answers: Mutex<VecDeque<Answer>>,
    reads: Mutex<usize>,
}

#[async_trait]
impl TrunkFeed for AGitHub {
    async fn newest_commits(&self, _repo: &RepoSlug, _branch: &str, _etag: Option<&str>) -> Answer {
        *self.reads.lock().unwrap() += 1;
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(Conditional::NotModified {
                poll_interval_ms: None,
            }))
    }
}

/// The host: one project's trunk, and which sessions are running.
struct AHost {
    github: Arc<AGitHub>,
    pollers_built: Mutex<usize>,
    live: Mutex<HashSet<String>>,
}

impl TrunkSource for AHost {
    fn poller_for(
        &self,
        _subscription: &TrunkSubscription,
    ) -> Result<(TrunkPoller, String), String> {
        *self.pollers_built.lock().unwrap() += 1;
        Ok((
            TrunkPoller::new(
                Arc::clone(&self.github) as Arc<dyn TrunkFeed>,
                RepoSlug {
                    owner: "acme".to_string(),
                    name: "app".to_string(),
                },
                "main".to_string(),
            ),
            "origin/main".to_string(),
        ))
    }
}

impl SubscriberLiveness for AHost {
    fn is_live(&self, session_id: &str) -> bool {
        self.live.lock().unwrap().contains(session_id)
    }
}

struct World {
    host: Arc<AHost>,
    published: Arc<RecordingSessionNotificationSubscriber>,
    watcher: TrunkWatcher,
}

/// A watcher over a trunk whose answers are `answers` (baseline first).
fn a_world(answers: Vec<Answer>) -> World {
    let host = Arc::new(AHost {
        github: Arc::new(AGitHub {
            answers: Mutex::new(answers.into()),
            reads: Mutex::default(),
        }),
        pollers_built: Mutex::default(),
        live: Mutex::default(),
    });
    let published = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&published)));
    let watcher = TrunkWatcher::new(
        Arc::clone(&host) as Arc<dyn TrunkSource>,
        Arc::clone(&host) as Arc<dyn SubscriberLiveness>,
        bus,
    );
    World {
        host,
        published,
        watcher,
    }
}

impl World {
    fn a_live_orchestrator_subscribes(&self, session_id: &str) {
        self.host
            .live
            .lock()
            .unwrap()
            .insert(session_id.to_string());
        self.watcher
            .subscribe(TrunkSubscription {
                project_id: PROJECT.to_string(),
                os_user: "dev".to_string(),
                session_id: session_id.to_string(),
            })
            .expect("the trunk can be watched");
    }

    fn stops(&self, session_id: &str) {
        self.host.live.lock().unwrap().remove(session_id);
    }

    /// The trunk activities published, as (session, text).
    fn trunk_activities(&self) -> Vec<(String, String)> {
        self.published
            .received()
            .into_iter()
            .filter(|n| n.source == SessionNotificationSource::TrunkCommit)
            .map(|n| (n.session_id, n.text))
            .collect()
    }
}

#[tokio::test]
async fn two_live_orchestrators_of_one_project_share_one_poller() {
    // Given two orchestrators of one project
    let world = a_world(vec![]);

    // When both subscribe and the watcher steps
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_A);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_B);
    world.watcher.step(0).await;

    // Then one poller serves them, reading once
    assert_eq!(world.watcher.live_pollers(), vec![PROJECT.to_string()]);
    assert_eq!(
        (
            *world.host.pollers_built.lock().unwrap(),
            *world.host.github.reads.lock().unwrap()
        ),
        (1, 1)
    );
}

#[tokio::test]
async fn a_new_trunk_commit_lands_as_an_activity_of_every_subscribed_orchestrator() {
    // Given two subscribed orchestrators and a baseline at a
    let world = a_world(vec![
        Ok(Conditional::Modified {
            value: vec![commit("a", "", "init")],
            etag: Some("e1".to_string()),
            poll_interval_ms: None,
        }),
        Ok(Conditional::Modified {
            value: vec![commit("b", "a", "Add the outbox"), commit("a", "", "init")],
            etag: Some("e2".to_string()),
            poll_interval_ms: None,
        }),
    ]);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_A);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_B);
    world.watcher.step(0).await;

    // When a commit lands and the next poll is due
    world.watcher.step(DEFAULT_POLL_INTERVAL_MS).await;

    // Then each orchestrator has it as an activity of its own session, naming the trunk and subject
    let activities = world.trunk_activities();
    let sessions: Vec<&str> = activities.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(sessions, vec![ORCHESTRATOR_A, ORCHESTRATOR_B]);
    assert!(
        activities
            .iter()
            .all(|(_, text)| text.contains("origin/main") && text.contains("Add the outbox")),
        "{activities:?}"
    );
}

#[tokio::test]
async fn the_poller_stops_when_the_last_subscribed_orchestrator_stops() {
    // Given one subscribed orchestrator
    let world = a_world(vec![]);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_A);
    world.watcher.step(0).await;

    // When it stops and the watcher steps
    world.stops(ORCHESTRATOR_A);
    let next = world.watcher.step(DEFAULT_POLL_INTERVAL_MS).await;

    // Then no poller is left, nothing is due, and the trunk was not read again
    assert_eq!(world.watcher.live_pollers(), Vec::<String>::new());
    assert_eq!(next, None);
    assert_eq!(*world.host.github.reads.lock().unwrap(), 1);
}

#[tokio::test]
async fn an_unsubscribed_orchestrator_gets_no_more_trunk_activities() {
    // Given two subscribed orchestrators and a baseline
    let world = a_world(vec![
        Ok(Conditional::Modified {
            value: vec![commit("a", "", "init")],
            etag: None,
            poll_interval_ms: None,
        }),
        Ok(Conditional::Modified {
            value: vec![commit("b", "a", "Fix CI"), commit("a", "", "init")],
            etag: None,
            poll_interval_ms: None,
        }),
    ]);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_A);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_B);
    world.watcher.step(0).await;

    // When one unsubscribes and a commit lands
    world.watcher.unsubscribe(ORCHESTRATOR_A);
    world.watcher.step(DEFAULT_POLL_INTERVAL_MS).await;

    // Then only the other gets it
    let sessions: Vec<String> = world
        .trunk_activities()
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert_eq!(sessions, vec![ORCHESTRATOR_B.to_string()]);
}

#[tokio::test]
async fn a_forced_push_is_reported_as_a_rewritten_trunk() {
    // Given a subscribed orchestrator and a baseline at c
    let world = a_world(vec![
        Ok(Conditional::Modified {
            value: vec![commit("c", "b", "three"), commit("b", "a", "two")],
            etag: None,
            poll_interval_ms: None,
        }),
        Ok(Conditional::Modified {
            value: vec![commit("x", "a", "rewritten")],
            etag: None,
            poll_interval_ms: None,
        }),
    ]);
    world.a_live_orchestrator_subscribes(ORCHESTRATOR_A);
    world.watcher.step(0).await;

    // When the trunk is force-pushed
    world.watcher.step(DEFAULT_POLL_INTERVAL_MS).await;

    // Then the activity says the trunk was rewritten, from c to x
    let activities = world.trunk_activities();
    assert_eq!(activities.len(), 1, "{activities:?}");
    let text = &activities[0].1;
    assert!(
        text.contains("rewritten") && text.contains('c') && text.contains('x'),
        "{text}"
    );
}
