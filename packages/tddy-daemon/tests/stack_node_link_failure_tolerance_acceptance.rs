//! Acceptance: a failed pr-stack node link does not fail the spawn (D36).
//!
//! The link lands *after* the worktree, the branch and the session already exist. Failing the spawn
//! there would leave an orphan session on the spawning host and still no branch on the
//! orchestrator's — strictly worse than a node the operator can re-link by restarting it. The live
//! association still travels in participant metadata (D37).
//!
//! That tolerance is the whole of D36 and it lived, until now, only as `if let Err(status)` arms
//! inside spawn paths no test can drive. What is proven here is the seam every spawn path goes
//! through: the owner is genuinely asked, and its refusal is not raised to the caller.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D36).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tddy_daemon::connection_service::{
    SpawnStackParent, StackBaseLookup, StackNodeLink, StackParentHost,
};
use tddy_rpc::Status;

const ORCHESTRATOR: &str = "orchestrator-1";
const NODE: &str = "attach-store";
const CHILD: &str = "dddddddd-0000-4000-8000-000000000004";
const BRANCH: &str = "feature/attach-docs/attach-store";

/// The daemon that owns the orchestrator, as the spawn path sees it: it records what it was asked,
/// and answers every link with `refusal`.
struct AnOwnerThatRefusesEveryLink {
    refusal: String,
    asked: Mutex<Vec<(String, String, String)>>,
}

impl AnOwnerThatRefusesEveryLink {
    fn refusing_with(refusal: &str) -> Self {
        Self {
            refusal: refusal.to_string(),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The `(node_id, child_session_id, branch)` of every link it was asked to record.
    fn links_it_was_asked_for(&self) -> Vec<(String, String, String)> {
        self.asked.lock().expect("recorded links").clone()
    }
}

#[async_trait::async_trait]
impl StackParentHost for AnOwnerThatRefusesEveryLink {
    async fn chain_base_ref(
        &self,
        _lookup: &StackBaseLookup<'_>,
    ) -> Result<Option<String>, Status> {
        Ok(None)
    }

    async fn link_spawned_branch(&self, link: &StackNodeLink<'_>) -> Result<(), Status> {
        self.asked.lock().expect("record the link").push((
            link.node_id.to_string(),
            link.child_session_id.to_string(),
            link.branch.to_string(),
        ));
        Err(Status::not_found(self.refusal.clone()))
    }
}

fn a_sessions_base() -> PathBuf {
    Path::new("/var/lib/tddy/u/sessions").to_path_buf()
}

fn a_spawn_of_the_planned_node(host: &dyn StackParentHost) -> SpawnStackParent<'_> {
    SpawnStackParent::OwnedBy {
        session_id: ORCHESTRATOR,
        daemon_instance_id: "host-a",
        stack_node_id: NODE,
        session_token: "valid-session-token",
        host,
    }
}

#[tokio::test]
async fn a_refused_link_leaves_the_spawn_standing() {
    // Given — the orchestrator's own daemon refuses the link, which is what an unreachable peer, an
    // unknown node or an unwritable changeset all look like from here
    let owner = AnOwnerThatRefusesEveryLink::refusing_with(
        "planned node 'attach-store' is not in the stack of orchestrator orchestrator-1",
    );

    // When
    a_spawn_of_the_planned_node(&owner)
        .link_spawned_branch_without_failing_the_spawn(&a_sessions_base(), BRANCH, CHILD)
        .await;

    // Then — the owner was genuinely asked, and its refusal never reached the spawn
    assert_eq!(
        owner.links_it_was_asked_for(),
        vec![(NODE.to_string(), CHILD.to_string(), BRANCH.to_string())],
    );
}
