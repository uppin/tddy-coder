//! The notification relay's own guarantees, over a real log on a real data directory with a fake
//! transport (n5, cross-host watches): what it sends to which host, that a host's cursor moves only
//! when the host acknowledges, that a resend after a lost acknowledgement appends nothing twice, that
//! its cursors survive a restart, that a relayed record never travels a second hop, and that an
//! eviction before relaying is reported as lost and never listed.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions-on-other-hosts

mod project_notification_log_support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, sessions_of, write_session};
use tddy_session_activity::notification_relay::{
    NotificationRelay, RelayAck, RelayPorts, RelayRequest,
};
use tddy_session_activity::project_notification_log::{
    record_from_json, ProjectNotificationLog, ProjectNotificationScope, RelayedBatch,
    PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const THIS_HOST: &str = "host-b";
const ORCHESTRATORS_HOST: &str = "host-a";
const ORCHESTRATOR: &str = "01900000-0000-7000-8000-AABB00000500";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000501";
const LOCAL_SESSION: &str = "01900000-0000-7000-8000-AABB00000502";
const BOUND: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_millis(20);

/// A transport that records every batch, and answers or fails as told.
#[derive(Default)]
struct FakePeers {
    sent: Mutex<Vec<(String, RelayRequest)>>,
    down: Mutex<bool>,
}

/// The relay's ports over [`FakePeers`] and a data directory.
struct FakePorts(Arc<FakePeers>, PathBuf);

#[async_trait]
impl RelayPorts for FakePorts {
    fn local_instance_id(&self) -> String {
        THIS_HOST.to_string()
    }

    fn github_user_of(&self, os_user: &str) -> Option<String> {
        Some(format!("gh-{os_user}"))
    }

    fn sessions_base_for(&self, os_user: &str) -> Option<PathBuf> {
        Some(sessions_of(&self.1, os_user))
    }

    async fn relay(&self, host: &str, request: &RelayRequest) -> Result<RelayAck, String> {
        self.0
            .sent
            .lock()
            .unwrap()
            .push((host.to_string(), request.clone()));
        if *self.0.down.lock().unwrap() {
            return Err(format!("the link to {host} is down"));
        }
        Ok(RelayAck {
            received_through_seq: request.through_seq,
        })
    }
}

struct World {
    data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
    peers: Arc<FakePeers>,
}

/// A host holding a child whose orchestrator runs on another host, and a session nobody started.
fn a_world() -> World {
    let data_dir = tempfile::tempdir().unwrap();
    let dir = data_dir.path();
    write_session(dir, ADA, CHILD, PROJECT);
    write_session(dir, ADA, LOCAL_SESSION, PROJECT);
    let child_dir = sessions_of(dir, ADA).join("sessions").join(CHILD);
    let mut changeset = tddy_core::read_changeset(&child_dir).unwrap_or_default();
    changeset.orchestrator_session_id = Some(ORCHESTRATOR.to_string());
    changeset.orchestrator_daemon_instance_id = Some(ORCHESTRATORS_HOST.to_string());
    tddy_core::write_changeset(&child_dir, &changeset).unwrap();
    let (log, bus) = a_log_over(dir);
    World {
        data_dir,
        log,
        bus,
        peers: Arc::new(FakePeers::default()),
    }
}

impl World {
    fn relay(&self) -> Arc<NotificationRelay> {
        Arc::new(
            NotificationRelay::open(
                Arc::clone(&self.log),
                Arc::new(FakePorts(
                    Arc::clone(&self.peers),
                    self.data_dir.path().to_path_buf(),
                )),
                &self.data_dir.path().join("relay"),
            )
            .expect("the relay opens"),
        )
    }

    fn start(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(self.relay().run(RETRY))
    }

    async fn a_turn_ends_in(&self, session_id: &str, n: usize) {
        self.bus
            .publish(SessionNotification {
                session_id: session_id.to_string(),
                os_user: ADA.to_string(),
                label: "s".to_string(),
                kind: SessionNotificationKind::AttentionRequired,
                source: SessionNotificationSource::ActivityStatus,
                text: format!("turn {n}"),
                agent: None,
                question: None,
                ended: None,
                activity_status: Some("Done".to_string()),
                message: None,
                at_unix_ms: 1_793_000_000_000,
            })
            .await;
    }

    /// The texts of every record sent to `host`, in the order sent, resends included.
    fn texts_sent_to(&self, host: &str) -> Vec<String> {
        self.peers
            .sent
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, _)| to == host)
            .flat_map(|(_, request)| request.records.iter())
            .map(|relayed| record_from_json(&relayed.record_json).unwrap().text)
            .collect()
    }

    async fn until_sent(&self, host: &str, count: usize) -> Vec<String> {
        tokio::time::timeout(BOUND, async {
            loop {
                let sent = self.texts_sent_to(host);
                if sent.len() >= count {
                    return sent;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{count} records were never sent to {host}: {:?}",
                self.texts_sent_to(host)
            )
        })
    }
}

#[tokio::test]
async fn a_childs_records_go_to_its_orchestrators_host_and_no_others() {
    // Given a relay on the child's host
    let world = a_world();
    let relay = world.start();

    // When the child and a session nobody started each finish a turn
    world.a_turn_ends_in(LOCAL_SESSION, 0).await;
    world.a_turn_ends_in(CHILD, 1).await;

    // Then only the child's record goes, to its orchestrator's host, relayed for the orchestrator
    let sent = world.until_sent(ORCHESTRATORS_HOST, 1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    relay.abort();
    let requests = world.peers.sent.lock().unwrap().clone();
    assert_eq!(
        (
            sent,
            requests.len(),
            requests[0].1.records[0].anchor_session_id.clone(),
            requests[0].1.github_user.clone(),
            requests[0].1.origin_daemon_instance_id.clone(),
        ),
        (
            vec!["turn 1".to_string()],
            1,
            ORCHESTRATOR.to_string(),
            "gh-ada".to_string(),
            THIS_HOST.to_string(),
        )
    );
}

#[tokio::test]
async fn a_host_that_did_not_take_its_batch_is_sent_it_again_and_the_cursor_waits() {
    // Given the orchestrator's host is unreachable
    let world = a_world();
    *world.peers.down.lock().unwrap() = true;
    let relay = world.start();

    // When the child finishes two turns, and the link comes back after a few retries
    world.a_turn_ends_in(CHILD, 1).await;
    world.a_turn_ends_in(CHILD, 2).await;
    world.until_sent(ORCHESTRATORS_HOST, 4).await;
    *world.peers.down.lock().unwrap() = false;
    let before = world.peers.sent.lock().unwrap().len();

    // Then the next attempt carries both records again, and after it is taken nothing more is sent
    tokio::time::timeout(BOUND, async {
        while world.peers.sent.lock().unwrap().len() == before {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the relay retried");
    tokio::time::sleep(Duration::from_millis(150)).await;
    relay.abort();
    let sent = world.peers.sent.lock().unwrap().clone();
    let last = &sent.last().unwrap().1;
    assert_eq!(
        (
            last.records
                .iter()
                .map(|r| record_from_json(&r.record_json).unwrap().text)
                .collect::<Vec<_>>(),
            sent.len(),
        ),
        (vec!["turn 1".to_string(), "turn 2".to_string()], before + 1)
    );
}

#[tokio::test]
async fn a_restarted_relay_resumes_after_what_was_taken() {
    // Given a relay that relayed one record and stopped
    let world = a_world();
    let relay = world.start();
    world.a_turn_ends_in(CHILD, 1).await;
    world.until_sent(ORCHESTRATORS_HOST, 1).await;
    relay.abort();

    // When another record is recorded while it is stopped, and a new relay starts over the same state
    world.a_turn_ends_in(CHILD, 2).await;
    let restarted = world.start();

    // Then only the new record is sent
    let sent = world.until_sent(ORCHESTRATORS_HOST, 2).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    restarted.abort();
    assert_eq!(
        world.texts_sent_to(ORCHESTRATORS_HOST),
        vec!["turn 1".to_string(), "turn 2".to_string()],
        "sent: {sent:?}"
    );
}

#[tokio::test]
async fn a_relayed_record_is_never_relayed_again() {
    // Given a relay, and a record of another host's child — whose orchestrator is named here as on a
    // third host — appended as relayed
    let world = a_world();
    let relay = world.start();
    let foreign = tddy_session_activity::project_notification_log::ProjectNotificationRecord {
        seq: 7,
        session_id: CHILD.to_string(),
        label: "s".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: "relayed".to_string(),
        agent: None,
        question: None,
        ended: None,
        activity_status: Some("Done".to_string()),
        message: None,
        at_unix_ms: 1,
        orchestrator_session_id: Some(ORCHESTRATOR.to_string()),
        daemon_instance_id: None,
        origin_seq: None,
        relay_lost: 0,
    };
    world
        .log
        .append_relayed(RelayedBatch {
            origin: "host-c".to_string(),
            project_id: PROJECT.to_string(),
            os_user: ADA.to_string(),
            records: vec![foreign],
            lost: 0,
            through_seq: 7,
        })
        .unwrap();

    // Then nothing is sent
    tokio::time::sleep(Duration::from_millis(200)).await;
    relay.abort();
    assert_eq!(world.peers.sent.lock().unwrap().len(), 0);
}

fn a_record(
    seq: u64,
    text: &str,
) -> tddy_session_activity::project_notification_log::ProjectNotificationRecord {
    tddy_session_activity::project_notification_log::ProjectNotificationRecord {
        seq,
        session_id: CHILD.to_string(),
        label: "child".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: text.to_string(),
        agent: None,
        question: None,
        ended: None,
        activity_status: Some("Done".to_string()),
        message: None,
        at_unix_ms: 1,
        orchestrator_session_id: Some(ORCHESTRATOR.to_string()),
        daemon_instance_id: None,
        origin_seq: None,
        relay_lost: 0,
    }
}

fn a_batch(
    records: Vec<tddy_session_activity::project_notification_log::ProjectNotificationRecord>,
    lost: u64,
) -> RelayedBatch {
    let through_seq = records.last().map_or(0, |r| r.seq);
    RelayedBatch {
        origin: ORCHESTRATORS_HOST.to_string(),
        project_id: PROJECT.to_string(),
        os_user: ADA.to_string(),
        records,
        lost,
        through_seq,
    }
}

fn everything(log: &ProjectNotificationLog) -> Vec<String> {
    log.read(
        &ProjectNotificationScope {
            project_id: PROJECT.to_string(),
            os_user: ADA.to_string(),
            exclude_session_id: String::new(),
            started_by: None,
        },
        "0",
        None,
    )
    .unwrap()
    .page
    .items
    .into_iter()
    .map(|r| {
        format!(
            "{}@{}#{}",
            r.text,
            r.daemon_instance_id.unwrap_or_default(),
            r.origin_seq.unwrap_or_default()
        )
    })
    .collect()
}

#[tokio::test]
async fn a_batch_relayed_again_appends_nothing_twice_and_survives_a_reopen() {
    // Given a log that took a batch of two records
    let data_dir = tempfile::tempdir().unwrap();
    let (log, _bus) = a_log_over(data_dir.path());
    let first = log
        .append_relayed(a_batch(vec![a_record(4, "a"), a_record(5, "b")], 0))
        .unwrap();

    // When the same batch arrives again, then a longer one, and the log is reopened and sent it
    // once more
    let again = log
        .append_relayed(a_batch(vec![a_record(4, "a"), a_record(5, "b")], 0))
        .unwrap();
    log.append_relayed(a_batch(vec![a_record(5, "b"), a_record(6, "c")], 0))
        .unwrap();
    drop(log);
    let (reopened, _bus) = a_log_over(data_dir.path());
    let after_reopen = reopened
        .append_relayed(a_batch(vec![a_record(6, "c")], 0))
        .unwrap();

    // Then each record is held once, naming its host and origin seq
    assert_eq!(
        (
            first.received_through,
            again.appended.len(),
            after_reopen.appended.len(),
            after_reopen.received_through,
            everything(&reopened),
        ),
        (
            5,
            0,
            0,
            6,
            vec![
                "a@host-a#4".to_string(),
                "b@host-a#5".to_string(),
                "c@host-a#6".to_string()
            ],
        )
    );
}

#[tokio::test]
async fn a_relay_gap_is_held_but_never_listed() {
    // Given a log
    let data_dir = tempfile::tempdir().unwrap();
    let (log, _bus) = a_log_over(data_dir.path());

    // When a batch arrives saying three records were evicted before it was relayed
    let appended = log
        .append_relayed(a_batch(vec![a_record(10, "after the gap")], 3))
        .unwrap();

    // Then the gap is held, counting three, and a read lists only the record
    assert_eq!(
        (
            appended
                .appended
                .iter()
                .map(|r| r.relay_lost)
                .collect::<Vec<_>>(),
            everything(&log),
        ),
        (vec![3, 0], vec!["after the gap@host-a#10".to_string()])
    );
}

#[tokio::test]
async fn records_evicted_before_a_down_host_came_back_are_reported_lost() {
    // Given a relay whose host is down after taking one record
    let world = a_world();
    let relay = world.start();
    world.a_turn_ends_in(CHILD, 0).await;
    world.until_sent(ORCHESTRATORS_HOST, 1).await;
    *world.peers.down.lock().unwrap() = true;

    // When more records than the window keeps are recorded, and the host comes back
    for n in 1..=PROJECT_NOTIFICATION_CAPACITY + 3 {
        world.a_turn_ends_in(CHILD, n).await;
    }
    *world.peers.down.lock().unwrap() = false;
    tokio::time::timeout(BOUND, async {
        loop {
            let taken = world
                .peers
                .sent
                .lock()
                .unwrap()
                .iter()
                .any(|(_, request)| request.lost > 0);
            if taken {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a batch reported what was lost");
    relay.abort();

    // Then the batch the host took after the gap says how many were evicted after its cursor
    let lost: Option<u64> = world
        .peers
        .sent
        .lock()
        .unwrap()
        .iter()
        .map(|(_, request)| request.lost)
        .max();
    assert_eq!(lost, Some(3));
}

#[tokio::test]
async fn records_no_host_was_owed_are_never_counted_lost() {
    // Given a relay that relayed one record of the child
    let world = a_world();
    let relay = world.start();
    world.a_turn_ends_in(CHILD, 0).await;
    world.until_sent(ORCHESTRATORS_HOST, 1).await;

    // When more records than the window keeps arrive of a session no other host is owed, and then
    // another of the child
    for n in 1..=PROJECT_NOTIFICATION_CAPACITY + 50 {
        world.a_turn_ends_in(LOCAL_SESSION, n).await;
    }
    world.a_turn_ends_in(CHILD, 1).await;
    world.until_sent(ORCHESTRATORS_HOST, 2).await;
    relay.abort();

    // Then nothing is reported lost: the evicted records were never owed to the orchestrator's host
    let lost: Vec<u64> = world
        .peers
        .sent
        .lock()
        .unwrap()
        .iter()
        .map(|(_, request)| request.lost)
        .collect();
    assert_eq!(lost, vec![0, 0]);
}
