//! The `cursor-create-chat` conversation-id adapter (`#agent-tools 14/19`): it mints the chat a
//! session owns by running `<binary> create-chat`, and a CLI that never answers cannot hang a start.
//!
//! Feature doc: `docs/ft/daemon/cursor-cli-session.md` § Chat. Each stub is a shell script in a
//! temp dir standing in for Cursor's `agent`.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use tddy_agent_backend::adapters::{
    ConversationMint, ConversationMintError, LaunchAdapters, CURSOR_CREATE_CHAT,
};

/// A stub `agent` in `dir` whose `create-chat` runs `body`.
fn a_stub_agent(dir: &Path, body: &str) -> PathBuf {
    let binary = dir.join("agent");
    std::fs::write(
        &binary,
        format!("#!/bin/sh\n[ \"$1\" = create-chat ] || exit 64\n{body}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

/// Mint through the built-in `cursor-create-chat` with `binary`, run in `workdir`.
async fn minting(
    binary: &Path,
    workdir: &Path,
    deadline: Duration,
) -> Result<String, ConversationMintError> {
    let adapter = LaunchAdapters::builtin()
        .conversation_id(CURSOR_CREATE_CHAT)
        .expect("cursor-create-chat is a built-in launch adapter");
    adapter
        .mint(ConversationMint {
            binary,
            workdir,
            env: &[],
            deadline,
        })
        .await
}

#[test]
fn cursor_create_chat_is_the_one_builtin_conversation_id_adapter() {
    // When
    let names: Vec<String> = LaunchAdapters::builtin()
        .conversation_id_names()
        .map(str::to_string)
        .collect();

    // Then
    assert_eq!(names, vec![CURSOR_CREATE_CHAT.to_string()]);
}

#[tokio::test]
async fn cursor_create_chat_returns_the_bare_chat_id_the_cli_prints_in_the_workdir() {
    // Given — a stub that prints its working directory's name as the chat
    let bin = tempfile::tempdir().unwrap();
    let workdir = tempfile::tempdir().unwrap();
    std::fs::write(workdir.path().join("chat"), "chat-7f3a\n").unwrap();
    let agent = a_stub_agent(bin.path(), "/bin/cat chat");

    // When
    let minted = minting(&agent, workdir.path(), Duration::from_secs(5)).await;

    // Then
    assert_eq!(minted, Ok("chat-7f3a".to_string()));
}

#[tokio::test]
async fn cursor_create_chat_refuses_output_that_is_not_a_bare_chat_id() {
    // Given
    let bin = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(bin.path(), "echo 'Welcome to Cursor Agent'");

    // When
    let minted = minting(&agent, bin.path(), Duration::from_secs(5)).await;

    // Then
    assert_eq!(
        minted,
        Err(ConversationMintError::Failed(format!(
            "`{} create-chat` printed \"Welcome to Cursor Agent\" on stdout, which is not a bare \
             chat id",
            agent.display()
        )))
    );
}

#[tokio::test]
async fn cursor_create_chat_is_killed_at_its_deadline_against_a_cli_that_never_answers() {
    // Given — a `create-chat` that sleeps far past the deadline
    let bin = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(bin.path(), "exec /bin/sleep 3600");
    let deadline = Duration::from_millis(300);

    // When
    let began = Instant::now();
    let minted = minting(&agent, bin.path(), deadline).await;
    let took = began.elapsed();

    // Then — timed out naming the deadline, well before the stub would have answered. The bound is
    // loose (5 s) because a loaded CI host may take a while to reap the killed child.
    assert_eq!(
        (minted, took < Duration::from_secs(5)),
        (
            Err(ConversationMintError::TimedOut { after: deadline }),
            true
        )
    );
}
