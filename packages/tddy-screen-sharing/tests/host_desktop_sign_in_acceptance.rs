//! Acceptance tests: a host desktop's sign-in is shaped by what the desktop advertises, and a
//! credential is kept only when the operator ticks Remember.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-sign-in-form.md` (`#host-desktops` 8/8).
//!
//! `InspectHostDesktop` runs the target's bridge in inspect mode and reports what it read. Opening
//! a desktop asks a `DESKTOP_CREDENTIALS` question unless the operator has a remembered credential
//! for it; an answer with `remember` is sealed in the operator's credential vault under
//! `host:<daemon_instance_id>:<target_id>`, one without is written nowhere, and a remembered
//! credential the desktop refuses is not tried again.
//!
//! The bridge is a `/bin/sh` stand-in scripted per test, the operator answers at once, and the
//! vault is the operator's real sealed one (`common::DaemonBuilder::with_the_operators_vault`):
//! what is and is not written is read back from the vault and from the daemon's storage, not
//! inferred. Placed outside `screen_sharing_service.rs` so node 1's split does not move them.

#![cfg(unix)]

mod common;

use pretty_assertions::assert_eq;

use common::{
    a_credential, a_daemon_whose_bridge, CredentialBuilder, TheBridge, A_HOST, A_VNC_PORT,
};
use tddy_host_service::host_prompts::PromptKind;
use tddy_rpc::Code;
use tddy_screenshare::inspect::AdvertisedMethod;
use tddy_service::proto::screen_sharing::{DesktopAuthMethod, InspectHostDesktopResponse};

const THE_DESKTOP_PASSWORD: &str = "correct horse battery staple";
const THE_OPERATORS_ACCOUNT: &str = "ada";
const THE_WINDOWS_DOMAIN: &str = "CORP";
const THE_DESKTOPS_REASON: &str = "the password has changed on this desktop";

/// Where a remembered host-desktop credential lives in the operator's vault: the `screen-sharing`
/// provider, under an account naming the host and the target, so it never collides with a session
/// target's account.
fn the_remembered_account(host: &str, target_id: &str) -> String {
    format!("host:{host}:{target_id}")
}

/// A desktop that signs in and reports itself ready.
fn a_desktop_that_accepts_the_sign_in() -> TheBridge {
    TheBridge::ready_at(1280, 800)
}

// ── Inspect ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inspecting_a_desktop_reports_the_methods_its_bridge_read() {
    // Given a desktop advertising VNC Authentication, Apple Remote Desktop and Tight (type 16)
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .whose_desktop_advertises(vec![
            AdvertisedMethod::VncPassword,
            AdvertisedMethod::AppleRemoteDesktop,
            AdvertisedMethod::UnsupportedVnc { security_type: 16 },
        ])
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    let inspected = daemon
        .inspect_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("a desktop whose bridge read its advertisement is inspected");

    // Then — the two tddy can sign in with are offered; the one it cannot is named by number
    assert_eq!(
        inspected,
        InspectHostDesktopResponse {
            offered: vec![
                DesktopAuthMethod::VncPassword as i32,
                DesktopAuthMethod::AppleRemoteDesktop as i32,
            ],
            unsupported_vnc_types: vec![16],
            remembered: false,
        }
    );
}

#[tokio::test]
async fn inspecting_a_desktop_tells_its_bridge_only_where_the_desktop_is() {
    // Given
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in()).start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    daemon
        .inspect_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop is inspected");

    // Then — an address, and nothing a sign-in would need
    assert_eq!(
        daemon.bridge.inspect_request(),
        serde_json::json!({ "host": "127.0.0.1", "port": A_VNC_PORT })
    );
}

#[tokio::test]
async fn inspecting_a_desktop_with_a_remembered_credential_says_so() {
    // Given a desktop the operator opened once with Remember ticked
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .with_the_operators_vault()
        .whose_operator_signs_in_with(
            a_credential()
                .with_password(THE_DESKTOP_PASSWORD)
                .remembered(),
        )
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");

    // When
    let inspected = daemon
        .inspect_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop is inspected");

    // Then
    assert!(
        inspected.remembered,
        "the inspection does not say a credential is remembered: {inspected:?}"
    );
}

// ── Asking ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn opening_a_desktop_asks_the_operator_for_its_credentials() {
    // Given
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .whose_operator_signs_in_with(a_credential().with_password(THE_DESKTOP_PASSWORD))
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");

    // Then — one question, for a structured credential rather than a bare password
    let kinds: Vec<PromptKind> = daemon
        .questions_asked()
        .iter()
        .map(|question| question.kind)
        .collect();
    assert_eq!(kinds, vec![PromptKind::DesktopCredentials]);
}

#[tokio::test]
async fn the_domain_the_operator_typed_reaches_the_bridge_on_stdin() {
    // Given an NLA desktop, and an operator signing in to it with a domain account
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .whose_desktop_advertises(vec![AdvertisedMethod::RdpNla])
        .whose_operator_signs_in_with(
            a_credential()
                .for_method(DesktopAuthMethod::RdpNla)
                .with_username(THE_OPERATORS_ACCOUNT)
                .with_domain(THE_WINDOWS_DOMAIN)
                .with_password(THE_DESKTOP_PASSWORD),
        )
        .start();
    let target_id = daemon.attach_an_rdp_desktop_to(A_HOST).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");

    // Then — the account, its domain and its password, on the bridge's stdin
    let config = daemon.bridge.nth_config_read_from_its_stdin(0).await;
    assert_eq!(
        (&config["username"], &config["domain"], &config["password"]),
        (
            &serde_json::json!(THE_OPERATORS_ACCOUNT),
            &serde_json::json!(THE_WINDOWS_DOMAIN),
            &serde_json::json!(THE_DESKTOP_PASSWORD)
        )
    );
}

// ── Remember ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_remembered_credential_opens_the_desktop_without_asking() {
    // Given a desktop the operator opened, and closed, with Remember ticked
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .with_the_operators_vault()
        .whose_operator_signs_in_with(
            a_credential()
                .with_password(THE_DESKTOP_PASSWORD)
                .remembered(),
        )
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");
    daemon.close_the_desktop_of(A_HOST, &target_id).await;

    // When it is opened again
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the remembered credential");

    // Then the operator was asked only the first time
    assert_eq!(daemon.questions_asked().len(), 1);
    // …and the second bridge signed in with what they remembered
    assert_eq!(
        daemon.bridge.nth_config_read_from_its_stdin(1).await["password"],
        serde_json::json!(THE_DESKTOP_PASSWORD)
    );
}

#[tokio::test]
async fn a_credential_given_with_remember_is_sealed_in_the_operators_vault() {
    // Given
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .with_the_operators_vault()
        .whose_operator_signs_in_with(
            a_credential()
                .with_password(THE_DESKTOP_PASSWORD)
                .remembered(),
        )
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");

    // Then — a screen-sharing record for this host's desktop, holding the password
    let record = daemon
        .the_operators_screen_sharing_record(&the_remembered_account(A_HOST, &target_id))
        .expect("the remembered credential is a record in the operator's vault");
    assert_eq!(record.secret.expose(), THE_DESKTOP_PASSWORD);
}

#[tokio::test]
async fn a_credential_given_without_remember_is_written_nowhere() {
    // Given an operator who signs in without ticking Remember
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .with_the_operators_vault()
        .whose_operator_signs_in_with(a_credential().with_password(THE_DESKTOP_PASSWORD))
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;

    // When
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");

    // Then the password was used — asserted first, so the absences below are not of a password
    // that never arrived
    assert_eq!(
        daemon.bridge.nth_config_read_from_its_stdin(0).await["password"],
        serde_json::json!(THE_DESKTOP_PASSWORD)
    );
    // …and the operator's vault holds no screen-sharing record
    assert_eq!(daemon.the_operators_screen_sharing_records(), vec![]);
    // …and no file the daemon wrote holds it
    assert_eq!(
        daemon.files_the_daemon_wrote_holding(THE_DESKTOP_PASSWORD),
        Vec::<std::path::PathBuf>::new()
    );
}

#[tokio::test]
async fn a_remembered_credential_the_desktop_refuses_is_not_used_again() {
    // Given a remembered credential…
    let daemon = a_daemon_whose_bridge(a_desktop_that_accepts_the_sign_in())
        .with_the_operators_vault()
        .whose_operator_signs_in_with(
            a_credential()
                .with_password(THE_DESKTOP_PASSWORD)
                .remembered(),
        )
        .start();
    let target_id = daemon.attach_a_desktop_to(A_HOST).await;
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed");
    daemon.close_the_desktop_of(A_HOST, &target_id).await;
    // …that the desktop has since stopped accepting
    daemon
        .bridge
        .now_does(&TheBridge::refusing_the_sign_in_with(THE_DESKTOPS_REASON));
    let refusal = daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect_err("the desktop refuses the remembered credential");
    assert_eq!(refusal.code, Code::Unauthenticated, "{refusal:?}");

    // When the operator opens it again
    daemon
        .bridge
        .now_does(&a_desktop_that_accepts_the_sign_in());
    daemon
        .open_the_desktop_of(A_HOST, &target_id)
        .await
        .expect("the desktop opens with the credential the operator typed again");

    // Then they were asked again, rather than the refused credential being tried a second time
    assert_eq!(daemon.questions_asked().len(), 2);
}
