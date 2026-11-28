#![cfg(test)]
use super::*;

// -----------------------------------------------------------------------
// effective_base_ref unit tests
// -----------------------------------------------------------------------

fn node_view_with_pr(
    node_id: &str,
    parents: &[&str],
    phase: ChildPhase,
    pr: PrLiveStatus,
) -> NodeView {
    NodeView {
        node_id: node_id.to_string(),
        branch: format!("feature/{node_id}"),
        parent_dep_ids: parents.iter().map(|s| s.to_string()).collect(),
        child_session_id: None,
        child_state: None,
        child_phase: phase,
        pr,
    }
}

/// `effective_base_ref_skips_merged_ancestor_returns_default_branch` — when the only
/// parent node's PR is merged, `effective_base_ref` must skip it and return `default_branch`.
#[test]
fn effective_base_ref_skips_merged_ancestor_returns_default_branch() {
    // n1 (root) → merged PR
    // n2 depends on n1 (which is merged) → effective base should collapse to default_branch
    let views = vec![
        node_view_with_pr("n1", &[], ChildPhase::PrOpen, PrLiveStatus::Merged),
        node_view_with_pr(
            "n2",
            &["n1"],
            ChildPhase::PrOpen,
            PrLiveStatus::Open {
                number: 2,
                base: "feature/n1".into(),
            },
        ),
    ];

    let base = effective_base_ref("n2", &views, "master");

    assert_eq!(
        base, "master",
        "effective base of n2 must be 'master' when its only parent (n1) is already merged"
    );
}

/// `effective_base_ref_returns_nearest_unmerged_ancestor_branch` — when a middle ancestor is
/// not yet merged, `effective_base_ref` must return that ancestor's branch.
#[test]
fn effective_base_ref_returns_nearest_unmerged_ancestor_branch() {
    // Linear: n1 (merged) → n2 (open PR, base=master after n1 merge) → n3 (open, base=feature/n2)
    // n2 is still open, so n3's effective base is feature/n2
    let views = vec![
        node_view_with_pr("n1", &[], ChildPhase::PrOpen, PrLiveStatus::Merged),
        node_view_with_pr(
            "n2",
            &["n1"],
            ChildPhase::PrOpen,
            PrLiveStatus::Open {
                number: 2,
                base: "master".into(),
            },
        ),
        node_view_with_pr(
            "n3",
            &["n2"],
            ChildPhase::PrOpen,
            PrLiveStatus::Open {
                number: 3,
                base: "feature/n2".into(),
            },
        ),
    ];

    let base = effective_base_ref("n3", &views, "master");

    assert_eq!(
        base, "feature/n2",
        "effective base of n3 must be feature/n2 (its unmerged parent branch), not 'master'"
    );
}

/// `effective_base_ref_root_node_returns_default_branch` — a root node with no parents
/// must always return `default_branch` as its effective base.
#[test]
fn effective_base_ref_root_node_returns_default_branch() {
    let views = vec![node_view_with_pr(
        "n1",
        &[],
        ChildPhase::NotSpawned,
        PrLiveStatus::None,
    )];

    let base = effective_base_ref("n1", &views, "master");

    assert_eq!(
        base, "master",
        "root node with no parents must use default_branch as effective base"
    );
}

// -----------------------------------------------------------------------
// assemble_views unit tests (via MockGithubPrApi)
// -----------------------------------------------------------------------

struct AlwaysOpenMockGh {
    pr_number: u64,
    base: String,
}
impl tddy_github::pr_api::GithubPrApi for AlwaysOpenMockGh {
    fn get_open_pr(
        &self,
        _head: &str,
    ) -> Result<Option<tddy_github::pr_api::PrRef>, tddy_core::WorkflowError> {
        Ok(Some(tddy_github::pr_api::PrRef {
            number: self.pr_number,
            head_sha: "sha".into(),
            base_branch: self.base.clone(),
            url: "https://github.com/o/r/pull/1".into(),
        }))
    }
    fn get_pr_by_head(&self, _head: &str) -> tddy_github::pr_api::PrLookupOutcome {
        tddy_github::pr_api::PrLookupOutcome::Found(tddy_github::pr_api::PrView {
            number: self.pr_number,
            url: "https://github.com/o/r/pull/1".into(),
            state: tddy_github::pr_api::PrState::Open,
        })
    }
    fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
        Ok("sha".into())
    }
    fn patch_pr_base(&self, _number: u64, _new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        Ok(99)
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn close_pr(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
}

struct NoneOpenMockGh;
impl tddy_github::pr_api::GithubPrApi for NoneOpenMockGh {
    fn get_open_pr(
        &self,
        _head: &str,
    ) -> Result<Option<tddy_github::pr_api::PrRef>, tddy_core::WorkflowError> {
        Ok(None)
    }
    fn get_pr_by_head(&self, _head: &str) -> tddy_github::pr_api::PrLookupOutcome {
        tddy_github::pr_api::PrLookupOutcome::NotFound
    }
    fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
        Ok("sha".into())
    }
    fn patch_pr_base(&self, _number: u64, _new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        Ok(99)
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn close_pr(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
}

/// `assemble_views_marks_notspawned_when_session_id_absent` — when a `WorkItem` has
/// `session_id == None`, `assemble_views` must produce a `NodeView` with
/// `child_phase == NotSpawned`, and the views must be in topological order.
#[test]
fn assemble_views_marks_notspawned_when_session_id_absent() {
    use tddy_core::changeset::{WorkItem, WorkQueue};

    let tmp = tempfile::tempdir().unwrap();
    let parent_dir = tmp.path();
    let sessions_root = tmp.path();

    // Write an orchestrator changeset with two nodes: n1 (root), n2 (depends on n1)
    let stack = WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".into(),
                title: "Root".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None, // not spawned
                parents: vec![],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n2".into(),
                title: "Dep".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None, // not spawned
                parents: vec!["n1".into()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mock_gh = NoneOpenMockGh;

    let views = assemble_views(parent_dir, sessions_root, &stack, &mock_gh, "master")
        .expect("assemble_views must not error for unspawned nodes");

    assert_eq!(views.len(), 2, "must produce one view per node");

    // Topological order: n1 before n2
    assert_eq!(
        views[0].node_id, "n1",
        "n1 (root) must appear before n2 in topo order"
    );
    assert_eq!(
        views[1].node_id, "n2",
        "n2 (dependent) must appear after n1"
    );

    assert!(
        matches!(views[0].child_phase, ChildPhase::NotSpawned),
        "n1 must be NotSpawned (no session_id); got: {:?}",
        views[0].child_phase
    );
    assert!(
        matches!(views[1].child_phase, ChildPhase::NotSpawned),
        "n2 must be NotSpawned (no session_id); got: {:?}",
        views[1].child_phase
    );
}

/// `assemble_views_marks_spawn_requested_when_a_pending_spawn_request_marker_file_exists` —
/// `SpawnTask` writes `.workflow/spawn-request-<node_id>.json` before the daemon has had a
/// chance to actually create the child session and set `session_id`. While that marker is
/// present, `assemble_views` must report `SpawnRequested` (not `NotSpawned`), so the same
/// spawn is not issued again before the child exists.
#[test]
fn assemble_views_marks_spawn_requested_when_a_pending_spawn_request_marker_file_exists() {
    use tddy_core::changeset::{WorkItem, WorkQueue};

    let tmp = tempfile::tempdir().unwrap();
    let parent_dir = tmp.path();
    let sessions_root = tmp.path();

    let workflow_dir = parent_dir.join(".workflow");
    std::fs::create_dir_all(&workflow_dir).unwrap();
    std::fs::write(
        workflow_dir.join("spawn-request-n1.json"),
        r#"{"node_id":"n1"}"#,
    )
    .unwrap();

    let stack = WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: "n1".into(),
            title: "Root".into(),
            description: String::new(),
            branch_suggestion: None,
            branch: None,
            session_id: None, // daemon hasn't created the child session yet
            parents: vec![],
            pr_status: None,
            child_state: None,
            internal_status: None,
            display_order: None,
            ..Default::default()
        }],
        ..Default::default()
    };
    let mock_gh = NoneOpenMockGh;

    let views = assemble_views(parent_dir, sessions_root, &stack, &mock_gh, "master")
        .expect("assemble_views must not error");

    assert_eq!(views.len(), 1);
    assert!(
        matches!(views[0].child_phase, ChildPhase::SpawnRequested),
        "n1 must be SpawnRequested while its spawn-request marker file exists; got: {:?}",
        views[0].child_phase
    );
}

/// A node's PR belongs to its **branch**, not to its child session. Once the branch exists the
/// PR must be reported whether or not a session is still attached — sessions come and go
/// (closed, cleaned, restarted) while the branch and its PR stay.
#[test]
fn assemble_views_reports_the_pr_of_a_branch_whose_child_session_is_gone() {
    use tddy_core::changeset::{WorkItem, WorkQueue};

    // Given — n1 owns a branch but no session points at it any more
    let tmp = tempfile::tempdir().unwrap();
    let stack = WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: "n1".into(),
            title: "Root".into(),
            description: String::new(),
            branch_suggestion: Some("feature/n1".into()),
            branch: Some("feature/n1".into()),
            session_id: None,
            parents: vec![],
            pr_status: None,
            child_state: None,
            internal_status: None,
            display_order: None,
            ..Default::default()
        }],
        ..Default::default()
    };
    let gh = AlwaysOpenMockGh {
        pr_number: 7,
        base: "master".into(),
    };

    // When
    let views = assemble_views(tmp.path(), tmp.path(), &stack, &gh, "master")
        .expect("assemble_views must not error");

    // Then
    assert_eq!(
        views[0].pr,
        PrLiveStatus::Open {
            number: 7,
            base: "master".to_string()
        }
    );
}

/// `assemble_views_reports_propen_when_mock_returns_open_pr` — when the `GithubPrApi` mock
/// returns an open PR for a node's branch, `assemble_views` must map it to `PrLiveStatus::Open`.
#[test]
fn assemble_views_reports_propen_when_mock_returns_open_pr() {
    use tddy_core::changeset::{write_changeset_atomic, Changeset, WorkItem, WorkQueue};
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::WorkflowState;

    let tmp = tempfile::tempdir().unwrap();
    let parent_dir = tmp.path();
    let sessions_root = tmp.path();

    // Child session dir for n1
    let child_dir = unified_session_dir_path(sessions_root, "child-sess-n1");
    std::fs::create_dir_all(&child_dir).unwrap();
    let mut child_cs = Changeset::default();
    child_cs.state.current = WorkflowState::new("Done");
    write_changeset_atomic(&child_dir, &child_cs).unwrap();

    let stack = WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: "n1".into(),
            title: "Root".into(),
            description: String::new(),
            branch_suggestion: None,
            branch: Some("feature/n1".into()),
            session_id: Some("child-sess-n1".into()),
            parents: vec![],
            pr_status: None,
            child_state: None,
            internal_status: None,
            display_order: None,
            ..Default::default()
        }],
        ..Default::default()
    };

    let mock_gh = AlwaysOpenMockGh {
        pr_number: 7,
        base: "master".into(),
    };

    let views = assemble_views(parent_dir, sessions_root, &stack, &mock_gh, "master")
        .expect("assemble_views must not error");

    assert_eq!(views.len(), 1);
    assert!(
        matches!(&views[0].pr, PrLiveStatus::Open { number: 7, .. }),
        "PR status must be Open {{ number: 7 }} when mock returns an open PR; got: {:?}",
        views[0].pr
    );
}
