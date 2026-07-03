//! PRD acceptance: PR-stack data model — `Stack`, `StackNode`, serde roundtrip, DAG methods.

use std::fs;
use std::path::PathBuf;

use tddy_core::changeset::{
    read_changeset, write_changeset, Changeset, GithubPrStatus, PrInternalStatus, Stack, StackNode,
};

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-stack-dm-acc-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn stack_roundtrip_linear_and_dag() {
    // Given — build a Changeset with a 3-node DAG (n1 <- n2, n1 <- n3 also depends on n2)
    let dir = scratch("roundtrip");
    let stack = Stack {
        version: 1,
        nodes: vec![
            StackNode {
                node_id: "n1".into(),
                title: "Auth store".into(),
                description: "JWT token storage".into(),
                branch_suggestion: Some("feature/auth-store".into()),
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
                internal_status: None,
            },
            StackNode {
                node_id: "n2".into(),
                title: "Login API".into(),
                description: "Login endpoint".into(),
                branch_suggestion: Some("feature/login-api".into()),
                branch: None,
                session_id: None,
                parents: vec!["n1".into()],
                pr_status: None,
                child_state: None,
                internal_status: None,
            },
            StackNode {
                node_id: "n3".into(),
                title: "Dashboard".into(),
                description: "Dashboard UI".into(),
                branch_suggestion: Some("feature/dashboard".into()),
                branch: None,
                session_id: None,
                parents: vec!["n1".into(), "n2".into()],
                pr_status: None,
                child_state: None,
                internal_status: None,
            },
        ],
    };
    let cs = Changeset {
        name: Some("PR stack roundtrip".into()),
        stack: Some(stack),
        ..Changeset::default()
    };

    // When
    write_changeset(&dir, &cs).unwrap();
    let loaded = read_changeset(&dir).unwrap();
    let _ = fs::remove_dir_all(&dir);

    // Then
    let loaded_stack = loaded.stack.expect("stack must survive serde roundtrip");
    assert_eq!(loaded_stack.version, 1);
    assert_eq!(loaded_stack.nodes.len(), 3);
    let n1 = loaded_stack.node("n1").expect("n1 must exist");
    let n3 = loaded_stack.node("n3").expect("n3 must exist");
    assert_eq!(n1.parents, Vec::<String>::new(), "root node has no parents");
    assert_eq!(
        n3.parents,
        vec!["n1".to_string(), "n2".to_string()],
        "DAG node carries both parent ids"
    );
}

#[test]
fn effective_base_refs_skips_merged_ancestor() {
    // Given — n1 merged; n2 depends on n1; effective base of n2 should collapse to stack_bottom_base
    let stack = Stack {
        version: 1,
        nodes: vec![
            StackNode {
                node_id: "n1".into(),
                title: "Base PR".into(),
                description: String::new(),
                branch_suggestion: Some("feature/base-pr".into()),
                branch: Some("feature/base-pr".into()),
                session_id: Some("sid-n1".into()),
                parents: vec![],
                pr_status: Some(GithubPrStatus {
                    phase: "merged".into(),
                    url: None,
                    error: None,
                }),
                child_state: None,
                internal_status: None,
            },
            StackNode {
                node_id: "n2".into(),
                title: "Child PR".into(),
                description: String::new(),
                branch_suggestion: Some("feature/child-pr".into()),
                branch: Some("feature/child-pr".into()),
                session_id: Some("sid-n2".into()),
                parents: vec!["n1".into()],
                pr_status: Some(GithubPrStatus {
                    phase: "open".into(),
                    url: None,
                    error: None,
                }),
                child_state: None,
                internal_status: None,
            },
        ],
    };

    // When — n2's effective base: n1 is merged, so it collapses to stack_bottom_base
    let refs = stack.effective_base_refs("n2", "origin/master");

    // Then
    assert_eq!(
        refs,
        vec!["origin/master".to_string()],
        "effective_base_refs must skip merged parent n1 and return stack_bottom_base for n2"
    );
}

#[test]
fn topo_order_rejects_cycle() {
    // Given — cycle: n1 depends on n2, n2 depends on n1
    let stack = Stack {
        version: 1,
        nodes: vec![
            StackNode {
                node_id: "n1".into(),
                title: "A".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec!["n2".into()],
                pr_status: None,
                child_state: None,
                internal_status: None,
            },
            StackNode {
                node_id: "n2".into(),
                title: "B".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec!["n1".into()],
                pr_status: None,
                child_state: None,
                internal_status: None,
            },
        ],
    };

    // When
    let result = stack.topo_order();

    // Then
    assert!(
        result.is_err(),
        "topo_order must return Err on a cyclic dependency graph"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("cycle"),
        "error message must mention cycle; got: {err_msg}"
    );
}

#[test]
fn internal_status_roundtrips_through_changeset_yaml() {
    // Given — a spawned node whose PR needs repointing (an action-needed signal that is
    // orthogonal to `pr_status`, which mirrors GitHub reality)
    let dir = scratch("internal-status");
    let stack = Stack {
        version: 1,
        nodes: vec![StackNode {
            node_id: "n2".into(),
            title: "Login API".into(),
            description: String::new(),
            branch_suggestion: Some("feature/auth/login-api".into()),
            branch: Some("feature/auth/login-api".into()),
            session_id: Some("sid-n2".into()),
            parents: vec!["n1".into()],
            pr_status: Some(GithubPrStatus {
                phase: "open".into(),
                url: None,
                error: None,
            }),
            child_state: None,
            internal_status: Some(PrInternalStatus {
                kind: "needs-repoint".into(),
                note: Some(
                    "parent n1 merged; base still points at feature/auth/token-store".into(),
                ),
                source: "derived".into(),
            }),
        }],
    };
    let cs = Changeset {
        stack: Some(stack),
        ..Changeset::default()
    };

    // When
    write_changeset(&dir, &cs).unwrap();
    let loaded = read_changeset(&dir).unwrap();
    let _ = fs::remove_dir_all(&dir);

    // Then
    let node = loaded
        .stack
        .expect("stack must survive roundtrip")
        .node("n2")
        .cloned()
        .expect("n2 must exist");
    let status = node
        .internal_status
        .expect("internal_status must survive the serde roundtrip");
    assert_eq!(status.kind, "needs-repoint");
    assert_eq!(
        status.note.as_deref(),
        Some("parent n1 merged; base still points at feature/auth/token-store")
    );
    assert_eq!(status.source, "derived");
}

#[test]
fn a_node_without_internal_status_key_reads_as_none() {
    // Given — a stack-node YAML written before this field existed (no `internal_status:` key)
    let dir = scratch("internal-status-legacy");
    let legacy_yaml = r#"
name: legacy stack session
version: 1
models: {}
sessions: []
state:
  current: StackPlanned
  updated_at: "2024-01-01T00:00:00Z"
  history: []
artifacts: {}
discovery: null
stack:
  version: 1
  nodes:
    - node_id: n1
      title: Add token store
      description: ""
      branch_suggestion: feature/auth/token-store
      branch: null
      session_id: null
      parents: []
      pr_status: null
      child_state: null
"#;
    fs::write(dir.join("changeset.yaml"), legacy_yaml.trim()).unwrap();

    // When
    let cs = read_changeset(&dir).unwrap();
    let _ = fs::remove_dir_all(&dir);

    // Then
    let node = cs
        .stack
        .expect("stack present")
        .node("n1")
        .cloned()
        .expect("n1 present");
    assert!(
        node.internal_status.is_none(),
        "a node YAML without internal_status: must deserialize with internal_status = None"
    );
}

#[test]
fn legacy_changeset_without_stack_reads_as_none() {
    // Given — YAML without a `stack:` key (simulates pre-feature changeset)
    let dir = scratch("legacy");
    let legacy_yaml = r#"
name: legacy session
version: 1
models: {}
sessions: []
state:
  current: Planned
  updated_at: "2024-01-01T00:00:00Z"
  history: []
artifacts: {}
discovery: null
"#;
    fs::write(dir.join("changeset.yaml"), legacy_yaml.trim()).unwrap();

    // When
    let cs = read_changeset(&dir).unwrap();
    let _ = fs::remove_dir_all(&dir);

    // Then
    assert!(
        cs.stack.is_none(),
        "legacy changeset without stack: key must deserialize with stack = None"
    );
}
