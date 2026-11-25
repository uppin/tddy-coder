//! Unit tests: `StreamWorkQueueStatus` re-reads when its queue changes, not only when GitHub does — an
//! item added by anyone gets its status row (its wave, its PR state) at once.
//!
//! The handler reads GitHub through a stand-in site whose reader is never asked anything: the items
//! here carry no branch, so their status needs no request.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError};
use tddy_github_inbox::{GithubInbox, InboxError, RepoReaders, RepoSlug};
use tddy_rpc::Request;
use tddy_service::proto::work_queue::{
    StreamWorkQueueStatusRequest, WorkQueueAddress, WorkQueueStatusView,
};
use tddy_worktree_service::pull_request_view::{
    PullRequestSite, PullRequestSummary, SessionPullRequests,
};

use super::add_work_item_unit_tests::make_unit_connection;
use super::WorkQueueRpcHandler;

const PROJECT: &str = "proj-wake";
const OWNER: &str = "queue-owner-wake";

/// GitHub that is never asked: an item with no branch needs no request.
struct NeverAsked;

#[async_trait]
impl ConditionalRead for NeverAsked {
    async fn get(
        &self,
        path: &str,
        _if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        panic!("GitHub was asked for {path}")
    }
}

impl RepoReaders for NeverAsked {
    fn reader_for(&self, _repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        Ok(Arc::new(NeverAsked))
    }
}

/// Every project's PRs live in `acme/app`, read through an inbox over [`NeverAsked`].
struct ASite {
    inbox: Arc<GithubInbox>,
}

impl SessionPullRequests for ASite {
    fn site_for_session(
        &self,
        _os_user: &str,
        _session_id: &str,
        _project_id: &str,
        _session_token: &str,
    ) -> Result<PullRequestSite, tddy_rpc::Status> {
        Ok(PullRequestSite {
            inbox: Arc::clone(&self.inbox),
            repo: RepoSlug {
                owner: "acme".to_string(),
                name: "app".to_string(),
            },
        })
    }

    fn cached_summary(
        &self,
        _os_user: &str,
        _project_id: &str,
        _branch: &str,
    ) -> Option<PullRequestSummary> {
        None
    }
}

fn an_item(id: &str) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: format!("item {id}"),
        ..WorkItem::default()
    }
}

fn item_ids(view: &WorkQueueStatusView) -> Vec<String> {
    view.items.iter().map(|item| item.item_id.clone()).collect()
}

#[tokio::test]
async fn the_status_stream_sends_a_new_snapshot_when_its_queue_changes() {
    // Given — a queue holding n1, and its status stream open
    let temp = tempfile::tempdir().unwrap();
    let mut handler =
        WorkQueueRpcHandler::from_host(&make_unit_connection(temp.path().to_path_buf()));
    handler.pull_requests = Arc::new(ASite {
        inbox: Arc::new(GithubInbox::new(Arc::new(NeverAsked))),
    });
    let queues = SessionWorkQueues::new(handler.sessions_base("u").unwrap().join("sessions"));
    queues.create_for_session(OWNER, PROJECT, 0).unwrap();
    queues
        .update(OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![an_item("n1")],
                ..Default::default()
            }
        })
        .unwrap();
    let mut stream = handler
        .stream_work_queue_status_rpc(Request::direct(StreamWorkQueueStatusRequest {
            session_token: "valid".to_string(),
            queue: Some(WorkQueueAddress {
                project_id: PROJECT.to_string(),
                queue_id: OWNER.to_string(),
                daemon_instance_id: String::new(),
            }),
        }))
        .await
        .expect("the stream opens")
        .into_inner();
    let first = stream.next().await.expect("a frame").expect("a status");

    // When — an item is added to the queue
    queues
        .update(OWNER, |queue| queue.nodes.push(an_item("n2")))
        .unwrap();

    // Then — a new snapshot carries it, with no GitHub event behind it. The stream sends a snapshot
    // on every wake and does not dedupe them, so a wake that lands before the change (a watcher
    // event for the queue's own record) may first resend the queue as it was. Each such snapshot
    // must still be exactly the state before the change: read on until the one carrying n2, within
    // one deadline for them all.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut before_the_change = Vec::new();
    let after_the_change = loop {
        let frame = tokio::time::timeout_at(deadline, stream.next())
            .await
            .unwrap_or_else(|_| {
                panic!("a snapshot carrying n2 follows the change; got only {before_the_change:?}")
            })
            .expect("a frame")
            .expect("a status");
        let ids = item_ids(&frame);
        if ids.iter().any(|id| id == "n2") {
            break ids;
        }
        before_the_change.push(ids);
    };
    let unchanged = vec!["n1".to_string()];
    assert_eq!(
        (
            item_ids(&first),
            before_the_change
                .iter()
                .filter(|ids| **ids != unchanged)
                .collect::<Vec<_>>(),
            after_the_change
        ),
        (
            unchanged.clone(),
            Vec::<&Vec<String>>::new(),
            vec!["n1".to_string(), "n2".to_string()]
        )
    );
}
