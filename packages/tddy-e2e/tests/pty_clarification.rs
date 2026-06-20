//! PTY-driven E2E test: clarification question appears on screen.
//!
//! Spawns tddy-demo in a PTY, verifies that the clarification question (Scope, options
//! Email/password, OAuth) is rendered, then selects first option and verifies workflow proceeds.

use std::path::PathBuf;
use std::time::Duration;

use termwright::prelude::*;

/// Path to tddy-demo binary. Uses workspace target dir when built via cargo test.
fn tddy_demo_binary() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // packages/tddy-e2e -> workspace root is ../..
    let workspace_root = manifest_dir.join("../..");
    workspace_root.join("target/debug/tddy-demo")
}

#[tokio::test]
#[ignore = "PTY test: run with --ignored; requires tddy-demo binary (cargo build -p tddy-demo)"]
async fn clarification_question_appears_on_screen() {
    // Given
    let bin = tddy_demo_binary();
    if !bin.exists() {
        eprintln!("Skipping: tddy-demo not built. Run: cargo build -p tddy-demo");
        return;
    }

    let term = Terminal::builder()
        .size(80, 24)
        .spawn(bin.to_str().unwrap(), &["--prompt", "CLARIFY test feature"])
        .await
        .expect("spawn tddy-demo");

    // When — workflow starts and reaches the clarification question
    term.expect("Which authentication method")
        .timeout(Duration::from_secs(10))
        .await
        .expect("clarification question should appear");

    // Then — both answer options are visible on screen
    let screen = term.screen().await;
    assert!(
        screen.contains("Email/password"),
        "screen should show Email/password option, got: {}",
        screen.text()
    );
    assert!(
        screen.contains("OAuth"),
        "screen should show OAuth option, got: {}",
        screen.text()
    );

    // When — user selects the first option
    term.enter().await.expect("send Enter");

    // Then — workflow proceeds past the clarification step
    term.expect("plan")
        .timeout(Duration::from_secs(15))
        .await
        .expect("workflow should proceed after answering");

    // Clean exit
    term.kill().await.ok();
}
