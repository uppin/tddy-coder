//! Acceptance tests for the `TmuxLocation` port, node 1/3 of `#target-reconnect`.

use std::sync::Arc;
use std::time::Duration;

use crate::cli_session_manager::{classify_main_exit, MainExit};
use crate::tmux_location::{FakeTmuxLocation, HostTmux, TmuxClient, TmuxLocation, TmuxUnreachable};

#[test]
fn a_client_exit_whose_location_cannot_be_asked_keeps_the_session() {
    let exit = classify_main_exit(Some("tddy-1"), |_| {
        Err(TmuxUnreachable("vault locked".into()))
    });
    assert_eq!(exit, MainExit::ClientOnly);
}

#[test]
fn a_refused_host_probe_no_longer_drops_the_session() {
    // D6: a refused probe (tmux missing, cannot become the user) is Err, never "gone".
    let exit = classify_main_exit(Some("tddy-1"), |_| {
        Err(TmuxUnreachable("cannot run tmux as dev".into()))
    });
    assert_eq!(exit, MainExit::ClientOnly);
}

#[test]
fn a_client_exit_whose_agent_is_gone_ends_the_agent() {
    let exit = classify_main_exit(Some("tddy-1"), |_| Ok(false));
    assert_eq!(exit, MainExit::AgentEnded);
}

#[test]
fn a_client_exit_whose_agent_lives_is_client_only() {
    let exit = classify_main_exit(Some("tddy-1"), |_| Ok(true));
    assert_eq!(exit, MainExit::ClientOnly);
}

#[tokio::test]
async fn the_host_location_attaches_with_tmux_attach_session_exactly() {
    let host = HostTmux::new("dev");
    let client: TmuxClient = host
        .attach_client("tddy-abc")
        .await
        .expect("host attach resolves");
    assert_eq!(
        client.argv[1..],
        ["-T", "clipboard", "attach-session", "-t", "=tddy-abc"]
    );
    assert_eq!(client.argv[0], crate::tmux::tmux_binary());
    assert_eq!(client.run_as.as_deref(), Some("dev"));
    assert!(client.env.is_empty());
    assert_eq!(client.exit_code_in_output, None);
    assert_eq!(host.describe(), "the tmux of dev");
}

#[tokio::test]
async fn a_fake_location_reports_alive_gone_and_unreachable() {
    let fake = FakeTmuxLocation::new(&["tddy-1"]);
    assert!(fake.session_lives("tddy-1").await.unwrap());
    assert!(!fake.session_lives("tddy-2").await.unwrap());
    fake.set_reachable(false);
    assert_eq!(
        fake.session_lives("tddy-1").await,
        Err(TmuxUnreachable(fake.unreachable_reason().to_string()))
    );
    fake.set_reachable(true);
    fake.kill("tddy-1").await.unwrap();
    assert!(!fake.session_lives("tddy-1").await.unwrap());
    // A session already gone is Ok to kill again.
    fake.kill("tddy-1").await.unwrap();
    let client = fake.attach_client("tddy-1").await.unwrap();
    assert_eq!(client.run_as, None);
    assert!(!client.argv.is_empty());
    let _: Arc<dyn TmuxLocation> = Arc::new(fake);
}

#[test]
fn backoff_doubles_from_the_base_and_caps_at_five_minutes() {
    let mut backoff = crate::cli_session_manager::WatchBackoff::new(Duration::from_secs(10));
    let delays: Vec<Duration> = (0..7)
        .map(|_| {
            let delay = backoff.delay();
            backoff.on_unreachable();
            delay
        })
        .collect();
    assert_eq!(
        delays,
        vec![
            Duration::from_secs(10),
            Duration::from_secs(20),
            Duration::from_secs(40),
            Duration::from_secs(80),
            Duration::from_secs(160),
            Duration::from_secs(300),
            Duration::from_secs(300),
        ]
    );
}

#[test]
fn a_location_that_comes_back_alive_resets_the_backoff() {
    let mut backoff = crate::cli_session_manager::WatchBackoff::new(Duration::from_secs(10));
    backoff.on_unreachable();
    backoff.on_unreachable();
    assert_eq!(backoff.delay(), Duration::from_secs(40));
    backoff.on_alive();
    assert_eq!(backoff.delay(), Duration::from_secs(10));
    assert!(!backoff.grace_expired());
}

#[test]
fn an_unreachable_location_releases_the_hold_after_the_grace() {
    let mut backoff = crate::cli_session_manager::WatchBackoff::new(Duration::from_secs(10));
    // 10+20+40+80+160+300+300 = 910 s of consecutive unreachability crosses 15 min.
    for _ in 0..6 {
        backoff.on_unreachable();
        assert!(!backoff.grace_expired());
    }
    backoff.on_unreachable();
    assert!(backoff.grace_expired());
    backoff.on_alive();
    assert!(!backoff.grace_expired());
}
