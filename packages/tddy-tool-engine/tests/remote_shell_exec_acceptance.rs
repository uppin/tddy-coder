//! Acceptance tests: exec-catalog tools run through LocalShell or RemoteShell.
//!
//! Empty `ssh_config_host` is today's host filesystem. A set alias is OpenSSH (`BatchMode=yes`)
//! on that Host — a failure is the result, never a local read.
//!
//! Green will speak `ssh` against a fixture target. Wave 2 publishes the surface; RemoteShell
//! bodies are still `TODO(exec)`.

use tddy_task::TaskRegistry;
use tddy_tool_engine::{
    execute_tool_on_shell, session_shell, LocalShell, RemoteShell, Shell, ShellKind,
};

fn registry() -> TaskRegistry {
    TaskRegistry::new()
}

#[tokio::test]
async fn read_over_remote_shell_returns_the_file_written_on_the_target() {
    // Given — a RemoteShell aimed at a Host alias whose worktree holds hello.txt
    let shell = RemoteShell::new("buildbox", "/home/dev/repo/.worktrees/sess");
    let registry = registry();

    // When
    let outcome = execute_tool_on_shell(
        &shell,
        "Read",
        r#"{"path":"hello.txt"}"#,
        &registry,
        "ssh-exec-session",
    )
    .await;

    // Then — the bytes come from the SSH target, not this host
    assert!(
        !outcome.is_error,
        "Read over RemoteShell must succeed; got: {}",
        outcome.error_message
    );
    let parsed: serde_json::Value = serde_json::from_str(&outcome.result_json).expect("json");
    assert_eq!(
        parsed.get("content").and_then(|v| v.as_str()),
        Some("from-the-target"),
        "Read must return the remote file; got: {}",
        outcome.result_json
    );
}

#[tokio::test]
async fn empty_ssh_config_host_keeps_local_execute_tool() {
    // Given — no alias, and a file only on this host
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::write(root.join("hello.txt"), "local-only").expect("write");
    let shell = session_shell(root, "");
    let registry = registry();

    // When
    let outcome = execute_tool_on_shell(
        shell.as_ref(),
        "Read",
        r#"{"path":"hello.txt"}"#,
        &registry,
        "local-session",
    )
    .await;

    // Then — LocalShell, never ssh
    assert_eq!(shell.kind(), ShellKind::Local);
    assert!(
        !outcome.is_error,
        "empty alias must keep local Read; got: {}",
        outcome.error_message
    );
    let parsed: serde_json::Value = serde_json::from_str(&outcome.result_json).expect("json");
    assert_eq!(
        parsed.get("content").and_then(|v| v.as_str()),
        Some("local-only")
    );
}

#[tokio::test]
async fn ssh_failure_is_an_error_not_a_local_read() {
    // Given — a file on this host that must not leak through a failed ssh
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("secret.txt"), "LOCAL_SECRET").expect("write");
    let shell = RemoteShell::new(
        "no-such-host-tddy-exec",
        tmp.path().to_string_lossy().into_owned(),
    );
    let registry = registry();

    // When
    let outcome = execute_tool_on_shell(
        &shell,
        "Read",
        r#"{"path":"secret.txt"}"#,
        &registry,
        "ssh-fail-session",
    )
    .await;

    // Then — BatchMode failure is the result; the local bytes are not
    assert!(outcome.is_error, "failed ssh must be an error outcome");
    assert!(
        !outcome.result_json.contains("LOCAL_SECRET"),
        "failed ssh must not return the local file; got: {}",
        outcome.result_json
    );
    assert!(
        !outcome.error_message.contains("LOCAL_SECRET"),
        "failed ssh must not echo the local file; got: {}",
        outcome.error_message
    );
    let looks_like_ssh = outcome.error_message.contains("ssh")
        || outcome.error_message.contains("BatchMode")
        || outcome.error_message.contains("Could not resolve")
        || outcome.error_message.contains("Connection refused")
        || outcome.error_message.contains("Permission denied");
    assert!(
        looks_like_ssh,
        "error must be an OpenSSH failure, not a local fallback; got: {}",
        outcome.error_message
    );
}

#[tokio::test]
async fn local_shell_read_is_contained_against_the_worktree_root() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("ok.txt"), "inside").expect("write");
    let shell = LocalShell::new(tmp.path().to_path_buf());

    // When
    let contents = shell.read_to_string("ok.txt").await.expect("in-tree read");

    // Then
    assert_eq!(contents, "inside");
}

#[test]
fn session_shell_picks_remote_when_an_alias_is_set() {
    // Given / When
    let tmp = tempfile::tempdir().expect("tempdir");
    let shell = session_shell(tmp.path().to_path_buf(), "buildbox");

    // Then
    assert_eq!(shell.kind(), ShellKind::Remote);
    assert_eq!(shell.root(), tmp.path().to_string_lossy());
}
