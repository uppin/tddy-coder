//! Unit tests: one `acting_identity` resolution at the session edge, as the `GIT_*` pairs a
//! co-located claude-cli session is launched with.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md (M2)

use super::session_acting_identity::*;
use std::collections::BTreeMap;
use std::sync::Arc;
use tddy_accounts::{AccountsError, IdentityError, META_SUBJECT, META_SUBJECT_ID, PROVIDER_GITHUB};
use tddy_core::toolcall::GithubCredentialHandler;
use tddy_credentials::{
    AccountId, CredentialRecord, CredentialStore, ProviderId, SecretString, SessionVaults,
    FIRST_VERSION,
};
use tddy_projects::project_storage::AccountAssignment;

const ADA: &str = "ada";
const ADAS_SESSION: &str = "session-token-for-ada";
const ADAS_PASSPHRASE: &str = "correct horse battery staple";

fn github() -> ProviderId {
    ProviderId::new(PROVIDER_GITHUB)
}

fn a_github_account(account: &str, login: &str, subject_id: &str) -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: AccountId::new(account),
        label: format!("{login}'s account"),
        secret: SecretString::new(format!("ghp_{login}_token")),
        metadata: BTreeMap::from([
            (META_SUBJECT_ID.to_string(), subject_id.to_string()),
            (META_SUBJECT.to_string(), login.to_string()),
        ]),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

fn assigned(account: &str) -> Vec<AccountAssignment> {
    vec![AccountAssignment {
        provider: PROVIDER_GITHUB.to_string(),
        account_id: account.to_string(),
    }]
}

fn pair<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn ada_and_grace_held() -> Vec<CredentialRecord> {
    vec![
        a_github_account("acct-ada", "ada", "101"),
        a_github_account("acct-grace", "grace", "202"),
    ]
}

/// A daemon on which Ada has unlocked a vault holding `records`.
fn a_daemon_where_ada_unlocked(
    records: Vec<CredentialRecord>,
) -> (tempfile::TempDir, SessionAccountAccess) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let passphrase = SecretString::new(ADAS_PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir.path(), ADA), &passphrase, ADA)
            .expect("Ada's vault is created");
    for record in records {
        store.put(record).expect("the record is retained");
    }
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    vaults
        .unlock(ADA, &passphrase)
        .expect("Ada's passphrase opens her vault");
    (dir, access_with(Some(vaults), ADAS_SESSION))
}

fn access_with(vaults: Option<Arc<SessionVaults>>, token: &str) -> SessionAccountAccess {
    SessionAccountAccess::new(
        vaults,
        Arc::new(|t: &str| (t == ADAS_SESSION).then(|| ADA.to_string())),
        token,
    )
}

#[test]
fn an_assigned_held_account_yields_exactly_its_four_git_pairs() {
    // Given a project assigned to Ada's account, which this host holds
    let assignments = assignments_of(&assigned("acct-ada"));

    // When the session's environment is resolved
    let pairs = git_environment_for(&assignments, &ada_and_grace_held()).expect("ada resolves");

    // Then it is her four author and committer variables, and nothing else
    let mut keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "GIT_AUTHOR_EMAIL",
            "GIT_AUTHOR_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_COMMITTER_NAME"
        ]
    );
    assert_eq!(pair(&pairs, "GIT_AUTHOR_NAME"), Some("ada"));
    assert_eq!(
        pair(&pairs, "GIT_COMMITTER_EMAIL"),
        Some("101+ada@users.noreply.github.com")
    );
}

#[test]
fn the_token_of_the_account_is_never_among_the_pairs() {
    // Given a resolvable project
    let assignments = assignments_of(&assigned("acct-ada"));

    // When the session's environment is resolved
    let pairs = git_environment_for(&assignments, &ada_and_grace_held()).expect("ada resolves");

    // Then no value carries the token
    assert!(pairs.iter().all(|(_, v)| !v.contains("ghp_ada_token")));
}

#[test]
fn a_project_with_no_assignment_is_refused_as_not_assigned() {
    // Given a project assigning nothing
    let assignments = assignments_of(&[]);

    // When the session's environment is resolved
    let refusal = git_environment_for(&assignments, &ada_and_grace_held());

    // Then the refusal says no account is assigned
    assert_eq!(
        refusal,
        Err(IdentityError::NotAssigned { provider: github() })
    );
}

#[test]
fn an_assignment_this_host_does_not_hold_is_refused_distinctly_from_no_assignment() {
    // Given a project assigned to an account the vault lacks
    let assignments = assignments_of(&assigned("acct-nobody"));

    // When the session's environment is resolved
    let refusal = git_environment_for(&assignments, &ada_and_grace_held());

    // Then the refusal is UnknownOnThisHost, not NotAssigned
    assert!(matches!(
        refusal,
        Err(IdentityError::UnknownOnThisHost { .. })
    ));
}

#[test]
fn a_github_token_in_the_process_environment_does_not_change_the_result() {
    // Given GITHUB_TOKEN exported in the daemon's environment
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_environment");

    // When an unassigned project and an assigned one are resolved
    let unassigned = git_environment_for(&assignments_of(&[]), &ada_and_grace_held());
    let resolved = git_environment_for(
        &assignments_of(&assigned("acct-ada")),
        &ada_and_grace_held(),
    )
    .expect("ada resolves");

    // Then the environment rescued nothing and leaked into nothing
    assert!(unassigned.is_err());
    assert!(resolved
        .iter()
        .all(|(_, v)| !v.contains("from_the_environment")));
}

#[test]
fn the_vault_backed_resolution_yields_the_assigned_accounts_pairs() {
    // Given Ada's unlocked vault holding her account
    let (_dir, access) = a_daemon_where_ada_unlocked(ada_and_grace_held());

    // When her session's start resolves a project assigned to Grace's account
    let pairs = access
        .git_environment(&assigned("acct-grace"))
        .expect("grace resolves");

    // Then the commits are Grace's
    assert_eq!(pair(&pairs, "GIT_AUTHOR_NAME"), Some("grace"));
}

#[test]
fn without_vaults_the_refusal_says_so_and_the_session_gets_no_pairs() {
    // Given a start with no vaults wired
    let access = access_with(None, ADAS_SESSION);

    // When it resolves an assigned project
    let refusal = access.git_environment(&assigned("acct-ada"));
    let inherited = access.git_environment_or_inherited("s1", &assigned("acct-ada"));

    // Then the refusal is named, and the session still proceeds without GIT_* variables
    assert_eq!(refusal, Err(SessionIdentityRefusal::NoVaultsConfigured));
    assert!(inherited.is_empty());
}

#[test]
fn a_token_no_session_owns_is_refused_as_such_and_the_session_gets_no_pairs() {
    // Given a start carrying a token no session owns
    let dir = tempfile::tempdir().expect("a temporary directory");
    let access = access_with(
        Some(Arc::new(SessionVaults::new(dir.path()))),
        "stale-token",
    );

    // When it resolves an assigned project
    let refusal = access.git_environment(&assigned("acct-ada"));

    // Then the refusal is NoSuchSession, distinct from a locked or missing vault
    assert_eq!(
        refusal,
        Err(SessionIdentityRefusal::Vault(AccountsError::NoSuchSession))
    );
    assert!(access
        .git_environment_or_inherited("s1", &assigned("acct-ada"))
        .is_empty());
}

#[test]
fn an_unassigned_project_in_an_open_vault_starts_without_pairs() {
    // Given Ada's unlocked vault and a project assigning no account
    let (_dir, access) = a_daemon_where_ada_unlocked(ada_and_grace_held());

    // When the start asks for the environment
    let refusal = access.git_environment(&[]);
    let inherited = access.git_environment_or_inherited("s1", &[]);

    // Then the refusal is the identity's, and no variables are added
    assert_eq!(
        refusal,
        Err(SessionIdentityRefusal::Identity(
            IdentityError::NotAssigned { provider: github() }
        ))
    );
    assert!(inherited.is_empty());
}

#[test]
fn the_two_vault_refusals_read_differently() {
    // Given the refusals of a missing session and a locked vault
    let no_session = SessionIdentityRefusal::Vault(AccountsError::NoSuchSession).to_string();
    let locked = SessionIdentityRefusal::Vault(AccountsError::Locked).to_string();

    // Then their messages differ
    assert_ne!(no_session, locked);
}

/// The handler a session's listener carries, for a project assigning `accounts`, over Ada's vault.
fn credential_handler_over(
    access: SessionAccountAccess,
    accounts: &[AccountAssignment],
) -> SessionGithubCredential {
    SessionGithubCredential::new(access, accounts)
}

#[tokio::test]
async fn the_credential_handler_returns_the_token_of_the_account_the_project_assigns() {
    // Given Ada's unlocked vault holding two accounts, and a project assigned to Grace's
    let (_dir, access) = a_daemon_where_ada_unlocked(ada_and_grace_held());
    let handler = credential_handler_over(access, &assigned("acct-grace"));

    // When the agent asks for its GitHub token
    let token = handler.github_token().await;

    // Then it is Grace's, not Ada's
    assert_eq!(token, Ok("ghp_grace_token".to_string()));
}

#[tokio::test]
async fn the_credential_handler_refuses_an_unassigned_project_with_the_resolvers_message() {
    // Given a project that assigns no account
    let (_dir, access) = a_daemon_where_ada_unlocked(ada_and_grace_held());
    let handler = credential_handler_over(access, &[]);

    // When the agent asks for its GitHub token
    let token = handler.github_token().await;

    // Then the refusal is the NotAssigned message
    assert_eq!(
        token,
        Err(IdentityError::NotAssigned { provider: github() }.to_string())
    );
}

#[tokio::test]
async fn the_credential_handler_tells_an_unheld_assignment_apart_from_no_assignment() {
    // Given a project assigned to an account this host does not hold
    let (_dir, access) = a_daemon_where_ada_unlocked(ada_and_grace_held());
    let handler = credential_handler_over(access, &assigned("acct-nobody"));

    // When the agent asks for its GitHub token
    let token = handler.github_token().await;

    // Then the refusal is UnknownOnThisHost's message, which is not NotAssigned's
    let unknown = IdentityError::UnknownOnThisHost {
        provider: github(),
        account: AccountId::new("acct-nobody"),
    }
    .to_string();
    assert_eq!(token, Err(unknown));
    assert_ne!(
        IdentityError::NotAssigned { provider: github() }.to_string(),
        token.unwrap_err()
    );
}

#[tokio::test]
async fn a_github_token_in_the_daemons_environment_rescues_nothing_and_replaces_nothing() {
    // Given GITHUB_TOKEN exported in the daemon's environment
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_environment");
    let (_dir, access) = a_daemon_where_ada_unlocked(ada_and_grace_held());

    // When an unassigned and an assigned project each ask for a token
    let unassigned = credential_handler_over(access.clone(), &[])
        .github_token()
        .await;
    let assigned = credential_handler_over(access, &assigned("acct-ada"))
        .github_token()
        .await;

    // Then the first is refused and the second is the account's own
    assert!(unassigned.is_err());
    assert_eq!(assigned, Ok("ghp_ada_token".to_string()));
}

#[tokio::test]
async fn the_credential_handler_names_a_vault_it_cannot_read() {
    // Given a session start that carried no vaults
    let handler = credential_handler_over(access_with(None, ADAS_SESSION), &assigned("acct-ada"));

    // When the agent asks for its GitHub token
    let token = handler.github_token().await;

    // Then the refusal says no vault is available
    assert_eq!(
        token,
        Err(SessionIdentityRefusal::NoVaultsConfigured.to_string())
    );
}

#[tokio::test]
async fn each_call_reads_the_vault_as_it_stands() {
    // Given a handler over a vault Ada has not unlocked yet
    let dir = tempfile::tempdir().expect("a temporary directory");
    let passphrase = SecretString::new(ADAS_PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir.path(), ADA), &passphrase, ADA)
            .expect("Ada's vault is created");
    store
        .put(a_github_account("acct-ada", "ada", "101"))
        .expect("the record is retained");
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    let handler = credential_handler_over(
        access_with(Some(Arc::clone(&vaults)), ADAS_SESSION),
        &assigned("acct-ada"),
    );
    let while_locked = handler.github_token().await;

    // When she unlocks it and the agent asks again
    vaults
        .unlock(ADA, &passphrase)
        .expect("the passphrase opens it");
    let once_unlocked = handler.github_token().await;

    // Then the first answer was a refusal and the second is the token
    assert_eq!(
        while_locked,
        Err(SessionIdentityRefusal::Vault(AccountsError::Locked).to_string())
    );
    assert_eq!(once_unlocked, Ok("ghp_ada_token".to_string()));
}
