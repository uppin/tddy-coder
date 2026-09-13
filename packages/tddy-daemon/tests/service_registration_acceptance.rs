//! What the assembled daemon actually registers, after `#unbundle` node 2 moved three subsystems
//! out of `tddy-daemon` and deleted a fourth.
//!
//! The roster is the boundary these moves must not change: `models.ModelRegistryService`,
//! `tddy.acp.v1.AcpService` and `screen_sharing.ScreenSharingService` are now built by
//! `tddy-model-registry` and `tddy-screen-sharing` constructors rather than assembled inline in
//! `runtime.rs`, so their entries have to survive the relocation unchanged. `vnc.VncService` is
//! the inverse: the deletion of the unreachable VNC service must not silently regress into a
//! re-registration.
//!
//! Complements `tddy-screen-sharing/tests/dead_vnc_service_removed.rs`, which asserts the *source*
//! is gone. This suite asserts against the running registry instead — the only place a
//! re-registration would show up.

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tempfile::TempDir;
use tokio::net::TcpListener;

/// A port that is free at this moment, so nothing in the fixture competes for a fixed one.
async fn a_free_tcp_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available");
    listener
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

/// A daemon whose GitHub auth resolves a session token to a user.
///
/// That is what gates the roster under test: every token-authenticated service — the three moved
/// ones included — is registered inside `runtime::build`'s `if let Some(user_resolver)` block, so a
/// config without GitHub auth would register none of them and make every assertion here vacuous.
/// `github.stub` is the daemon's own supported way to have that resolver without a real OAuth app.
///
/// `tddy_data_dir` points into a temp directory so assembling the runtime — which initialises the
/// host registry, the VM library and the model-registry store — writes nothing into the developer's
/// tddy home.
fn a_daemon_config_with_github_auth(web_port: u16, tddy_data_dir: &TempDir) -> DaemonConfig {
    let yaml = format!(
        r#"
listen:
  web_port: {web_port}
  web_host: 127.0.0.1
tddy_data_dir: {data_dir}
github:
  stub: true
"#,
        data_dir = tddy_data_dir.path().display()
    );
    serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
}

/// The service names an assembled daemon hosts, in registration order.
///
/// Built for an embedding host so assembling binds no port and starts no task — the roster is
/// available from `build` alone.
async fn the_services_an_assembled_daemon_registers() -> Vec<String> {
    let tddy_data_dir = tempfile::tempdir().expect("a temp tddy home");
    let port = a_free_tcp_port().await;

    let runtime = runtime::build(
        a_daemon_config_with_github_auth(port, &tddy_data_dir),
        RuntimeOptions::for_embedded(),
    )
    .await
    .expect("the daemon runtime did not build");

    runtime
        .service_names()
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn registers_the_model_registry_service_that_tddy_model_registry_now_owns() {
    // Given / When
    let services = the_services_an_assembled_daemon_registers().await;

    // Then
    assert!(
        services.iter().any(|name| name == "models.ModelRegistryService"),
        "the daemon must still register models.ModelRegistryService after the move; it hosts {services:?}"
    );
}

#[tokio::test]
async fn registers_the_model_addressed_acp_service_that_tddy_model_registry_now_owns() {
    // Given / When
    let services = the_services_an_assembled_daemon_registers().await;

    // Then — the daemon's own, model-addressed ACP surface, so the Models & Agents screen can open
    // a chat with no session in existence. The session-addressed one is mounted per session
    // process and is not part of this roster.
    assert!(
        services.iter().any(|name| name == "tddy.acp.v1.AcpService"),
        "the daemon must still register tddy.acp.v1.AcpService after the move; it hosts {services:?}"
    );
}

#[tokio::test]
async fn registers_the_screen_sharing_service_that_tddy_screen_sharing_now_owns() {
    // Given / When
    let services = the_services_an_assembled_daemon_registers().await;

    // Then
    assert!(
        services
            .iter()
            .any(|name| name == "screen_sharing.ScreenSharingService"),
        "the daemon must still register screen_sharing.ScreenSharingService after the move; it hosts {services:?}"
    );
}

/// `#unbundle` node 6 gave the thirteen session-file methods their own coordinate, served by
/// `tddy-session-files`. The crate can build the entry on its own, but only the daemon's wiring
/// layer can *mount* it — and until it did, the service existed and nothing reached it.
#[tokio::test]
async fn registers_the_session_files_service_that_tddy_session_files_now_owns() {
    // Given / When
    let services = the_services_an_assembled_daemon_registers().await;

    // Then
    assert!(
        services
            .iter()
            .any(|name| name == "session_files.SessionFilesService"),
        "the daemon must register session_files.SessionFilesService, or nothing serves the \
         thirteen session-file methods; it hosts {services:?}"
    );
}

/// The same node gave the nine terminal methods their own coordinate, served by
/// `tddy-terminal-rpc`, and removed them from `the pre-unbundle monolithic RPC coordinate` — so this roster is
/// the only place a LiveKit- or HTTP-reached terminal is answered from. Dropping the one line that
/// pushes it would leave every such terminal unserved: the UDS builder takes its terminal adapter
/// as a separate argument, so `local_token_uds.rs` would still pass.
#[tokio::test]
async fn registers_the_terminal_session_service_that_tddy_terminal_rpc_now_owns() {
    // Given / When
    let services = the_services_an_assembled_daemon_registers().await;

    // Then
    assert!(
        services
            .iter()
            .any(|name| name == "terminal_session.TerminalSessionService"),
        "the daemon must register terminal_session.TerminalSessionService, or nothing serves the \
         nine terminal methods; it hosts {services:?}"
    );
}

#[tokio::test]
async fn no_longer_registers_the_deleted_vnc_service() {
    // Given / When
    let services = the_services_an_assembled_daemon_registers().await;

    // Then the roster is the auth-gated one, or an absence below would prove nothing
    assert!(
        services
            .iter()
            .any(|name| name == "screen_sharing.ScreenSharingService"),
        "the auth-gated services did not register, so this test could not observe a VNC service either way; the daemon hosts {services:?}"
    );

    // Then nothing serves the deleted VNC service
    assert!(
        !services.iter().any(|name| name == "vnc.VncService"),
        "vnc.VncService is deleted and must not be registered again; the daemon hosts {services:?}"
    );
}
