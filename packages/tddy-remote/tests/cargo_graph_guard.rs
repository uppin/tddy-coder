//! Acceptance: remote client crate must not pull in workflow orchestration (PRD: cargo_graph_no_tddy_workflow_in_remote_crates).

use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("tddy-remote manifest should live under packages/<crate>/")
}

#[test]
fn cargo_graph_no_tddy_workflow_in_remote_crates() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.contains("tddy-workflow"),
        "tddy-remote must not depend on tddy-workflow (PRD F7 / explicit non-goal)"
    );
    assert!(
        manifest.contains("tddy-connectrpc"),
        "tddy-remote Cargo.toml must declare tddy-connectrpc (Connect stack; PRD F3)"
    );
    assert!(
        manifest.contains("reqwest"),
        "tddy-remote Cargo.toml must declare reqwest for HTTP Connect client calls (PRD F3)"
    );

    // Direct edges only: tddy-service legitimately depends on tddy-workflow for other modules, but
    // tddy-remote must not list workflow crates as its own dependencies (PRD F7).
    let out = Command::new("cargo")
        .current_dir(workspace_root())
        .args([
            "tree",
            "-p",
            "tddy-remote",
            "--edges",
            "normal,build",
            "--depth",
            "1",
        ])
        .output()
        .expect("spawn cargo tree; ensure you run tests from the workspace root toolchain");

    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(
        !tree.contains("tddy-workflow"),
        "direct dependency edges for tddy-remote must not include tddy-workflow:\n{tree}"
    );
    assert!(
        tree.contains("tddy-service"),
        "tddy-remote must list tddy-service in `cargo tree` once Connect/proto client wiring exists (add path dependency).\n\
Current tree:\n{tree}"
    );
}
