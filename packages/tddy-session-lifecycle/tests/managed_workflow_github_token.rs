//! Integration tests: a managed session's toolcall socket answers `github-token` from the handler
//! its listener was started with, and from nothing else.
//!
//! The token reaches the agent's tools over `TDDY_SOCKET` per call — never an environment variable
//! or a file. Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::toolcall::{dispatch_toolcall, GithubCredentialHandler};
use tddy_session_lifecycle::session_toolcall::{set_up_managed_workflow, ManagedWorkflow};
use tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name;

/// Answers every `github-token` call with a fixed outcome.
struct FixedCredential(Result<String, String>);

#[async_trait]
impl GithubCredentialHandler for FixedCredential {
    async fn github_token(&self) -> Result<String, String> {
        self.0.clone()
    }
}

struct Session {
    workflow: ManagedWorkflow,
    _dirs: Vec<tempfile::TempDir>,
}

fn a_managed_session_whose_credential_is(credential: Option<Result<String, String>>) -> Session {
    let session_dir = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let tddy_data = tempfile::tempdir().unwrap();
    let socket_dir = tempfile::tempdir().unwrap();
    write_changeset(session_dir.path(), &Changeset::default()).unwrap();
    let handler: Option<Arc<dyn GithubCredentialHandler>> =
        credential.map(|c| Arc::new(FixedCredential(c)) as Arc<dyn GithubCredentialHandler>);
    let workflow = set_up_managed_workflow(
        "github-token-session",
        resolve_workflow_recipe_from_cli_name("tdd").expect("recipe must resolve"),
        session_dir.path(),
        worktree.path(),
        tddy_data.path(),
        socket_dir.path(),
        None,
        None,
        handler,
    )
    .expect("set_up_managed_workflow must succeed");
    Session {
        workflow,
        _dirs: vec![session_dir, worktree, tddy_data, socket_dir],
    }
}

#[tokio::test]
async fn a_session_socket_answers_github_token_from_its_credential_handler() {
    // Given a managed session whose project resolves to a token
    let session = a_managed_session_whose_credential_is(Some(Ok("ghp_project_account".into())));

    // When the agent's tool asks over the session's socket
    let response = dispatch_toolcall(
        session.workflow.listener.socket_path(),
        serde_json::json!({"type": "github-token"}),
    )
    .await
    .expect("the socket answers");

    // Then it receives that token
    assert_eq!(response["status"], "ok");
    assert_eq!(response["token"], "ghp_project_account");
}

#[tokio::test]
async fn a_session_socket_relays_the_refusal_of_its_credential_handler() {
    // Given a managed session whose project assigns no account
    let session = a_managed_session_whose_credential_is(Some(Err("no account assigned".into())));

    // When the agent's tool asks over the session's socket
    let response = dispatch_toolcall(
        session.workflow.listener.socket_path(),
        serde_json::json!({"type": "github-token"}),
    )
    .await
    .expect("the socket answers");

    // Then the refusal arrives as an error carrying the reason
    assert_eq!(response["status"], "error");
    assert_eq!(response["message"], "no account assigned");
}

#[tokio::test]
async fn a_session_socket_without_a_credential_handler_refuses() {
    // Given a managed session started with no credential handler
    let session = a_managed_session_whose_credential_is(None);

    // When the agent's tool asks over the session's socket
    let response = dispatch_toolcall(
        session.workflow.listener.socket_path(),
        serde_json::json!({"type": "github-token"}),
    )
    .await
    .expect("the socket answers");

    // Then it is an error, never a token
    assert_eq!(response["status"], "error");
    assert!(response.get("token").is_none());
}
