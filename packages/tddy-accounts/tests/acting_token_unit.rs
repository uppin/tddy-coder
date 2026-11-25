//! The token without the identity — what reads and PR-API calls resolve.
//!
//! A PR read, a check run, a comment, opening or updating a PR, assigning one to a work item:
//! none of them authors a commit, so none of them needs a name and email. They resolve through
//! [`acting_token`], which answers the account and its token from the record alone and never looks
//! at the provider metadata — a record linked before `subject_id` existed still yields its token.

use tddy_accounts::{acting_identity, acting_token, IdentityError, PROVIDER_GITHUB};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString, FIRST_VERSION};

fn github() -> ProviderId {
    ProviderId::new(PROVIDER_GITHUB)
}

fn assigning(account: &str) -> Vec<(ProviderId, AccountId)> {
    vec![(github(), AccountId::new(account))]
}

/// Ada's account as the operator's `uppin` row stands: linked before the provider identifiers
/// existed, so its metadata carries neither `subject_id` nor `subject`.
fn a_github_account_without_identifiers() -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: AccountId::new("acct-ada"),
        label: "Ada's work account".to_string(),
        secret: SecretString::new("ghp_ada_token"),
        metadata: Default::default(),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

fn a_github_account_with(account: &str, login: &str, token: &str) -> CredentialRecord {
    let mut record = a_github_account_without_identifiers();
    record.account = AccountId::new(account);
    record.label = format!("{login}'s account");
    record.secret = SecretString::new(token);
    record.metadata.insert(
        tddy_accounts::META_SUBJECT_ID.to_string(),
        format!("id-of-{login}"),
    );
    record
        .metadata
        .insert(tddy_accounts::META_SUBJECT.to_string(), login.to_string());
    record
}

#[test]
fn a_record_with_no_provider_metadata_still_yields_its_token() {
    // Given an account linked before the provider identifiers existed
    let held = vec![a_github_account_without_identifiers()];

    // When a read resolves the project's token
    let token = acting_token(&assigning("acct-ada"), &github(), &held).expect("ada is assigned");

    // Then it is her account and her token — the missing identifiers refuse nothing here
    assert_eq!(token.account, AccountId::new("acct-ada"));
    assert_eq!(token.token, "ghp_ada_token");
}

#[test]
fn identifier_values_present_but_empty_still_yield_the_token() {
    // Given a record whose identifier values are present but empty (the paired
    // resolution refuses these exactly as it refuses absent ones)
    let mut record = a_github_account_without_identifiers();
    record
        .metadata
        .insert(tddy_accounts::META_SUBJECT_ID.to_string(), String::new());
    record
        .metadata
        .insert(tddy_accounts::META_SUBJECT.to_string(), String::new());
    let held = vec![record];
    assert!(acting_identity(&assigning("acct-ada"), &github(), &held).is_err());

    // When a read resolves the project's token
    let token = acting_token(&assigning("acct-ada"), &github(), &held).expect("ada is assigned");

    // Then the empty values are not even looked at
    assert_eq!(token.token, "ghp_ada_token");
}

#[test]
fn the_token_is_the_assigned_account_s_not_the_vault_s_only_one() {
    // Given a vault holding two accounts and a project assigned to Grace's
    let held = vec![
        a_github_account_with("acct-ada", "ada", "ghp_ada_token"),
        a_github_account_with("acct-grace", "grace", "ghp_grace_token"),
    ];

    // When a read resolves the project's token
    let token =
        acting_token(&assigning("acct-grace"), &github(), &held).expect("grace is assigned");

    // Then it is Grace's account and Grace's token
    assert_eq!(token.account, AccountId::new("acct-grace"));
    assert_eq!(token.token, "ghp_grace_token");
}

#[test]
fn renaming_an_account_does_not_change_its_token() {
    // Given the same account under two different labels
    let mut before = a_github_account_with("acct-ada", "ada", "ghp_ada_token");
    before.label = "work laptop".to_string();
    let mut after = before.clone();
    after.label = "Ada — personal".to_string();

    // When each resolves the project's token
    let first =
        acting_token(&assigning("acct-ada"), &github(), &[before]).expect("ada is assigned");
    let second =
        acting_token(&assigning("acct-ada"), &github(), &[after]).expect("ada is assigned");

    // Then both are her token
    assert_eq!(first, second);
}

#[test]
fn an_unassigned_project_is_refused_as_not_assigned() {
    // Given a vault holding Ada's account and a project assigning nothing
    let held = vec![a_github_account_without_identifiers()];

    // When a read resolves the project's token
    let refusal = acting_token(&[], &github(), &held).expect_err("nothing was assigned");

    // Then the refusal is NotAssigned, in the resolver's own words
    assert_eq!(refusal, IdentityError::NotAssigned { provider: github() });
}

#[test]
fn an_assignment_this_host_lacks_is_refused_as_unknown_on_this_host() {
    // Given a vault holding Ada's account and a project assigned to Grace's
    let held = vec![a_github_account_without_identifiers()];

    // When a read resolves the project's token
    let refusal =
        acting_token(&assigning("acct-grace"), &github(), &held).expect_err("grace is not held");

    // Then the refusal names the missing account
    assert_eq!(
        refusal,
        IdentityError::UnknownOnThisHost {
            provider: github(),
            account: AccountId::new("acct-grace"),
        }
    );
}

#[test]
fn two_assignments_at_one_provider_are_refused_as_ambiguous() {
    // Given two accounts assigned at the one provider
    let held = vec![
        a_github_account_with("acct-ada", "ada", "ghp_ada_token"),
        a_github_account_with("acct-grace", "grace", "ghp_grace_token"),
    ];
    let assignments = vec![
        (github(), AccountId::new("acct-ada")),
        (github(), AccountId::new("acct-grace")),
    ];

    // When a read resolves the project's token
    let refusal = acting_token(&assignments, &github(), &held).expect_err("never picked");

    // Then the refusal is Ambiguous, never a choice
    assert_eq!(refusal, IdentityError::Ambiguous { provider: github() });
}
