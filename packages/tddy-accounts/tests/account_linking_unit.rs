//! What linking decides before anything is written.
//!
//! Two decisions live here, and both fail silently when they are wrong. Deduplicating on the login
//! name instead of the provider's id eventually attaches a stranger's token to a person's project,
//! because a released GitHub login can be re-registered. Minting a fresh account id on a re-link
//! leaves every `#keyring` 5/9 project assignment pointing at nothing, and the resolver then
//! answers `NotAssigned` — which is exactly what it answers when nobody ever assigned one.
//!
//! Neither decision needs a sealed vault, so neither is tested through one.

use std::collections::BTreeMap;

use pretty_assertions::assert_eq;
use tddy_accounts::{
    record_for_link, removal_allowed, LinkedIdentity, RemovalRefusal, META_SUBJECT, META_SUBJECT_ID,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, FIRST_VERSION};

const GITHUB: &str = "github";
const WHEN_THE_LINK_COMPLETED: u64 = 1_758_240_000;

// ---------------------------------------------------------------------------------------------
// Builders

fn github() -> ProviderId {
    ProviderId::new(GITHUB)
}

/// Ada, as the provider describes her the first time she approves a link.
fn ada() -> LinkedIdentity {
    LinkedIdentity {
        subject_id: "1024".to_string(),
        login: "ada".to_string(),
    }
}

/// The same person, after she renames herself at the provider. Same id, different handle.
fn ada_renamed() -> LinkedIdentity {
    LinkedIdentity {
        subject_id: "1024".to_string(),
        login: "ada-lovelace".to_string(),
    }
}

/// Somebody else entirely.
fn grace() -> LinkedIdentity {
    LinkedIdentity {
        subject_id: "2048".to_string(),
        login: "grace".to_string(),
    }
}

/// A record the vault already holds for `identity`, under the account id and label it was given
/// when it was first linked.
fn a_held_account(identity: &LinkedIdentity, account: &str, label: &str) -> CredentialRecord {
    let mut metadata = BTreeMap::new();
    metadata.insert(META_SUBJECT_ID.to_string(), identity.subject_id.clone());
    metadata.insert(META_SUBJECT.to_string(), identity.login.clone());

    CredentialRecord {
        provider: github(),
        account: AccountId::new(account),
        label: label.to_string(),
        secret: "the-token-from-the-first-link".to_string(),
        metadata,
        updated_at: 1_726_700_000,
        version: FIRST_VERSION,
    }
}

// ---------------------------------------------------------------------------------------------
// Deduplication

#[test]
fn a_person_the_vault_has_never_seen_becomes_a_new_account() {
    let held = vec![a_held_account(&grace(), "account-grace", "Work")];

    let linked = record_for_link(
        &held,
        &github(),
        &ada(),
        "ada-s-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(linked.provider, github());
    assert_ne!(linked.account, AccountId::new("account-grace"));
}

#[test]
fn the_same_person_under_a_changed_login_name_is_the_same_account() {
    let held = vec![a_held_account(&ada(), "account-ada", "Personal")];

    let linked = record_for_link(
        &held,
        &github(),
        &ada_renamed(),
        "a-fresh-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(linked.account, AccountId::new("account-ada"));
}

#[test]
fn the_changed_login_name_is_what_the_record_now_shows() {
    let held = vec![a_held_account(&ada(), "account-ada", "Personal")];

    let linked = record_for_link(
        &held,
        &github(),
        &ada_renamed(),
        "a-fresh-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(
        linked.metadata.get(META_SUBJECT),
        Some(&"ada-lovelace".to_string())
    );
}

#[test]
fn the_provider_s_own_id_travels_with_the_record() {
    let linked = record_for_link(
        &[],
        &github(),
        &ada(),
        "ada-s-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(
        linked.metadata.get(META_SUBJECT_ID),
        Some(&"1024".to_string())
    );
}

// ---------------------------------------------------------------------------------------------
// What a re-link preserves

#[test]
fn re_linking_keeps_the_label_a_person_gave_the_account() {
    let held = vec![a_held_account(&ada(), "account-ada", "Personal")];

    let linked = record_for_link(
        &held,
        &github(),
        &ada_renamed(),
        "a-fresh-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(linked.label, "Personal");
}

#[test]
fn re_linking_replaces_the_credential() {
    let held = vec![a_held_account(&ada(), "account-ada", "Personal")];

    let linked = record_for_link(
        &held,
        &github(),
        &ada(),
        "a-fresh-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(linked.secret, "a-fresh-token");
}

#[test]
fn a_linked_record_carries_the_moment_it_was_linked() {
    let linked = record_for_link(
        &[],
        &github(),
        &ada(),
        "ada-s-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_eq!(linked.updated_at, WHEN_THE_LINK_COMPLETED);
}

#[test]
fn an_account_linked_at_one_provider_does_not_deduplicate_against_another() {
    let mut elsewhere = a_held_account(&ada(), "account-ada", "Personal");
    elsewhere.provider = ProviderId::new("cloudflare");

    let linked = record_for_link(
        &[elsewhere],
        &github(),
        &ada(),
        "ada-s-token",
        WHEN_THE_LINK_COMPLETED,
    );

    assert_ne!(linked.account, AccountId::new("account-ada"));
}

// ---------------------------------------------------------------------------------------------
// What may be forgotten

#[test]
fn the_account_this_session_was_established_with_cannot_be_forgotten() {
    let ada_s_account = AccountId::new("account-ada");

    let allowed = removal_allowed(
        (&github(), &ada_s_account),
        Some((&github(), &ada_s_account)),
    );

    assert_eq!(allowed, Err(RemovalRefusal::SessionAccount));
}

#[test]
fn any_other_account_can_be_forgotten() {
    let ada_s_account = AccountId::new("account-ada");
    let grace_s_account = AccountId::new("account-grace");

    let allowed = removal_allowed(
        (&github(), &grace_s_account),
        Some((&github(), &ada_s_account)),
    );

    assert_eq!(allowed, Ok(()));
}

#[test]
fn a_session_that_belongs_to_no_linked_account_may_forget_any_of_them() {
    let ada_s_account = AccountId::new("account-ada");

    let allowed = removal_allowed((&github(), &ada_s_account), None);

    assert_eq!(allowed, Ok(()));
}

#[test]
fn an_account_sharing_an_id_with_the_session_s_at_another_provider_can_be_forgotten() {
    let shared = AccountId::new("ada");

    let allowed = removal_allowed(
        (&ProviderId::new("cloudflare"), &shared),
        Some((&github(), &shared)),
    );

    assert_eq!(allowed, Ok(()));
}
