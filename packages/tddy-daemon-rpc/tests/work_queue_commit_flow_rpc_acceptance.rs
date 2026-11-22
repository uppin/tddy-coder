//! Acceptance: an item's commit inbox and outbox, and the counts a queue row shows, over the daemon's
//! RPCs (`#workqueue` 11/14).
//!
//! The queue is seeded into the host's store with planned bases (the test host has no GitHub, so the
//! answer says the planned bases stood in); its owning session's checkout is a clone of a bare origin
//! that holds the items' branches. The analysis itself is proved on real repositories by
//! `tddy-work-queue`'s `commit_flow_acceptance`; here the wire: who is addressed, what is refused.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::work_queue::{
    GetWorkItemCommitFlowRequest, GetWorkQueueGraphRequest, WorkQueueAddress, WorkQueueService,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-commit-flow";

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

fn commit(dir: &Path, file: &str, content: &str, subject: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", subject]);
}

fn an_item(id: &str, branch: Option<&str>, parents: &[&str]) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: format!("Item {id}"),
        branch: branch.map(str::to_string),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..WorkItem::default()
    }
}

struct AQueueOverRealBranches {
    host: Host,
    address: WorkQueueAddress,
    /// The bare origin's scratch clone a test pushes to.
    work: PathBuf,
    _repos: tempfile::TempDir,
}

/// n1 (feat/one) ← n2 (feat/two) on the trunk, with n3 planned but branchless; the owning session's
/// checkout is a fresh clone of the origin holding both branches.
fn a_queue_over_real_branches() -> AQueueOverRealBranches {
    let repos = tempfile::tempdir().unwrap();
    let origin = repos.path().join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--bare", "-b", "main"]);
    git(repos.path(), &["clone", origin.to_str().unwrap(), "work"]);
    let work = repos.path().join("work");
    git(&work, &["checkout", "-b", "main"]);
    commit(&work, "f.txt", "base\n", "base");
    git(&work, &["push", "-u", "origin", "main"]);
    for (branch, file) in [("feat/one", "one.txt"), ("feat/two", "two.txt")] {
        git(&work, &["checkout", "-b", branch]);
        commit(&work, file, "x\n", branch);
        git(&work, &["push", "-u", "origin", branch]);
    }
    git(
        repos.path(),
        &["clone", origin.to_str().unwrap(), "checkout"],
    );

    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let sessions_base = host.sessions.path().join("sessions");
    let queues = SessionWorkQueues::new(sessions_base.clone());
    queues
        .create_for_session(OWNER, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    queues
        .update(OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![
                    an_item("n1", Some("feat/one"), &[]),
                    an_item("n2", Some("feat/two"), &["n1"]),
                    an_item("n3", None, &["n2"]),
                ],
            }
        })
        .expect("the queue is filled");
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), OWNER);
    std::fs::create_dir_all(&session_dir).expect("owner session dir");
    tddy_changeset::write_initial_tool_session_metadata(
        &session_dir,
        tddy_changeset::InitialToolSessionMetadataOpts {
            project_id: THIS_PROJECT.to_string(),
            repo_path: Some(repos.path().join("checkout").to_string_lossy().into_owned()),
            ..Default::default()
        },
    )
    .expect("the owner's metadata names its checkout");
    let address = WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: OWNER.to_string(),
        daemon_instance_id: HOST_A.to_string(),
    };
    AQueueOverRealBranches {
        host,
        address,
        work,
        _repos: repos,
    }
}

impl AQueueOverRealBranches {
    /// A commit pushed to `branch` after the branches above it were cut.
    fn push_to(&self, branch: &str, file: &str, content: &str, subject: &str) {
        git(&self.work, &["checkout", branch]);
        commit(&self.work, file, content, subject);
        git(&self.work, &["push", "origin", branch]);
    }

    fn request(&self, item: &str) -> GetWorkItemCommitFlowRequest {
        GetWorkItemCommitFlowRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(self.address.clone()),
            item_id: item.to_string(),
        }
    }
}

#[tokio::test]
async fn an_items_inbox_lists_what_an_ancestor_pushed_after_the_cut_attributed_to_it() {
    // Given
    let queue = a_queue_over_real_branches();
    queue.push_to("feat/one", "one.txt", "fix\n", "fix one");

    // When
    let inbox = queue
        .host
        .daemon
        .get_work_item_inbox(Request::direct(queue.request("n2")))
        .await
        .expect("inbox")
        .into_inner();

    // Then — the planned bases stood in, and the answer says so
    assert_eq!(
        (
            inbox
                .commits
                .iter()
                .map(|c| (
                    c.subject.as_str(),
                    c.source_kind.as_str(),
                    c.source.as_str()
                ))
                .collect::<Vec<_>>(),
            inbox.unavailable_reason.is_empty(),
        ),
        (vec![("fix one", "item", "n1")], false)
    );
}

#[tokio::test]
async fn an_items_outbox_lists_what_each_successor_lacks() {
    // Given
    let queue = a_queue_over_real_branches();
    queue.push_to("feat/one", "one.txt", "fix\n", "fix one");

    // When
    let outbox = queue
        .host
        .daemon
        .get_work_item_outbox(Request::direct(queue.request("n1")))
        .await
        .expect("outbox")
        .into_inner();

    // Then
    assert_eq!(
        outbox
            .successors
            .iter()
            .map(|s| {
                (
                    s.successor.as_str(),
                    s.commits
                        .iter()
                        .map(|c| c.subject.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
        vec![("n2", vec!["fix one"])]
    );
}

#[tokio::test]
async fn the_queue_rows_counts_cover_every_item_with_a_branch() {
    // Given
    let queue = a_queue_over_real_branches();
    queue.push_to("feat/one", "one.txt", "fix\n", "fix one");

    // When
    let counts = queue
        .host
        .daemon
        .get_work_queue_commit_counts(Request::direct(GetWorkQueueGraphRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(queue.address.clone()),
        }))
        .await
        .expect("counts")
        .into_inner();

    // Then — n3 owns no branch yet, so it has no row counts; n1 owes n2 one commit
    assert_eq!(
        counts
            .items
            .iter()
            .map(|c| (
                c.item_id.as_str(),
                c.inbox_count,
                c.outbox_count,
                c.conflict_count
            ))
            .collect::<Vec<_>>(),
        vec![("n1", 0, 1, 0), ("n2", 1, 0, 0)]
    );
}

#[tokio::test]
async fn an_item_without_a_branch_or_an_unknown_item_is_refused_naming_why() {
    // Given
    let queue = a_queue_over_real_branches();

    // When
    let refusals = [
        queue
            .host
            .daemon
            .get_work_item_inbox(Request::direct(queue.request("n3")))
            .await,
        queue
            .host
            .daemon
            .get_work_item_inbox(Request::direct(queue.request("n9")))
            .await,
    ]
    .map(|r| r.expect_err("refused"));

    // Then
    assert_eq!(
        (
            refusals[0].code(),
            refusals[0].message().contains("has no branch"),
            refusals[1].code(),
        ),
        (Code::NotFound, true, Code::NotFound)
    );
}
