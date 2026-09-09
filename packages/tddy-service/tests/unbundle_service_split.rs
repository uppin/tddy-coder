//! What the `#unbundle` stack's proto split must end up having done.
//!
//! Two kinds of assertion live here. The **served-coordinate** tests pass as soon as a node
//! publishes its proto, and pin the coordinate so a later tidy-up cannot rename it. The
//! **final-shape** test fails until the methods are actually gone from
//! `connection.ConnectionService`, which is what makes it the completion criterion for a node
//! rather than a description of one.
//!
//! Reading the `.proto` text rather than the generated Rust is deliberate: the generated trait is
//! what the daemon implements, but the `.proto` is what every other language's client is generated
//! from, and a method left declared there is a coordinate somebody can still call.

use std::path::Path;

fn connection_proto() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/connection.proto");
    std::fs::read_to_string(path).expect("connection.proto is readable")
}

fn service_block(proto: &str, service: &str) -> String {
    let start = proto
        .find(&format!("service {service} {{"))
        .unwrap_or_else(|| panic!("{service} is declared"));
    let end = proto[start..]
        .find("\n}")
        .unwrap_or_else(|| panic!("{service}'s block is closed"));
    proto[start..start + end].to_string()
}

fn read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proto")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{name} is readable"))
}

const HOST_METHODS: [&str; 8] = [
    "ListEligibleDaemons",
    "ListKnownHosts",
    "GetHostTooling",
    "StreamHostPrompts",
    "AnswerHostPrompt",
    "AddHostKey",
    "ListHostKeyCandidates",
    "StreamHostStats",
];

const WORKTREE_METHODS: [&str; 9] = [
    "ListWorktreesForProject",
    "RemoveWorktree",
    "StreamWorktreeStats",
    "CalculateWorktreeSize",
    "CleanWorktree",
    "RestoreSessionWorktree",
    "ListWorktreeDirectory",
    "ReadWorktreeFile",
    "StreamReadWorktreeFile",
];

#[test]
fn host_service_declares_every_host_method() {
    // Given
    let block = service_block(&read("host.proto"), "HostService");

    // Then
    for method in HOST_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "host.HostService is missing {method}"
        );
    }
}

#[test]
fn worktree_service_declares_every_worktree_method() {
    // Given
    let block = service_block(&read("worktree.proto"), "WorktreeService");

    // Then
    for method in WORKTREE_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "worktree.WorktreeService is missing {method}"
        );
    }
}

/// The two new protos import nothing, because the closure of messages their 17 methods reach shares
/// **nothing** with any method that stayed in `connection.proto`. That was established by walking
/// field types, and this test is what stops a later node importing a shared types file into them out
/// of habit and re-coupling them.
#[test]
fn neither_new_proto_imports_connection() {
    for name in ["host.proto", "worktree.proto"] {
        let proto = read(name);
        assert!(
            !proto.contains("import \"connection.proto\""),
            "{name} must not import connection.proto"
        );
    }
}

/// **The completion criterion for `#unbundle` node 1.** Publishing the two protos is half the job;
/// the split is only real once the coordinates are gone from the service they left, because until
/// then both coordinates answer and a client can keep calling the old one.
#[test]
fn connection_service_no_longer_declares_the_moved_methods() {
    // Given
    let block = service_block(&connection_proto(), "ConnectionService");

    // When
    let still_there: Vec<&str> = HOST_METHODS
        .into_iter()
        .chain(WORKTREE_METHODS)
        .filter(|method| block.contains(&format!("rpc {method}(")))
        .collect();

    // Then
    assert!(
        still_there.is_empty(),
        "these moved to host.HostService / worktree.WorktreeService but are still declared on \
         connection.ConnectionService: {still_there:?}"
    );
}

/// The residual is the deliberate endpoint of the whole stack, not a leftover: families C (sessions
/// lifecycle), D (projects and branches), O (demo VM) and Q (`MintLocalToken`). Pinning the count
/// makes every later node state its arithmetic out loud instead of drifting.
#[test]
fn connection_service_keeps_exactly_the_methods_node_one_leaves_behind() {
    // Given
    let block = service_block(&connection_proto(), "ConnectionService");

    // When
    let declared = block.matches("  rpc ").count();

    // Then
    assert_eq!(
        declared, 73,
        "node 1 moves 17 of 90; later nodes take it to 21"
    );
}
