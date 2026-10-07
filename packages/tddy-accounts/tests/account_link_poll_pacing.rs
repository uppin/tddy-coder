//! A poll that arrives before the provider's interval has passed is answered without the provider.
//!
//! The interval is the provider's rule, and the daemon is the one holding the provider's rate limit.
//! A client that ignores the interval — a buggy page, a second tab — must not be able to spend it.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use tddy_accounts::{
    AccountLinker, AccountStore, AccountsError, AccountsServiceImpl, Clock, LinkChallenge,
    LinkError, LinkProgress, LinkedAccountStore,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};
use tddy_rpc::Request;
use tddy_service::proto::accounts::{
    AccountsService, BeginLinkAccountRequest, LinkState, PollLinkAccountRequest,
    PollLinkAccountResponse,
};

const ADAS_SESSION: &str = "session-token-for-ada";
const LINK_ID: &str = "link-1";
const WINDOW_SECONDS: u64 = 900;
const INTERVAL_SECONDS: u64 = 5;
const WIDENED_INTERVAL_SECONDS: u64 = 10;

/// A provider that is never approved yet, counts how often it was asked, and answers each poll
/// with the interval it was told to.
struct AProviderThatIsNeverReady {
    asked: AtomicU32,
    interval_seconds: Mutex<u64>,
}

impl AccountLinker for AProviderThatIsNeverReady {
    fn begin(&self, _provider: &ProviderId) -> Result<LinkChallenge, LinkError> {
        Ok(LinkChallenge {
            link_id: LINK_ID.to_string(),
            user_code: "WXYZ-1234".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            expires_in_seconds: WINDOW_SECONDS,
            interval_seconds: INTERVAL_SECONDS,
        })
    }

    fn poll(&self, _link_id: &str) -> Result<LinkProgress, LinkError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(LinkProgress::Pending {
            interval_seconds: *self.interval_seconds.lock().unwrap(),
        })
    }
}

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

/// A clock only the test moves, so no test sleeps.
#[derive(Clone)]
struct AClockThatOnlyMovesWhenTold(Arc<Mutex<Instant>>);

impl AClockThatOnlyMovesWhenTold {
    fn advance_seconds(&self, seconds: u64) {
        *self.0.lock().unwrap() += Duration::from_secs(seconds);
    }
}

struct ABegunLink {
    service: AccountsServiceImpl<AnEmptyVault>,
    provider: Arc<AProviderThatIsNeverReady>,
    clock: AClockThatOnlyMovesWhenTold,
}

impl ABegunLink {
    async fn polled(&self) -> PollLinkAccountResponse {
        self.service
            .poll_link_account(Request::direct(PollLinkAccountRequest {
                session_token: ADAS_SESSION.to_string(),
                link_id: LINK_ID.to_string(),
            }))
            .await
            .expect("the poll is answered")
            .into_inner()
    }

    fn provider_was_asked(&self) -> u32 {
        self.provider.asked.load(Ordering::SeqCst)
    }
}

async fn a_link_begun_and_polled_once() -> ABegunLink {
    let clock = AClockThatOnlyMovesWhenTold(Arc::new(Mutex::new(Instant::now())));
    let provider = Arc::new(AProviderThatIsNeverReady {
        asked: AtomicU32::new(0),
        interval_seconds: Mutex::new(INTERVAL_SECONDS),
    });
    let reading = clock.clone();
    let as_clock: Clock = Arc::new(move || *reading.0.lock().unwrap());
    let service = AccountsServiceImpl::new(Arc::new(AnEmptyVault))
        .with_linking(provider.clone(), Arc::new(AnEmptyVault))
        .with_clock(as_clock);
    service
        .begin_link_account(Request::direct(BeginLinkAccountRequest {
            session_token: ADAS_SESSION.to_string(),
            provider: "github".to_string(),
        }))
        .await
        .expect("the link begins");
    let link = ABegunLink {
        service,
        provider,
        clock,
    };
    link.polled().await;
    link
}

#[tokio::test]
async fn a_poll_before_the_interval_is_pending_without_asking_the_provider() {
    // Given a link whose provider was just asked
    let link = a_link_begun_and_polled_once().await;
    link.clock.advance_seconds(INTERVAL_SECONDS - 1);

    // When it is polled one second early
    let answer = link.polled().await;

    // Then it is pending, and the provider was not asked again
    assert_eq!(
        (answer.state, link.provider_was_asked()),
        (LinkState::LinkPending as i32, 1)
    );
}

#[tokio::test]
async fn an_early_poll_repeats_the_interval_the_client_should_wait() {
    // Given a link whose provider was just asked
    let link = a_link_begun_and_polled_once().await;

    // When it is polled at once
    let answer = link.polled().await;

    // Then the answer carries the interval
    assert_eq!(answer.interval_seconds, INTERVAL_SECONDS as i64);
}

#[tokio::test]
async fn a_poll_at_the_interval_asks_the_provider_again() {
    // Given a link whose provider was just asked
    let link = a_link_begun_and_polled_once().await;
    link.clock.advance_seconds(INTERVAL_SECONDS);

    // When it is polled once the interval has passed
    link.polled().await;

    // Then the provider was asked a second time
    assert_eq!(link.provider_was_asked(), 2);
}

#[tokio::test]
async fn a_widened_interval_holds_the_next_poll_back_for_longer() {
    // Given a provider that widens the interval when it is asked
    let link = a_link_begun_and_polled_once().await;
    *link.provider.interval_seconds.lock().unwrap() = WIDENED_INTERVAL_SECONDS;
    link.clock.advance_seconds(INTERVAL_SECONDS);
    link.polled().await;

    // When the client polls after the original interval but before the widened one
    link.clock.advance_seconds(INTERVAL_SECONDS);
    let answer = link.polled().await;

    // Then it is held back, and the provider was not asked a third time
    assert_eq!(
        (answer.state, link.provider_was_asked()),
        (LinkState::LinkPending as i32, 2)
    );
}
