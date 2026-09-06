//! Acceptance tests for `daemon_config.DaemonConfigService`: the daemon's own settings, read and
//! written by its UI.
//!
//! See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::{DaemonConfig, LiveKitConfig};
use tddy_daemon::daemon_config_service::{CommonRoomSupervisor, DaemonConfigServiceImpl};
use tddy_rpc::{Code, Request};
use tddy_service::proto::daemon_config::{
    ClientAllowedAgent, DaemonConfigService as DaemonConfigServiceTrait, DaemonSettings,
    GetClientConfigRequest, GetConfigRequest, ListenSettings, LiveKitSettings, UpdateConfigRequest,
};
use tokio::sync::Mutex;

/// The only token the service under test accepts.
const VALID_TOKEN: &str = "valid-token";

const FIXTURE_LIVEKIT_URL: &str = "ws://127.0.0.1:7880";

/// The YAML the daemon under test was started with.
fn a_daemon_config_file() -> String {
    format!(
        r#"
listen:
  web_port: 8899
  web_host: 127.0.0.1
livekit:
  enabled: true
  url: {FIXTURE_LIVEKIT_URL}
  public_url: {FIXTURE_LIVEKIT_URL}
  api_key: devkey
  api_secret: the-secret
  common_room: tddy-lobby
allowed_agents:
  - id: stub
    label: "Stub"
"#
    )
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// A service serving the fixture configuration from a real file on disk, so an update can be
/// observed where it has to land.
struct ADaemonConfigService {
    service: DaemonConfigServiceImpl,
    config_path: PathBuf,
    common_room: Arc<RecordingCommonRoom>,
    _dir: tempfile::TempDir,
}

fn a_daemon_config_service() -> ADaemonConfigService {
    let dir = tempfile::tempdir().expect("no temp dir");
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, a_daemon_config_file()).expect("the config file was not written");
    let config = DaemonConfig::load(&config_path).expect("the config fixture did not load");
    let common_room = Arc::new(RecordingCommonRoom::default());

    ADaemonConfigService {
        service: DaemonConfigServiceImpl::new(
            Some(config_path.clone()),
            Arc::new(Mutex::new(config)),
            common_room.clone(),
            Arc::new(|token| token == VALID_TOKEN),
        ),
        config_path,
        common_room,
        _dir: dir,
    }
}

/// Records what the service asked the common-room connection to become.
#[derive(Default)]
struct RecordingCommonRoom {
    reconfigured: std::sync::Mutex<Vec<Option<LiveKitConfig>>>,
}

impl CommonRoomSupervisor for RecordingCommonRoom {
    fn reconfigure(&self, livekit: Option<LiveKitConfig>) {
        self.reconfigured
            .lock()
            .expect("recording supervisor poisoned")
            .push(livekit);
    }
}

impl RecordingCommonRoom {
    /// The LiveKit URL of each reconfiguration, in order.
    fn reconfigured_urls(&self) -> Vec<Option<String>> {
        self.reconfigured
            .lock()
            .expect("recording supervisor poisoned")
            .iter()
            .map(|livekit| livekit.as_ref().and_then(|lk| lk.url.clone()))
            .collect()
    }

    /// The state of the operator's switch in each reconfiguration, in order. Switching the common
    /// room off changes no URL, so the URLs alone cannot tell a disable from an untouched save.
    fn reconfigured_switch_states(&self) -> Vec<Option<bool>> {
        self.reconfigured
            .lock()
            .expect("recording supervisor poisoned")
            .iter()
            .map(|livekit| livekit.as_ref().map(|lk| lk.enabled))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Settings builder
// ---------------------------------------------------------------------------

/// The fixture's settings, as an update would carry them back. Override only the field the
/// scenario changes — an update replaces the whole message, so a partial one would read as a
/// request to clear everything it omits.
fn the_current_settings() -> SettingsBuilder {
    SettingsBuilder {
        livekit_enabled: true,
        livekit_url: FIXTURE_LIVEKIT_URL.to_string(),
        common_room: "tddy-lobby".to_string(),
        web_port: 8899,
    }
}

struct SettingsBuilder {
    livekit_enabled: bool,
    livekit_url: String,
    common_room: String,
    web_port: u32,
}

impl SettingsBuilder {
    fn with_livekit_url(mut self, url: &str) -> Self {
        self.livekit_url = url.to_string();
        self
    }

    /// The operator moving the toggle to off, changing nothing else.
    fn with_livekit_switched_off(mut self) -> Self {
        self.livekit_enabled = false;
        self
    }

    fn with_web_port(mut self, web_port: u32) -> Self {
        self.web_port = web_port;
        self
    }

    fn build(self) -> DaemonSettings {
        DaemonSettings {
            livekit: Some(LiveKitSettings {
                url: Some(self.livekit_url.clone()),
                public_url: Some(self.livekit_url),
                api_key: Some("devkey".to_string()),
                // An update that carries no secret leaves the stored one in place; these tests are
                // about the other fields.
                api_secret: None,
                common_room: Some(self.common_room),
                api_secret_set: false,
                enabled: self.livekit_enabled,
            }),
            listen: Some(ListenSettings {
                web_port: Some(self.web_port),
                web_host: Some("127.0.0.1".to_string()),
            }),
        }
    }
}

fn an_update_of(settings: DaemonSettings) -> Request<UpdateConfigRequest> {
    Request::new(UpdateConfigRequest {
        session_token: VALID_TOKEN.to_string(),
        settings: Some(settings),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn returns_the_effective_configuration_with_the_livekit_api_secret_redacted() {
    // Given a daemon whose configuration holds a LiveKit secret
    let daemon = a_daemon_config_service();

    // When its configuration is read
    let response = daemon
        .service
        .get_config(Request::new(GetConfigRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("the configuration was not served")
        .into_inner();

    // Then everything but the secret comes back, and the secret's presence is reported instead
    let livekit = response
        .settings
        .expect("no settings in the response")
        .livekit
        .expect("no livekit block in the response");
    assert_eq!(livekit.url.as_deref(), Some(FIXTURE_LIVEKIT_URL));
    assert_eq!(livekit.api_key.as_deref(), Some("devkey"));
    assert_eq!(livekit.api_secret, None);
    assert!(
        livekit.api_secret_set,
        "the stored secret was not reported as set"
    );
}

#[tokio::test]
async fn reports_the_path_of_the_file_an_update_will_be_written_to() {
    // Given a daemon started from a config file
    let daemon = a_daemon_config_service();

    // When its configuration is read
    let response = daemon
        .service
        .get_config(Request::new(GetConfigRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("the configuration was not served")
        .into_inner();

    // Then the response names that file
    assert_eq!(
        response.config_path,
        daemon.config_path.to_string_lossy().to_string()
    );
}

#[tokio::test]
async fn writes_an_edited_livekit_url_back_to_the_yaml_file_the_daemon_was_loaded_from() {
    // Given a daemon loaded from a config file
    let daemon = a_daemon_config_service();

    // When the LiveKit URL is changed
    let response = daemon
        .service
        .update_config(an_update_of(
            the_current_settings()
                .with_livekit_url("ws://10.0.0.5:7880")
                .build(),
        ))
        .await
        .expect("the update was refused")
        .into_inner();

    // Then that file names the new URL, and nothing was deferred to a restart
    let reloaded = DaemonConfig::load(&daemon.config_path).expect("the rewritten config not load");
    assert_eq!(
        reloaded
            .livekit
            .expect("the rewritten config has no livekit block")
            .url
            .as_deref(),
        Some("ws://10.0.0.5:7880")
    );
    assert_eq!(response.restart_required, Vec::<String>::new());
}

#[tokio::test]
async fn reconnects_the_common_room_when_the_livekit_url_changes() {
    // Given a daemon connected to the fixture's LiveKit server
    let daemon = a_daemon_config_service();

    // When the LiveKit URL is changed
    daemon
        .service
        .update_config(an_update_of(
            the_current_settings()
                .with_livekit_url("ws://10.0.0.5:7880")
                .build(),
        ))
        .await
        .expect("the update was refused");

    // Then the running connection was told to become the new one
    assert_eq!(
        daemon.common_room.reconfigured_urls(),
        vec![Some("ws://10.0.0.5:7880".to_string())]
    );
}

#[tokio::test]
async fn keeps_the_common_room_connected_when_an_unrelated_field_changes() {
    // Given a daemon connected to the fixture's LiveKit server
    let daemon = a_daemon_config_service();

    // When a field outside the LiveKit block is changed
    daemon
        .service
        .update_config(an_update_of(
            the_current_settings().with_web_port(9911).build(),
        ))
        .await
        .expect("the update was refused");

    // Then the connection was left alone
    assert_eq!(daemon.common_room.reconfigured_urls(), Vec::new());
}

#[tokio::test]
async fn persists_a_changed_web_port_and_reports_it_as_restart_required() {
    // Given a daemon whose web port is 8899
    let daemon = a_daemon_config_service();

    // When the web port is changed
    let response = daemon
        .service
        .update_config(an_update_of(
            the_current_settings().with_web_port(9911).build(),
        ))
        .await
        .expect("the update was refused")
        .into_inner();

    // Then the new port is persisted, and named as one that needs a restart
    let reloaded = DaemonConfig::load(&daemon.config_path).expect("the rewritten config not load");
    assert_eq!(reloaded.listen.web_port, Some(9911));
    assert_eq!(
        response.restart_required,
        vec!["listen.web_port".to_string()]
    );
}

#[tokio::test]
async fn refuses_a_livekit_url_that_is_not_a_websocket_url_and_leaves_the_file_unchanged() {
    // Given a daemon loaded from a config file
    let daemon = a_daemon_config_service();
    let before = std::fs::read_to_string(&daemon.config_path).expect("the config file not read");

    // When an update carries a LiveKit URL that is not a websocket URL
    let status = daemon
        .service
        .update_config(an_update_of(
            the_current_settings()
                .with_livekit_url("http://10.0.0.5:7880")
                .build(),
        ))
        .await
        .expect_err("the update was accepted");

    // Then it is refused and nothing was written
    assert_eq!(status.code(), Code::InvalidArgument);
    assert_eq!(
        std::fs::read_to_string(&daemon.config_path).expect("the config file not read"),
        before
    );
}

#[tokio::test]
async fn returns_the_client_config_the_web_bundle_otherwise_fetches_over_http() {
    // Given a daemon serving a web bundle with no HTTP origin to fetch config from
    let daemon = a_daemon_config_service();

    // When the client config is requested over RPC
    let response = daemon
        .service
        .get_client_config(Request::new(GetClientConfigRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("the client config was not served")
        .into_inner();

    // Then it carries what /api/config carries
    assert_eq!(response.livekit_url.as_deref(), Some(FIXTURE_LIVEKIT_URL));
    assert_eq!(response.common_room.as_deref(), Some("tddy-lobby"));
    assert_eq!(response.daemon_mode, Some(true));
    assert_eq!(
        response.allowed_agents,
        vec![ClientAllowedAgent {
            id: "stub".to_string(),
            label: "Stub".to_string(),
        }]
    );
}

#[tokio::test]
async fn serves_the_client_config_to_a_caller_that_has_not_signed_in_yet() {
    // Given a daemon a page has just been loaded from, before any sign-in
    let daemon = a_daemon_config_service();

    // When that page asks for the config that tells it there is a daemon to sign in to
    let response = daemon
        .service
        .get_client_config(Request::new(GetClientConfigRequest {
            session_token: String::new(),
        }))
        .await
        .expect("the client config was refused to a caller that cannot have a token yet")
        .into_inner();

    // Then it is served — the same snapshot `GET /api/config` serves unauthenticated
    assert_eq!(response.livekit_url.as_deref(), Some(FIXTURE_LIVEKIT_URL));
    assert_eq!(response.daemon_mode, Some(true));
}

#[tokio::test]
async fn refuses_to_return_the_configuration_to_a_caller_without_a_valid_session_token() {
    // Given a daemon whose configuration holds LiveKit credentials
    let daemon = a_daemon_config_service();

    // When a caller presents a token the daemon does not accept
    let status = daemon
        .service
        .get_config(Request::new(GetConfigRequest {
            session_token: "not-a-token".to_string(),
        }))
        .await
        .expect_err("the configuration was served to an unauthenticated caller");

    // Then the call is refused
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn refuses_to_write_the_configuration_for_a_caller_without_a_valid_session_token() {
    // Given a daemon loaded from a config file
    let daemon = a_daemon_config_service();
    let before = std::fs::read_to_string(&daemon.config_path).expect("the config file not read");

    // When a caller presents a token the daemon does not accept
    let status = daemon
        .service
        .update_config(Request::new(UpdateConfigRequest {
            session_token: "not-a-token".to_string(),
            settings: Some(the_current_settings().with_web_port(9911).build()),
        }))
        .await
        .expect_err("the configuration was written for an unauthenticated caller");

    // Then the call is refused and nothing was written
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(
        std::fs::read_to_string(&daemon.config_path).expect("the config file not read"),
        before
    );
}

// ---------------------------------------------------------------------------
// The operator's switch, end to end: the toggle saved to the file the daemon was loaded from, the
// live connection told about it, and the page told not to build a room of its own.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn writes_a_switched_off_common_room_back_to_the_yaml_file_the_daemon_was_loaded_from() {
    // Given a daemon joining its common room
    let daemon = a_daemon_config_service();

    // When the operator switches it off
    daemon
        .service
        .update_config(an_update_of(
            the_current_settings().with_livekit_switched_off().build(),
        ))
        .await
        .expect("the update was refused");

    // Then the file says so
    let reloaded = DaemonConfig::load(&daemon.config_path).expect("the rewritten config not load");
    assert!(
        !reloaded
            .livekit
            .expect("the rewritten config has no livekit block")
            .enabled,
        "the toggle was saved but the file still says enabled"
    );
}

#[tokio::test]
async fn keeps_the_livekit_credentials_in_the_file_when_the_common_room_is_switched_off() {
    // Given a daemon joining its common room
    let daemon = a_daemon_config_service();

    // When the operator switches it off
    daemon
        .service
        .update_config(an_update_of(
            the_current_settings().with_livekit_switched_off().build(),
        ))
        .await
        .expect("the update was refused");

    // Then the url, key, secret and room are still in the file. "Off" that costs the operator their
    // credentials is the thing this switch exists to replace.
    let reloaded = DaemonConfig::load(&daemon.config_path).expect("the rewritten config not load");
    let livekit = reloaded
        .livekit
        .expect("the rewritten config has no livekit block");
    assert_eq!(
        (
            livekit.url.as_deref(),
            livekit.api_key.as_deref(),
            livekit.api_secret.as_deref(),
            livekit.common_room.as_deref()
        ),
        (
            Some(FIXTURE_LIVEKIT_URL),
            Some("devkey"),
            Some("the-secret"),
            Some("tddy-lobby")
        )
    );
}

#[tokio::test]
async fn disconnects_the_common_room_when_the_operator_switches_it_off() {
    // Given a daemon connected to the fixture's LiveKit server
    let daemon = a_daemon_config_service();

    // When the operator switches the common room off, changing nothing else
    daemon
        .service
        .update_config(an_update_of(
            the_current_settings().with_livekit_switched_off().build(),
        ))
        .await
        .expect("the update was refused");

    // Then the running connection was told to become a switched-off one. Saving the toggle has to
    // reach the live room; the URL and room name are unchanged, so nothing else here would.
    assert_eq!(
        daemon.common_room.reconfigured_switch_states(),
        vec![Some(false)]
    );
}

#[tokio::test]
async fn reports_the_common_room_as_switched_off_to_the_page_the_daemon_serves() {
    // Given a daemon whose common room the operator has switched off
    let daemon = a_daemon_config_service();
    daemon
        .service
        .update_config(an_update_of(
            the_current_settings().with_livekit_switched_off().build(),
        ))
        .await
        .expect("the update was refused");

    // When a page asks for the configuration it starts up with
    let response = daemon
        .service
        .get_client_config(Request::new(GetClientConfigRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("the client config was not served")
        .into_inner();

    // Then it is told the common room is off, so it builds no `Room` and mints no token — the same
    // quiet outcome an unconfigured deployment already has, reached deliberately
    assert_eq!(response.livekit_enabled, Some(false));
}

#[tokio::test]
async fn reports_the_common_room_as_switched_on_to_the_page_the_daemon_serves() {
    // Given a daemon joining its common room
    let daemon = a_daemon_config_service();

    // When a page asks for the configuration it starts up with
    let response = daemon
        .service
        .get_client_config(Request::new(GetClientConfigRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("the client config was not served")
        .into_inner();

    // Then it is cleared to join
    assert_eq!(response.livekit_enabled, Some(true));
}
