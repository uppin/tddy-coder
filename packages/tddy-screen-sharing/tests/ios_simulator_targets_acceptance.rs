//! Acceptance tests: a booted iOS Simulator attached to its Mac's host row starts through the
//! `tddy-ios-emulator` bridge, told which device to stream.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-11-26-ios-simulator-screen-sharing.md` AC-IOS-2, AC-IOS-7.
//!
//! The bridge is the `/bin/sh` stand-in (`common::TheBridge`), installed as the iOS bridge only:
//! the VNC and RDP bridge paths name nothing, so a start routed to the wrong binary fails.

#![cfg(unix)]

mod common;

use common::{a_daemon_whose_bridge, TheBridge, A_HOST};
use pretty_assertions::assert_eq;
use tddy_rpc::Code;

const AN_IPHONE: &str = "8A1C2E4F-1111-4222-8333-944455556666";
const AN_IPHONE_SCREEN: (u32, u32) = (1206, 2622);

#[tokio::test]
async fn a_simulator_starts_through_the_ios_bridge_at_the_size_it_reported() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(AN_IPHONE_SCREEN.0, AN_IPHONE_SCREEN.1))
        .as_the_ios_simulator_bridge_only()
        .start();
    let target_id = daemon.attach_an_ios_simulator_to(A_HOST, AN_IPHONE).await;

    // When
    let stream = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("a simulator whose bridge reported ready starts");

    // Then
    assert_eq!((stream.width, stream.height), AN_IPHONE_SCREEN);
}

#[tokio::test]
async fn the_ios_bridge_is_told_which_simulator_to_stream() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(AN_IPHONE_SCREEN.0, AN_IPHONE_SCREEN.1))
        .as_the_ios_simulator_bridge_only()
        .start();
    let target_id = daemon.attach_an_ios_simulator_to(A_HOST, AN_IPHONE).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("a simulator whose bridge reported ready starts");

    // Then
    let config = daemon.bridge.config_read_from_its_stdin().await;
    assert_eq!(config["device"], AN_IPHONE, "{config}");
    assert_eq!(config["port"], 0, "{config}");
}

#[tokio::test]
async fn a_simulator_addressed_at_another_machine_is_refused() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(AN_IPHONE_SCREEN.0, AN_IPHONE_SCREEN.1))
        .as_the_ios_simulator_bridge_only()
        .start();

    // When
    let refusal = daemon
        .try_to_attach_an_ios_simulator(A_HOST, "10.0.0.7", AN_IPHONE)
        .await
        .expect_err("a simulator is bridged only on the Mac that runs it");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument, "{refusal:?}");
    assert_eq!(
        refusal.message,
        "an iOS Simulator is bridged only on the Mac that runs it, not at 10.0.0.7",
        "{refusal:?}"
    );
}

#[tokio::test]
async fn a_simulator_target_without_a_device_is_refused() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(AN_IPHONE_SCREEN.0, AN_IPHONE_SCREEN.1))
        .as_the_ios_simulator_bridge_only()
        .start();

    // When
    let refusal = daemon
        .try_to_attach_an_ios_simulator(A_HOST, "127.0.0.1", "")
        .await
        .expect_err("an iOS Simulator target must name its device");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument, "{refusal:?}");
    assert_eq!(
        refusal.message,
        "an iOS Simulator target must name a device udid",
        "{refusal:?}"
    );
}

#[tokio::test]
async fn an_attached_simulator_is_listed_with_its_device() {
    // Given
    let daemon = a_daemon_whose_bridge(TheBridge::ready_at(AN_IPHONE_SCREEN.0, AN_IPHONE_SCREEN.1))
        .as_the_ios_simulator_bridge_only()
        .start();
    let target_id = daemon.attach_an_ios_simulator_to(A_HOST, AN_IPHONE).await;

    // When
    let targets = daemon.the_desktops_of(A_HOST).await;

    // Then
    let listed = targets
        .iter()
        .find(|target| target.id == target_id)
        .expect("the attached simulator is listed");
    assert_eq!(listed.device_id, AN_IPHONE);
}
