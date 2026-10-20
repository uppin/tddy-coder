//! Acceptance tests: opening an X11 desktop channel runs x11vnc for that display around the bridge.
//!
//! Feature: `docs/ft/web/screen-sharing-sessions.md` (AC-SS-18, `#host-desktops` 7/8).
//!
//! A host target may name an X11 display instead of a listening desktop. Starting one starts
//! `x11vnc -display :N -localhost -rfbport <free> -nopw -forever`, waits until it accepts, and points
//! the bridge at it; x11vnc is stopped with the bridge, and a reopen leaves exactly one running.
//!
//! x11vnc and the bridge are `/bin/sh` stand-ins (`common::StandInX11vnc`, `common::StandInBridge`);
//! everything from the RPC down to the spawns is the production path. Placed outside
//! `screen_sharing_service.rs` so node 1's split of that file does not move them.

#![cfg(unix)]

mod common;

use std::time::Duration;

use common::{a_daemon_whose_bridge, TheBridge, TheX11vnc, A_HOST};
use tddy_rpc::Code;

const DISPLAY_ONE: u32 = 1;

/// Short, so a test about an x11vnc that never accepts costs a moment rather than the default wait.
///
/// Not shorter: the window must outlast the stand-in's own start (fork, exec of `/bin/sh`, its first
/// `echo` to the launch log), or the daemon kills a stand-in that never logged its launch and the
/// test waits on a launch that cannot appear. The stand-in is warmed before the clock starts
/// (`common::StandInX11vnc`), but on a heavily loaded machine 500 ms still lost that race.
const A_SHORT_ACCEPT_WAIT: Duration = Duration::from_secs(3);

/// How long the reopened channel's x11vnc is watched once the old bridge has exited: the daemon
/// reaps an exited bridge within milliseconds, so a stop that reaping would cause has arrived by
/// then.
const AN_OLD_BRIDGE_HAS_BEEN_REAPED_WITHIN: Duration = Duration::from_secs(1);

fn a_bridge_that_comes_up() -> TheBridge {
    TheBridge::ready_at(1280, 800)
}

#[tokio::test]
async fn opening_an_x11_channel_starts_x11vnc_for_that_display_on_loopback_only() {
    // Given
    let daemon = a_daemon_whose_bridge(a_bridge_that_comes_up()).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("an X11 channel whose x11vnc accepts opens");

    // Then
    daemon
        .x11vnc
        .launch(0)
        .await
        .assert_serves_display(DISPLAY_ONE)
        .assert_listens_on_loopback_only();
}

#[tokio::test]
async fn the_bridge_of_an_x11_channel_connects_to_its_x11vnc() {
    // Given
    let daemon = a_daemon_whose_bridge(a_bridge_that_comes_up()).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("an X11 channel whose x11vnc accepts opens");

    // Then
    let x11vnc_port = daemon.x11vnc.launch(0).await.rfb_port();
    let bridge_config = daemon.bridge.config_read_from_its_stdin().await;
    assert_eq!(
        (&bridge_config["host"], &bridge_config["port"]),
        (
            &serde_json::json!("127.0.0.1"),
            &serde_json::json!(x11vnc_port)
        ),
    );
}

#[tokio::test]
async fn closing_an_x11_channel_stops_its_x11vnc() {
    // Given
    let daemon = a_daemon_whose_bridge(a_bridge_that_comes_up()).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("an X11 channel whose x11vnc accepts opens");
    let x11vnc = daemon.x11vnc.launch(0).await;

    // When
    daemon.close_the_desktop_of(A_HOST, &target_id).await;

    // Then
    daemon.x11vnc.has_exited(&x11vnc).await;
}

#[tokio::test]
async fn reopening_an_x11_channel_leaves_one_x11vnc_running() {
    // Given
    let daemon = a_daemon_whose_bridge(a_bridge_that_comes_up()).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("an X11 channel whose x11vnc accepts opens");
    let first = daemon.x11vnc.launch(0).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("reopening the same X11 channel");

    // Then
    let second = daemon.x11vnc.launch(1).await;
    daemon.x11vnc.has_exited(&first).await;
    assert!(
        tddy_testing_commons::process_is_alive(second.pid),
        "the reopened channel's x11vnc must still be serving it"
    );
}

#[tokio::test]
async fn reopening_an_x11_channel_whose_old_bridge_exits_with_its_x11vnc_keeps_the_new_x11vnc() {
    // Given
    let daemon =
        a_daemon_whose_bridge(TheBridge::ready_at_until_its_x11vnc_stops(1280, 800)).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("an X11 channel whose x11vnc accepts opens");
    let first_bridge = daemon.bridge.pid().await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("reopening the same X11 channel");

    // Then
    daemon.bridge.has_exited_as(first_bridge).await;
    let second = daemon.x11vnc.launch(1).await;
    daemon
        .x11vnc
        .keeps_running(&second, AN_OLD_BRIDGE_HAS_BEEN_REAPED_WITHIN)
        .await;
}

#[tokio::test]
async fn an_x11vnc_that_never_accepts_fails_the_start_and_starts_no_bridge() {
    // Given
    let daemon = a_daemon_whose_bridge(a_bridge_that_comes_up())
        .with_an_x11vnc_that(TheX11vnc::NeverAccepts)
        .waiting_for_x11vnc_at_most(A_SHORT_ACCEPT_WAIT)
        .start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;

    // When
    let failure = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("an X11 channel whose x11vnc never accepted must not report a started stream");

    // Then
    assert_eq!(failure.code, Code::DeadlineExceeded, "{failure:?}");
    assert!(
        !daemon.bridge.was_ever_started(),
        "no bridge may be started for an x11vnc that never accepted"
    );
}

#[tokio::test]
async fn an_x11vnc_that_never_accepts_is_stopped() {
    // Given
    let daemon = a_daemon_whose_bridge(a_bridge_that_comes_up())
        .with_an_x11vnc_that(TheX11vnc::NeverAccepts)
        .waiting_for_x11vnc_at_most(A_SHORT_ACCEPT_WAIT)
        .start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("an X11 channel whose x11vnc never accepted must not report a started stream");

    // Then
    let x11vnc = daemon.x11vnc.launch(0).await;
    daemon.x11vnc.has_exited(&x11vnc).await;
}

#[tokio::test]
async fn an_x11_channel_whose_bridge_fails_to_start_stops_its_x11vnc() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ExitsBeforeReporting).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("an X11 channel whose bridge exited before reporting must not start");

    // Then
    let x11vnc = daemon.x11vnc.launch(0).await;
    daemon.x11vnc.has_exited(&x11vnc).await;
}

#[tokio::test]
async fn an_x11_channel_whose_running_bridge_exits_stops_its_x11vnc() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at_then_exits(1280, 800)).start();
    let target_id = daemon.attach_an_x11_display_to(A_HOST, DISPLAY_ONE).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("an X11 channel whose bridge came up opens");
    let x11vnc = daemon.x11vnc.launch(0).await;

    // When — the bridge exits on its own after reporting ready
    daemon.bridge.has_exited().await;

    // Then
    daemon.x11vnc.has_exited(&x11vnc).await;
}
