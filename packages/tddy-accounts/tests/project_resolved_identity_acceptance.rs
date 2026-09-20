//! Who a project's session acts as at GitHub — and the ways it can act as nobody.
//!
//! Every one of these goes through [`acting_identity`], because the whole point of the node is
//! that there is one way in. A test that reached for a token by one route and an identity by
//! another would be testing a shape this crate deliberately does not publish.

use tddy_accounts::{
    acting_identity, ActingIdentity, GitIdentity, IdentityError, META_SUBJECT, META_SUBJECT_ID,
    PROVIDER_GITHUB,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, FIRST_VERSION};

fn github() -> ProviderId {
    ProviderId::new(PROVIDER_GITHUB)
}

/// A GitHub credential as `#keyring` 8/9's linking writes one.
fn a_github_account(account: &str, subject_id: &str, login: &str, token: &str) -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: AccountId::new(account),
        label: format!("{login}'s account"),
        secret: token.to_string(),
        metadata: [
            (META_SUBJECT_ID.to_string(), subject_id.to_string()),
            (META_SUBJECT.to_string(), login.to_string()),
        ]
        .into_iter()
        .collect(),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

/// One project row's assignment of `account` at GitHub.
fn a_project_assigning(account: &str) -> Vec<(ProviderId, AccountId)> {
    vec![(github(), AccountId::new(account))]
}

fn a_vault_holding_ada_and_grace() -> Vec<CredentialRecord> {
    vec![
        a_github_account("acct-ada", "101", "ada", "ghp_ada_token"),
        a_github_account("acct-grace", "202", "grace", "ghp_grace_token"),
    ]
}

/// The load-bearing one: a token and an identity that could disagree, and must not.
#[test]
fn the_token_and_the_git_identity_name_the_same_account() {
    // Given a host holding two GitHub accounts
    let held = a_vault_holding_ada_and_grace();

    // And a project that assigns the second of them
    let assignments = a_project_assigning("acct-grace");

    // When the session resolves who it acts as
    let acting = acting_identity(&assignments, &github(), &held).expect("grace is assigned");

    // Then one account answered, in both halves at once
    assert_eq!(
        acting,
        ActingIdentity {
            account: AccountId::new("acct-grace"),
            token: "ghp_grace_token".to_string(),
            git: GitIdentity {
                name: "grace".to_string(),
                email: "202+grace@users.noreply.github.com".to_string(),
            },
        }
    );
}

#[test]
fn two_projects_on_one_daemon_act_as_different_github_users() {
    // Given one host's vault
    let held = a_vault_holding_ada_and_grace();

    // When two projects on it resolve who they act as
    let one = acting_identity(&a_project_assigning("acct-ada"), &github(), &held)
        .expect("ada is assigned to the first project");
    let other = acting_identity(&a_project_assigning("acct-grace"), &github(), &held)
        .expect("grace is assigned to the second project");

    // Then each is the account its own project named
    assert_eq!(
        (one.token.as_str(), one.git.name.as_str()),
        ("ghp_ada_token", "ada")
    );
    assert_eq!(
        (other.token.as_str(), other.git.name.as_str()),
        ("ghp_grace_token", "grace")
    );
}

#[test]
fn a_project_that_assigns_no_account_is_refused_although_the_environment_holds_a_token() {
    // Given a daemon whose own environment carries a GitHub token
    std::env::set_var("GITHUB_TOKEN", "ghp_the_daemon_s_own_token");

    // And a host holding an account that no project assigned
    let held = a_vault_holding_ada_and_grace();

    // When a project with no assignment resolves who it acts as
    let refusal = acting_identity(&[], &github(), &held).expect_err("nothing was assigned");

    // Then it acts as nobody, and the environment rescued nothing
    assert_eq!(refusal, IdentityError::NotAssigned { provider: github() });

    std::env::remove_var("GITHUB_TOKEN");
}

#[test]
fn an_account_this_host_has_never_received_is_refused_in_its_own_words() {
    // Given a host holding only one of the two accounts
    let held = vec![a_github_account("acct-ada", "101", "ada", "ghp_ada_token")];

    // When a project assigning the other one resolves who it acts as
    let refusal = acting_identity(&a_project_assigning("acct-grace"), &github(), &held)
        .expect_err("grace has not reached this host");

    // Then the one account on hand was not offered in her place
    assert_eq!(
        refusal,
        IdentityError::UnknownOnThisHost {
            provider: github(),
            account: AccountId::new("acct-grace"),
        }
    );
}

#[test]
fn two_accounts_assigned_at_one_provider_are_refused_rather_than_one_being_picked() {
    // Given a host holding both accounts
    let held = a_vault_holding_ada_and_grace();

    // And a project row naming both of them at the one provider
    let assignments = vec![
        (github(), AccountId::new("acct-ada")),
        (github(), AccountId::new("acct-grace")),
    ];

    // When it resolves who it acts as
    let refusal = acting_identity(&assignments, &github(), &held)
        .expect_err("two accounts at one provider settle nothing");

    // Then neither was chosen
    assert_eq!(refusal, IdentityError::Ambiguous { provider: github() });
}
