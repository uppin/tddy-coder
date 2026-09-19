//! Which account a project uses at a provider — and, above all, what happens when none does.
//!
//! The four answers exist because collapsing any two of them puts work under an identity nobody
//! chose. The test that matters most is the one asserting what the resolver *does not* do: with a
//! single GitHub account in the vault and no assignment on the project, the answer is still
//! `NotAssigned`.

use pretty_assertions::assert_eq;
use tddy_accounts::{resolve_account, AccountResolution};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_provider(name: &str) -> ProviderId {
    ProviderId::new(name)
}

fn an_account(id: &str) -> AccountId {
    AccountId::new(id)
}

/// One record as this host's vault holds it. The secret is recognisable so a test that leaked one
/// would say so out loud.
fn a_credential(provider: &str, account: &str) -> CredentialRecord {
    CredentialRecord {
        provider: a_provider(provider),
        account: an_account(account),
        label: format!("{account} at {provider}"),
        secret: format!("shhh-{provider}-{account}"),
        metadata: std::collections::BTreeMap::new(),
        updated_at: 1_700_000_000,
    }
}

/// One entry of a project row's assignment set.
fn an_assignment(provider: &str, account: &str) -> (ProviderId, AccountId) {
    (a_provider(provider), an_account(account))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn an_account_assigned_for_the_provider_and_held_here_resolves_to_that_account() {
    let assignments = vec![an_assignment("github", "ada")];
    let held = vec![a_credential("github", "ada")];

    let resolution = resolve_account(&assignments, &a_provider("github"), &held);

    assert_eq!(resolution, AccountResolution::Assigned(an_account("ada")));
}

#[test]
fn a_project_with_no_assignment_resolves_to_nothing_though_the_vault_holds_one_candidate() {
    let assignments: Vec<(ProviderId, AccountId)> = Vec::new();
    let held = vec![a_credential("github", "ada")];

    let resolution = resolve_account(&assignments, &a_provider("github"), &held);

    assert_eq!(resolution, AccountResolution::NotAssigned);
}

#[test]
fn an_account_assigned_but_not_held_here_is_a_different_answer_from_no_assignment() {
    let assignments = vec![an_assignment("github", "ada")];
    let held = vec![a_credential("github", "bob")];

    let resolution = resolve_account(&assignments, &a_provider("github"), &held);

    assert_eq!(
        resolution,
        AccountResolution::UnknownOnThisHost(an_account("ada"))
    );
}

#[test]
fn two_accounts_assigned_at_one_provider_is_ambiguous_and_names_that_provider() {
    let assignments = vec![
        an_assignment("github", "ada"),
        an_assignment("github", "bob"),
    ];
    let held = vec![a_credential("github", "ada"), a_credential("github", "bob")];

    let resolution = resolve_account(&assignments, &a_provider("github"), &held);

    assert_eq!(
        resolution,
        AccountResolution::Ambiguous(a_provider("github"))
    );
}

#[test]
fn an_assignment_at_another_provider_does_not_answer_for_this_one() {
    let assignments = vec![an_assignment("cloudflare", "zoe")];
    let held = vec![
        a_credential("cloudflare", "zoe"),
        a_credential("github", "ada"),
    ];

    let resolution = resolve_account(&assignments, &a_provider("github"), &held);

    assert_eq!(resolution, AccountResolution::NotAssigned);
}

#[test]
fn each_provider_resolves_to_its_own_assigned_account() {
    let assignments = vec![
        an_assignment("github", "ada"),
        an_assignment("cloudflare", "zoe"),
    ];
    let held = vec![
        a_credential("github", "ada"),
        a_credential("cloudflare", "zoe"),
    ];

    let github = resolve_account(&assignments, &a_provider("github"), &held);
    let cloudflare = resolve_account(&assignments, &a_provider("cloudflare"), &held);

    assert_eq!(
        (github, cloudflare),
        (
            AccountResolution::Assigned(an_account("ada")),
            AccountResolution::Assigned(an_account("zoe"))
        )
    );
}

#[test]
fn a_host_holding_nothing_at_all_still_tells_an_assignment_from_an_absent_one() {
    let held: Vec<CredentialRecord> = Vec::new();

    let assigned = resolve_account(
        &[an_assignment("github", "ada")],
        &a_provider("github"),
        &held,
    );
    let unassigned = resolve_account(&[], &a_provider("github"), &held);

    assert_eq!(
        (assigned, unassigned),
        (
            AccountResolution::UnknownOnThisHost(an_account("ada")),
            AccountResolution::NotAssigned
        )
    );
}
