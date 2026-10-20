//! Acceptance tests: a start waits for its bridge to say whether the desktop came up.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-bridge-ready.md` (`#host-desktops` 6/8).
//!
//! A bridge writes one readiness line to its stdout (`tddy_screenshare::readiness`). The daemon
//! reads it with a bounded wait: a refused sign-in fails the start as `Unauthenticated` with the
//! desktop's reason, a bridge that exits or stays silent fails it too, every failed bridge is
//! stopped, and a started desktop reports the size its bridge saw instead of 1920×1080.
//!
//! The bridge is a `/bin/sh` stand-in scripted per test (`common::TheBridge`); everything from the
//! RPC down to the spawn is the production path. Placed outside `screen_sharing_service.rs` so
//! node 1's split of that file does not move them.

#![cfg(unix)]

mod common;

use std::time::Duration;

use common::{a_daemon_whose_bridge, TheBridge, A_HOST};
use tddy_rpc::Code;

const THE_DESKTOPS_REASON: &str = "the password is not right for this desktop";

/// A desktop that is not full HD, so its size cannot be mistaken for the old 1920×1080 guess.
const A_DESKTOP_THAT_IS_NOT_FULL_HD: (u32, u32) = (1280, 800);

/// Short against the 30 s default, so a test about a bridge that never reports costs seconds rather
/// than the default wait. The stand-in is warmed up first (`tests/common`), so this budget is spent
/// on the readiness wait and not on the first exec of a fresh script.
const A_SHORT_READINESS_WAIT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn a_desktop_that_refuses_the_sign_in_fails_the_start_as_unauthenticated() {
    // Given
    let daemon =
        a_daemon_whose_bridge(TheBridge::refusing_the_sign_in_with(THE_DESKTOPS_REASON)).start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    let refusal = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("a desktop that refused the sign-in must not report a started stream");

    // Then
    assert_eq!(refusal.code, Code::Unauthenticated, "{refusal:?}");
    assert_eq!(refusal.message, THE_DESKTOPS_REASON, "{refusal:?}");
}

#[tokio::test]
async fn a_refused_start_leaves_no_bridge_running() {
    // Given
    let daemon =
        a_daemon_whose_bridge(TheBridge::refusing_the_sign_in_with(THE_DESKTOPS_REASON)).start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("a desktop that refused the sign-in must not report a started stream");

    // Then
    daemon.bridge.has_exited().await;
}

#[tokio::test]
async fn a_started_desktop_reports_the_size_its_bridge_saw() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(
        A_DESKTOP_THAT_IS_NOT_FULL_HD.0,
        A_DESKTOP_THAT_IS_NOT_FULL_HD.1,
    ))
    .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    let stream = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("a desktop whose bridge reported ready starts");

    // Then
    assert_eq!((stream.width, stream.height), A_DESKTOP_THAT_IS_NOT_FULL_HD);
}

#[tokio::test]
async fn a_bridge_that_exits_before_reporting_fails_the_start() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ExitsBeforeReporting).start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    let failure = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("a bridge that exited before reporting must not report a started stream");

    // Then
    assert_eq!(failure.code, Code::Unavailable, "{failure:?}");
}

#[tokio::test]
async fn a_bridge_that_never_reports_fails_the_start_and_is_stopped() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::NeverReports)
        .waiting_for_readiness_at_most(A_SHORT_READINESS_WAIT)
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    let failure = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("a bridge that never reported must not report a started stream");

    // Then
    assert_eq!(failure.code, Code::DeadlineExceeded, "{failure:?}");
    daemon.bridge.has_exited().await;
}

#[tokio::test]
async fn a_session_desktop_reports_its_real_size_too() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(
        A_DESKTOP_THAT_IS_NOT_FULL_HD.0,
        A_DESKTOP_THAT_IS_NOT_FULL_HD.1,
    ))
    .start();
    let target_id = daemon.a_session_holding_a_desktop().await;

    // When
    let stream = daemon
        .open_the_session_desktop(&target_id)
        .await
        .expect("a session desktop whose bridge reported ready starts");

    // Then
    assert_eq!((stream.width, stream.height), A_DESKTOP_THAT_IS_NOT_FULL_HD);
}
