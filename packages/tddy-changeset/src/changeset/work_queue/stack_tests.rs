use super::*;

use crate::changeset::model::{Changeset, ChangesetWorkflow};

#[test]
fn effective_base_refs_single_parent_skip() {
    let stack = WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n1".to_string()),
                session_id: Some("sess-1".to_string()),
                parents: vec![],
                pr_status: Some(GithubPrStatus {
                    phase: "merged".to_string(),
                    url: None,
                    error: None,
                }),
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n2".to_string(),
                title: "Node 2".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n2".to_string()),
                session_id: None,
                parents: vec!["n1".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let refs = stack.effective_base_refs("n2", "origin/master");
    assert_eq!(refs, vec!["origin/master".to_string()]);
}

#[test]
fn effective_base_refs_multi_parent_unmerged_set() {
    let stack = WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n1".to_string()),
                session_id: Some("sess-1".to_string()),
                parents: vec![],
                pr_status: Some(GithubPrStatus {
                    phase: "open".to_string(),
                    url: None,
                    error: None,
                }),
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n2".to_string(),
                title: "Node 2".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n2".to_string()),
                session_id: Some("sess-2".to_string()),
                parents: vec![],
                pr_status: Some(GithubPrStatus {
                    phase: "open".to_string(),
                    url: None,
                    error: None,
                }),
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n3".to_string(),
                title: "Node 3".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n3".to_string()),
                session_id: None,
                parents: vec!["n1".to_string(), "n2".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let refs = stack.effective_base_refs("n3", "origin/master");
    assert_eq!(
        refs,
        vec![
            "origin/feature/n1".to_string(),
            "origin/feature/n2".to_string()
        ]
    );
}

#[test]
fn effective_base_refs_all_merged_returns_bottom_base() {
    let merged_status = || {
        Some(GithubPrStatus {
            phase: "merged".to_string(),
            url: None,
            error: None,
        })
    };
    let stack = WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n1".to_string()),
                session_id: Some("sess-1".to_string()),
                parents: vec![],
                pr_status: merged_status(),
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n2".to_string(),
                title: "Node 2".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n2".to_string()),
                session_id: Some("sess-2".to_string()),
                parents: vec!["n1".to_string()],
                pr_status: merged_status(),
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n3".to_string(),
                title: "Node 3".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n3".to_string()),
                session_id: None,
                parents: vec!["n2".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let refs = stack.effective_base_refs("n3", "origin/master");
    assert_eq!(refs, vec!["origin/master".to_string()]);
}

#[test]
fn topo_order_linear_dag() {
    let stack = WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n2".to_string(),
                title: "Node 2".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec!["n1".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n3".to_string(),
                title: "Node 3".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec!["n2".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let order = stack.topo_order().unwrap();
    assert_eq!(
        order,
        vec!["n1".to_string(), "n2".to_string(), "n3".to_string()]
    );
}

#[test]
fn topo_order_cycle_returns_error() {
    let stack = WorkQueue {
        version: 1,
        nodes: vec![
            WorkItem {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec!["n2".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
            WorkItem {
                node_id: "n2".to_string(),
                title: "Node 2".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec!["n1".to_string()],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let result = stack.topo_order();
    assert!(result.is_err(), "expected Err for cycle, got Ok");
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains("cycle"),
        "error message should mention 'cycle', got: {err}"
    );
}

// -----------------------------------------------------------------------
// `display_order` — the *reading* order, which is a different fact from the DAG
// -----------------------------------------------------------------------

mod display_order;

use crate::work_queue_fixtures::{a_stack_host, queue_id_of, stack_on_disk};

fn a_plain_node(node_id: &str) -> WorkItem {
    WorkItem {
        node_id: node_id.to_string(),
        title: "Node 1".to_string(),
        ..WorkItem::default()
    }
}

#[test]
fn update_stack_atomic_reads_and_writes_back() {
    // Given
    let host = a_stack_host();
    let dir = host.orchestrator(
        "orch",
        WorkQueue {
            version: 1,
            nodes: vec![],
            ..Default::default()
        },
    );

    // When
    update_stack_atomic(&dir, |stack| {
        stack.version = 42;
    })
    .unwrap();

    // Then
    assert_eq!(stack_on_disk(&dir).version, 42);
}

#[test]
fn updating_a_session_that_owns_no_queue_is_refused_and_creates_none() {
    // Given
    let host = a_stack_host();
    let dir = host.session("orch", Changeset::default());

    // When
    let written = update_stack_atomic(&dir, |stack| stack.nodes.push(a_plain_node("n1")));

    // Then
    assert!(written.is_err(), "no write creates a queue");
    assert_eq!(read_session_stack(&dir).unwrap(), None);
    assert_eq!(queue_id_of(&dir), None);
}

#[test]
fn a_session_that_owns_a_queue_reads_it_from_its_own_directory() {
    // Given
    let host = a_stack_host();
    let dir = host.orchestrator(
        "orch",
        WorkQueue {
            version: 1,
            nodes: vec![a_plain_node("n1")],
            ..Default::default()
        },
    );

    // When
    let record = host
        .store()
        .read(crate::work_queue_fixtures::A_PROJECT, "orch")
        .unwrap();

    // Then
    assert_eq!(queue_id_of(&dir).as_deref(), Some("orch"));
    assert_eq!(
        (record.owner_session_id(), record.stack.nodes.len()),
        ("orch", 1)
    );
}

#[test]
fn link_work_item_to_session_sets_fields() {
    // Given
    let host = a_stack_host();
    let dir = host.orchestrator(
        "orch",
        WorkQueue {
            version: 1,
            nodes: vec![a_plain_node("n1")],
            ..Default::default()
        },
    );

    // When
    link_work_item_to_session(&dir, "n1", "sess-abc", Some("feature/n1".to_string())).unwrap();

    // Then
    let stack = stack_on_disk(&dir);
    let n1 = stack.node("n1").unwrap();
    assert_eq!(n1.session_id.as_deref(), Some("sess-abc"));
    assert_eq!(n1.branch.as_deref(), Some("feature/n1"));
}

#[test]
fn sync_stack_node_from_child_propagates_state_and_pr_status() {
    use tddy_workflow::ids::WorkflowState;
    // Given — an orchestrator whose node n1 names a child session in the same host
    let host = a_stack_host();
    let child_id = "child-session-1";
    let mut child_cs = Changeset::default();
    child_cs.state.current = WorkflowState::new("GreenImplementing");
    child_cs.workflow = Some(ChangesetWorkflow {
        github_pr_status: Some(GithubPrStatus {
            phase: "open".to_string(),
            url: Some("https://github.com/example/pr/1".to_string()),
            error: None,
        }),
        ..Default::default()
    });
    host.session(child_id, child_cs);
    let orch_dir = host.orchestrator(
        "orch",
        WorkQueue {
            version: 1,
            nodes: vec![WorkItem {
                branch: Some("feature/n1".to_string()),
                session_id: Some(child_id.to_string()),
                ..a_plain_node("n1")
            }],
            ..Default::default()
        },
    );

    // When
    sync_stack_node_from_child(&orch_dir, host.base(), "n1").unwrap();

    // Then
    let stack = stack_on_disk(&orch_dir);
    let n1 = stack.node("n1").unwrap();
    assert_eq!(
        n1.child_state,
        Some(WorkflowState::new("GreenImplementing"))
    );
    assert_eq!(n1.pr_status.as_ref().unwrap().phase, "open");
}

#[test]
fn serde_back_compat_changeset_without_stack_field() {
    let yaml = r#"
version: 1
models: {}
sessions: []
state:
  current: Init
  updated_at: "2024-01-01T00:00:00Z"
  history: []
artifacts: {}
discovery: ~
"#;
    let cs: Changeset = serde_yaml::from_str(yaml).expect("legacy changeset should deserialize");
    assert!(
        cs.orchestrator_session_id.is_none(),
        "orchestrator_session_id should be None for legacy changeset"
    );
}
