//! How long an unfinished link attempt is remembered.
//!
//! A person who starts adding an account and closes the tab never polls again. The attempt must not
//! outlive the window the provider gave its code, or every abandoned click stays in the daemon's
//! memory until it restarts.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use pretty_assertions::assert_eq;
use tddy_accounts::{
    AccountLinker, AccountStore, AccountsError, AccountsServiceImpl, LinkChallenge, LinkError,
    LinkProgress, LinkedAccountStore, LinkedIdentity,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};
use tddy_rpc::{Code, Request};
use tddy_service::proto::accounts::{
    AccountsService, BeginLinkAccountRequest, LinkState, PollLinkAccountRequest,
};

const ADAS_SESSION: &str = "session-token-for-ada";

/// Hands out `link-1`, `link-2`, … each valid for `window_seconds`, and approves every poll.
struct AProviderWithAWindowOf {
    window_seconds: u64,
    issued: AtomicU32,
}

fn a_provider_with_a_window_of(window_seconds: u64) -> AProviderWithAWindowOf {
    AProviderWithAWindowOf {
        window_seconds,
        issued: AtomicU32::new(0),
    }
}

impl AccountLinker for AProviderWithAWindowOf {
    fn begin(&self, _provider: &ProviderId) -> Result<LinkChallenge, LinkError> {
        let number = self.issued.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(LinkChallenge {
            link_id: format!("link-{number}"),
            user_code: "WXYZ-1234".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            expires_in_seconds: self.window_seconds,
            interval_seconds: 5,
        })
    }

    fn poll(&self, _link_id: &str) -> Result<LinkProgress, LinkError> {
        Ok(LinkProgress::Approved {
            identity: LinkedIdentity {
                subject_id: "1024".to_string(),
                login: "ada".to_string(),
            },
            access_token: "the-token-ada-linked-with".to_string(),
        })
    }
}

/// An open, empty vault: storage only.
struct AnEmptyVault;

impl LinkedAccountStore for AnEmptyVault {
    fn held(&self, _: &str, _: &ProviderId) -> Result<Vec<CredentialRecord>, LinkError> {
        Ok(Vec::new())
    }

    fn put(&self, _: &str, _: CredentialRecord) -> Result<(), LinkError> {
        Ok(())
    }

    fn session_account(&self, _: &str) -> Result<Option<(ProviderId, AccountId)>, LinkError> {
        Ok(None)
    }
}

impl AccountStore for AnEmptyVault {
    fn list(&self, _: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        Ok(Vec::new())
    }

    fn set_label(
        &self,
        _: &str,
        _: &ProviderId,
        _: &AccountId,
        _: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        Err(AccountsError::Unavailable("not used".to_string()))
    }

    fn remove(&self, _: &str, _: &ProviderId, _: &AccountId) -> Result<(), AccountsError> {
        Ok(())
    }
}

fn a_service_whose_codes_last(window_seconds: u64) -> AccountsServiceImpl<AnEmptyVault> {
    AccountsServiceImpl::new(Arc::new(AnEmptyVault)).with_linking(
        Arc::new(a_provider_with_a_window_of(window_seconds)),
        Arc::new(AnEmptyVault),
    )
}

async fn begin(service: &AccountsServiceImpl<AnEmptyVault>) -> String {
    service
        .begin_link_account(Request::direct(BeginLinkAccountRequest {
            session_token: ADAS_SESSION.to_string(),
            provider: "github".to_string(),
        }))
        .await
        .expect("the link begins")
        .into_inner()
        .link_id
}

async fn poll(
    service: &AccountsServiceImpl<AnEmptyVault>,
    link_id: &str,
) -> Result<LinkState, Code> {
    service
        .poll_link_account(Request::direct(PollLinkAccountRequest {
            session_token: ADAS_SESSION.to_string(),
            link_id: link_id.to_string(),
        }))
        .await
        .map(|response| LinkState::try_from(response.into_inner().state).expect("a known state"))
        .map_err(|status| status.code)
}

#[tokio::test]
async fn an_attempt_polled_inside_its_window_is_still_served() {
    // Given an attempt whose code is good for a quarter of an hour
    let service = a_service_whose_codes_last(900);
    let link_id = begin(&service).await;

    // When it is polled
    let state = poll(&service, &link_id).await;

    // Then the provider is asked, and its approval is what the person sees
    assert_eq!(state, Ok(LinkState::LinkLinked));
}

#[tokio::test]
async fn an_attempt_polled_after_its_window_is_reported_expired_and_not_linked() {
    // Given an attempt whose code has already run out
    let service = a_service_whose_codes_last(0);
    let link_id = begin(&service).await;

    // When it is polled
    let state = poll(&service, &link_id).await;

    // Then it is reported as expired, though the provider would have approved it
    assert_eq!(state, Ok(LinkState::LinkExpired));
}

#[tokio::test]
async fn an_attempt_reported_expired_is_forgotten() {
    // Given an attempt that has been reported expired once
    let service = a_service_whose_codes_last(0);
    let link_id = begin(&service).await;
    poll(&service, &link_id).await.expect("the first poll");

    // When it is polled again
    let state = poll(&service, &link_id).await;

    // Then the daemon no longer knows it
    assert_eq!(state, Err(Code::NotFound));
}

#[tokio::test]
async fn an_abandoned_attempt_is_dropped_when_the_next_one_begins() {
    // Given an attempt that was never polled, whose code has run out
    let service = a_service_whose_codes_last(0);
    let abandoned = begin(&service).await;

    // When another attempt begins
    begin(&service).await;

    // Then the abandoned one is no longer remembered, rather than reported expired
    assert_eq!(poll(&service, &abandoned).await, Err(Code::NotFound));
}
