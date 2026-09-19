//! How a stored record becomes a git identity — and when it refuses to.

use tddy_accounts::{
    acting_identity, IdentityError, META_SUBJECT, META_SUBJECT_ID, PROVIDER_GITHUB,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, FIRST_VERSION};

fn github() -> ProviderId {
    ProviderId::new(PROVIDER_GITHUB)
}

fn a_github_account_labelled(label: &str) -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: AccountId::new("acct-ada"),
        label: label.to_string(),
        secret: "ghp_ada_token".to_string(),
        metadata: [
            (META_SUBJECT_ID.to_string(), "101".to_string()),
            (META_SUBJECT.to_string(), "ada".to_string()),
        ]
        .into_iter()
        .collect(),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

fn a_github_account_without(key: &str) -> CredentialRecord {
    let mut record = a_github_account_labelled("Ada's work account");
    record.metadata.remove(key);
    record
}

fn assigning_ada() -> Vec<(ProviderId, AccountId)> {
    vec![(github(), AccountId::new("acct-ada"))]
}

#[test]
fn the_git_name_is_the_account_s_login_at_the_provider_not_the_label_a_person_typed() {
    // Given an account a person labelled something else entirely
    let held = vec![a_github_account_labelled("work laptop")];

    // When a project assigned to it resolves who it acts as
    let acting = acting_identity(&assigning_ada(), &github(), &held).expect("ada is assigned");

    // Then the commits are by her login, not by her label
    assert_eq!(acting.git.name, "ada");
}

#[test]
fn renaming_an_account_does_not_change_who_its_commits_are_authored_by() {
    // Given the same account under two different labels
    let before = vec![a_github_account_labelled("work laptop")];
    let after = vec![a_github_account_labelled("Ada — personal")];

    // When each resolves who it acts as
    let earlier = acting_identity(&assigning_ada(), &github(), &before).expect("ada is assigned");
    let later = acting_identity(&assigning_ada(), &github(), &after).expect("ada is assigned");

    // Then the identity is the same one
    assert_eq!(earlier.git, later.git);
}

#[test]
fn the_git_email_is_the_account_s_no_reply_address_at_the_provider() {
    // Given an account linked with the provider's own identifiers
    let held = vec![a_github_account_labelled("work laptop")];

    // When a project assigned to it resolves who it acts as
    let acting = acting_identity(&assigning_ada(), &github(), &held).expect("ada is assigned");

    // Then the address is the one GitHub itself attributes the commit by
    assert_eq!(acting.git.email, "101+ada@users.noreply.github.com");
}

#[test]
fn a_record_without_the_provider_s_immutable_identifier_is_refused_rather_than_guessed() {
    // Given a record whose subject id never made it into the vault
    let held = vec![a_github_account_without(META_SUBJECT_ID)];

    // When a project assigned to it resolves who it acts as
    let refusal = acting_identity(&assigning_ada(), &github(), &held)
        .expect_err("there is no identity to build");

    // Then the missing key is named rather than invented
    assert_eq!(
        refusal,
        IdentityError::Unusable {
            provider: github(),
            account: AccountId::new("acct-ada"),
            missing: META_SUBJECT_ID,
        }
    );
}

#[test]
fn a_record_without_the_provider_s_login_is_refused_rather_than_guessed() {
    // Given a record whose login never made it into the vault
    let held = vec![a_github_account_without(META_SUBJECT)];

    // When a project assigned to it resolves who it acts as
    let refusal = acting_identity(&assigning_ada(), &github(), &held)
        .expect_err("there is no name to commit under");

    // Then the missing key is named rather than invented
    assert_eq!(
        refusal,
        IdentityError::Unusable {
            provider: github(),
            account: AccountId::new("acct-ada"),
            missing: META_SUBJECT,
        }
    );
}

#[test]
fn an_unassigned_project_and_one_assigned_an_account_this_host_lacks_do_not_read_alike() {
    // Given the two refusals a caller most easily confuses
    let unassigned = IdentityError::NotAssigned { provider: github() }.to_string();
    let elsewhere = IdentityError::UnknownOnThisHost {
        provider: github(),
        account: AccountId::new("acct-grace"),
    }
    .to_string();

    // Then they are told apart by their words alone
    assert_ne!(unassigned, elsewhere);
}

#[test]
fn a_refusal_to_pick_between_two_accounts_names_the_provider_it_could_not_settle() {
    // Given a provider with more than one account assigned
    let refusal = IdentityError::Ambiguous { provider: github() };

    // Then the message says which provider is ambiguous
    assert!(
        refusal.to_string().contains(PROVIDER_GITHUB),
        "the refusal does not name the provider: {refusal}"
    );
}

#[test]
fn a_refusal_for_an_account_this_host_lacks_names_the_account_that_is_missing() {
    // Given an assignment this host cannot honour
    let refusal = IdentityError::UnknownOnThisHost {
        provider: github(),
        account: AccountId::new("acct-grace"),
    };

    // Then the message says which account it is waiting for
    assert!(
        refusal.to_string().contains("acct-grace"),
        "the refusal does not name the account: {refusal}"
    );
}
