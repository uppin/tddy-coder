//! A run wedged *inside* one language-server request must still be interruptible.
//!
//! `tests/cancellation_acceptance.rs` pins the waits *between* requests: those are synchronous poll
//! loops that look at the token beside each sleep. This suite pins the other half, and the one the
//! token could not reach — the moment a request is in flight. The bridge used to drive
//! `LspClient::request_raw`, which nothing but its own per-request bound ends, so a caller who
//! pressed `^C` a second time was held until that bound expired. Driving `request_abandonable` with
//! the run's token is what closes it.
//!
//! `fake_lsp`'s `tddy/neverAnswers` is the wedge: it receives the request and sends nothing back,
//! so the only two things that can end the wait are the per-request bound and the caller.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tddy_code_restructuring::backends::LspClientBridge;
use tddy_code_restructuring::{client_capabilities, server_settings, RestructureError};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// The per-request bound a run installs on the shared client, near enough to production's ten
/// minutes that a wait ending on it rather than on the token is unmistakable in the timing below.
const LONGER_THAN_ANY_TEST_WILL_WAIT: Duration = Duration::from_secs(600);

/// How long an abandoned request may take to come back before the test calls it wedged. Generous
/// next to the single `select!` that ends it, negligible next to the bound above.
const ABANDONMENT_UNWIND: Duration = Duration::from_secs(5);

/// A bridge onto a fake server, with the per-request bound raised to production's, so that nothing
/// but `cancel` can end a request the server never answers.
async fn a_bridge_onto_a_server_that_never_answers(
    root: &Path,
    cancel: CancellationToken,
) -> LspClientBridge {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(client_capabilities())
            .with_initialization_options(server_settings()),
    );
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: Language::Rust,
        })
        .await
        .expect("the fake language server starts");
    service
        .client
        .set_request_timeout(LONGER_THAN_ANY_TEST_WILL_WAIT);
    LspClientBridge::new(Arc::clone(&service.client), cancel)
}

/// Issue the request the fake never answers, on a blocking thread — the way the backend drives it.
fn a_request_the_server_never_answers(
    bridge: LspClientBridge,
) -> tokio::task::JoinHandle<Result<serde_json::Value, RestructureError>> {
    tokio::task::spawn_blocking(move || bridge.request("tddy/neverAnswers", json!({})))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_in_flight_is_abandoned_as_soon_as_the_run_is_cancelled() {
    // Given a request in flight to a server that will never answer it
    let root = PathBuf::from("/workspace");
    let cancel = CancellationToken::new();
    let bridge = a_bridge_onto_a_server_that_never_answers(&root, cancel.clone()).await;
    let wedged = a_request_the_server_never_answers(bridge);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // When the run is cancelled
    let cancelled_at = Instant::now();
    cancel.cancel();
    let outcome = tokio::time::timeout(ABANDONMENT_UNWIND, wedged)
        .await
        .expect("an abandoned request comes back rather than waiting out its own bound")
        .expect("the blocking half joins");

    // Then it came back on the token rather than on the ten-minute bound
    assert!(
        cancelled_at.elapsed() < ABANDONMENT_UNWIND,
        "the request held its thread for {:?} after the run was cancelled",
        cancelled_at.elapsed()
    );
    // and it reports the caller having stopped, which is not a fault of the server's
    let failure = outcome.expect_err("an abandoned request is not an answer");
    assert!(
        matches!(failure, RestructureError::CallerStopped),
        "an abandoned request was reported as {failure:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_already_cancelled_is_never_waited_on_at_all() {
    // Given a run whose caller has already stopped
    let root = PathBuf::from("/workspace");
    let cancel = CancellationToken::new();
    let bridge = a_bridge_onto_a_server_that_never_answers(&root, cancel.clone()).await;
    cancel.cancel();

    // When a request that would never be answered is issued
    let started = Instant::now();
    let outcome = tokio::time::timeout(
        ABANDONMENT_UNWIND,
        a_request_the_server_never_answers(bridge),
    )
    .await
    .expect("a cancelled run does not wait for a server")
    .expect("the blocking half joins");

    // Then it comes straight back, reporting the caller rather than the server
    assert!(
        started.elapsed() < ABANDONMENT_UNWIND,
        "a cancelled run still waited {:?} for a server",
        started.elapsed()
    );
    let failure = outcome.expect_err("an abandoned request is not an answer");
    assert!(
        matches!(failure, RestructureError::CallerStopped),
        "an abandoned request was reported as {failure:?}"
    );
}
