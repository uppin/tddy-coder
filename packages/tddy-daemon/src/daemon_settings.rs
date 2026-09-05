//! What the daemon's settings mean, as pure functions.
//!
//! Redaction, validation, merging an update into the current configuration, and deciding what an
//! accepted update implies — which fields cannot be applied to a running process, and whether the
//! common-room connection has to be rebuilt. Kept separate from
//! [`crate::daemon_config_service`] so the rules are testable without a service, a token, or a
//! file: the service authenticates, calls in here, persists, and applies.

use tddy_rpc::Status;
use tddy_service::proto::daemon_config::{DaemonSettings, ListenSettings, LiveKitSettings};

use crate::config::{DaemonConfig, LiveKitConfig};

/// The only URL schemes a LiveKit client can connect with.
const LIVEKIT_URL_SCHEMES: [&str; 2] = ["ws://", "wss://"];

/// What accepting an update means for the daemon.
pub struct AppliedUpdate {
    /// The configuration to persist and adopt.
    pub config: DaemonConfig,
    /// Field paths (e.g. `listen.web_port`) that were accepted but cannot take effect until the
    /// daemon restarts. Named rather than silently ignored.
    pub restart_required: Vec<String>,
    /// True when the common-room connection must be torn down and rebuilt.
    pub reconnect_common_room: bool,
}

/// `config` as the UI may see it: an API secret is reported as *set* rather than returned.
pub fn redacted_settings(config: &DaemonConfig) -> DaemonSettings {
    DaemonSettings {
        livekit: config.livekit.as_ref().map(|livekit| LiveKitSettings {
            url: livekit.url.clone(),
            public_url: livekit.public_url.clone(),
            api_key: livekit.api_key.clone(),
            api_secret: None,
            common_room: livekit.common_room.clone(),
            api_secret_set: livekit.api_secret.is_some(),
        }),
        listen: Some(ListenSettings {
            web_port: config.listen.web_port.map(u32::from),
            web_host: config.listen.web_host.clone(),
        }),
    }
}

/// Validate `settings` against `current` and work out what accepting them means.
///
/// An update carries the complete settings message, so an omitted secret means *leave the stored
/// one alone* — the UI never held it to send back.
pub fn apply_update(
    current: &DaemonConfig,
    settings: &DaemonSettings,
) -> Result<AppliedUpdate, Status> {
    let mut config = current.clone();

    config.livekit = match &settings.livekit {
        Some(livekit) => Some(merged_livekit(current.livekit.as_ref(), livekit)?),
        None => None,
    };
    if let Some(listen) = &settings.listen {
        config.listen.web_port = match listen.web_port {
            Some(port) => Some(web_port(port)?),
            None => None,
        };
        config.listen.web_host = listen.web_host.clone();
    }

    Ok(AppliedUpdate {
        reconnect_common_room: common_room_changed(&current.livekit, &config.livekit),
        restart_required: restart_required(current, &config),
        config,
    })
}

/// `settings` merged onto `stored`, so an update speaks only for the fields the UI shows: the
/// LiveKit timeout it never renders, and a secret it never held, survive it.
fn merged_livekit(
    stored: Option<&LiveKitConfig>,
    settings: &LiveKitSettings,
) -> Result<LiveKitConfig, Status> {
    let mut livekit = stored.cloned().unwrap_or_default();
    livekit.url = Some(livekit_url(settings.url.as_deref())?);
    livekit.public_url = settings.public_url.clone();
    livekit.api_key = settings.api_key.clone();
    livekit.common_room = settings.common_room.clone();
    if settings.api_secret.is_some() {
        livekit.api_secret = settings.api_secret.clone();
    }
    Ok(livekit)
}

/// A LiveKit URL a client can actually connect with. Anything else is refused rather than stored:
/// an `http://` URL would leave the daemon permanently unable to reach its own server.
fn livekit_url(url: Option<&str>) -> Result<String, Status> {
    let url = url.unwrap_or_default();
    if LIVEKIT_URL_SCHEMES
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        return Ok(url.to_string());
    }
    Err(Status::invalid_argument(format!(
        "livekit.url must start with ws:// or wss://, was '{url}'"
    )))
}

/// The listening port, which the daemon binds as a `u16`. A number outside that range is refused
/// rather than truncated into a port the operator never asked for.
fn web_port(port: u32) -> Result<u16, Status> {
    u16::try_from(port).map_err(|_| {
        Status::invalid_argument(format!(
            "listen.web_port must be a port number below 65536, was {port}"
        ))
    })
}

/// Whether the common-room connection has to be rebuilt: it is defined by the server it is made to
/// and the room it joins, so a change to either invalidates the live one.
fn common_room_changed(before: &Option<LiveKitConfig>, after: &Option<LiveKitConfig>) -> bool {
    let identity = |livekit: &Option<LiveKitConfig>| {
        livekit
            .as_ref()
            .map(|lk| (lk.url.clone(), lk.common_room.clone()))
    };
    identity(before) != identity(after)
}

/// The accepted fields a running daemon cannot apply to itself, named so the response can say so
/// rather than leaving an operator to wonder why nothing happened.
fn restart_required(before: &DaemonConfig, after: &DaemonConfig) -> Vec<String> {
    let mut fields = Vec::new();
    if before.listen.web_port != after.listen.web_port {
        fields.push("listen.web_port".to_string());
    }
    if before.listen.web_host != after.listen.web_host {
        fields.push("listen.web_host".to_string());
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use tddy_rpc::Code;
    use tddy_service::proto::daemon_config::{ListenSettings, LiveKitSettings};

    const CONFIGURED_URL: &str = "ws://127.0.0.1:7880";

    /// A daemon configured with a LiveKit block holding a secret, and a web port.
    fn a_configured_daemon() -> DaemonConfig {
        serde_yaml::from_str(
            r#"
listen:
  web_port: 8899
  web_host: 127.0.0.1
livekit:
  url: ws://127.0.0.1:7880
  public_url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: the-secret
  common_room: tddy-lobby
"#,
        )
        .expect("the daemon fixture did not parse")
    }

    /// A daemon with no `livekit:` block at all — the state that leaves it disconnected.
    fn a_daemon_without_livekit() -> DaemonConfig {
        serde_yaml::from_str(
            r#"
listen:
  web_port: 8899
"#,
        )
        .expect("the daemon fixture did not parse")
    }

    /// The configured daemon's own settings, as an update would carry them back. Override only the
    /// field under test.
    fn the_current_settings() -> SettingsBuilder {
        SettingsBuilder {
            livekit_url: CONFIGURED_URL.to_string(),
            api_secret: None,
            common_room: "tddy-lobby".to_string(),
            web_port: 8899,
            web_host: "127.0.0.1".to_string(),
        }
    }

    struct SettingsBuilder {
        livekit_url: String,
        api_secret: Option<String>,
        common_room: String,
        web_port: u32,
        web_host: String,
    }

    impl SettingsBuilder {
        fn with_livekit_url(mut self, url: &str) -> Self {
            self.livekit_url = url.to_string();
            self
        }

        fn with_api_secret(mut self, secret: &str) -> Self {
            self.api_secret = Some(secret.to_string());
            self
        }

        fn with_common_room(mut self, common_room: &str) -> Self {
            self.common_room = common_room.to_string();
            self
        }

        fn with_web_port(mut self, web_port: u32) -> Self {
            self.web_port = web_port;
            self
        }

        fn with_web_host(mut self, web_host: &str) -> Self {
            self.web_host = web_host.to_string();
            self
        }

        fn build(self) -> DaemonSettings {
            DaemonSettings {
                livekit: Some(LiveKitSettings {
                    url: Some(self.livekit_url.clone()),
                    public_url: Some(self.livekit_url),
                    api_key: Some("devkey".to_string()),
                    api_secret: self.api_secret,
                    common_room: Some(self.common_room),
                    api_secret_set: false,
                }),
                listen: Some(ListenSettings {
                    web_port: Some(self.web_port),
                    web_host: Some(self.web_host),
                }),
            }
        }
    }

    /// Assert the update was refused as invalid, and say what the message must mention.
    fn assert_invalid(result: Result<AppliedUpdate, Status>, fragment: &str) {
        let status = match result {
            Err(status) => status,
            Ok(_) => panic!("expected the update to be refused, but it was accepted"),
        };
        assert_eq!(
            status.code(),
            Code::InvalidArgument,
            "refusal code mismatch"
        );
        assert!(
            status.message().contains(fragment),
            "expected the refusal to mention '{fragment}', was '{}'",
            status.message()
        );
    }

    fn accepted(result: Result<AppliedUpdate, Status>) -> AppliedUpdate {
        result.expect("the update was refused")
    }

    // -----------------------------------------------------------------------
    // Redaction
    // -----------------------------------------------------------------------

    #[test]
    fn reports_a_stored_livekit_secret_as_set_without_returning_it() {
        // Given a daemon holding a LiveKit API secret
        let config = a_configured_daemon();

        // When its settings are projected for the UI
        let settings = redacted_settings(&config);

        // Then the secret's presence is reported and its value is not
        let livekit = settings.livekit.expect("no livekit block");
        assert_eq!(livekit.api_secret, None);
        assert!(livekit.api_secret_set, "a stored secret was not reported");
        assert_eq!(livekit.api_key.as_deref(), Some("devkey"));
        assert_eq!(livekit.url.as_deref(), Some(CONFIGURED_URL));
    }

    #[test]
    fn reports_no_livekit_settings_when_the_daemon_has_no_livekit_block() {
        // Given a daemon configured without LiveKit
        let config = a_daemon_without_livekit();

        // When its settings are projected for the UI
        let settings = redacted_settings(&config);

        // Then the absent block stays absent rather than becoming an empty one
        assert!(
            settings.livekit.is_none(),
            "an absent livekit block was reported as present"
        );
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::ws("ws://10.0.0.5:7880")]
    #[case::wss("wss://livekit.example.com")]
    fn accepts_a_websocket_livekit_url(#[case] url: &str) {
        // Given an update carrying a websocket LiveKit URL
        let settings = the_current_settings().with_livekit_url(url).build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the URL is adopted
        assert_eq!(
            update
                .config
                .livekit
                .expect("no livekit block")
                .url
                .as_deref(),
            Some(url)
        );
    }

    #[rstest]
    #[case::http("http://10.0.0.5:7880")]
    #[case::https("https://livekit.example.com")]
    #[case::empty("")]
    #[case::not_a_url("livekit.example.com")]
    fn refuses_a_livekit_url_that_is_not_a_websocket_url(#[case] url: &str) {
        // Given an update carrying a LiveKit URL that is not a websocket URL
        let settings = the_current_settings().with_livekit_url(url).build();

        // When it is applied
        let result = apply_update(&a_configured_daemon(), &settings);

        // Then it is refused, naming the field
        assert_invalid(result, "livekit.url");
    }

    // -----------------------------------------------------------------------
    // Secrets
    // -----------------------------------------------------------------------

    #[test]
    fn keeps_the_stored_api_secret_when_an_update_omits_it() {
        // Given an update that carries no API secret, because the UI never held one
        let settings = the_current_settings().build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the stored secret survives
        assert_eq!(
            update
                .config
                .livekit
                .expect("no livekit block")
                .api_secret
                .as_deref(),
            Some("the-secret")
        );
    }

    #[test]
    fn replaces_the_stored_api_secret_when_an_update_carries_a_new_one() {
        // Given an update carrying a newly typed API secret
        let settings = the_current_settings().with_api_secret("rotated").build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the new secret replaces the stored one
        assert_eq!(
            update
                .config
                .livekit
                .expect("no livekit block")
                .api_secret
                .as_deref(),
            Some("rotated")
        );
    }

    // -----------------------------------------------------------------------
    // Reconnecting the common room
    // -----------------------------------------------------------------------

    #[test]
    fn asks_for_a_common_room_reconnect_when_the_livekit_url_changes() {
        // Given an update pointing LiveKit at another server
        let settings = the_current_settings()
            .with_livekit_url("ws://10.0.0.5:7880")
            .build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the running connection has to be rebuilt
        assert!(
            update.reconnect_common_room,
            "a changed LiveKit URL did not ask for a reconnect"
        );
    }

    #[test]
    fn asks_for_a_common_room_reconnect_when_the_room_name_changes() {
        // Given an update naming another common room
        let settings = the_current_settings()
            .with_common_room("tddy-other")
            .build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the running connection has to be rebuilt
        assert!(
            update.reconnect_common_room,
            "a changed common room did not ask for a reconnect"
        );
    }

    #[test]
    fn leaves_the_common_room_alone_when_the_livekit_block_is_unchanged() {
        // Given an update that changes nothing about LiveKit
        let settings = the_current_settings().with_web_port(9911).build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the connection is not disturbed
        assert!(
            !update.reconnect_common_room,
            "an unrelated change asked for a reconnect"
        );
    }

    // -----------------------------------------------------------------------
    // What cannot apply to a running daemon
    // -----------------------------------------------------------------------

    #[test]
    fn names_the_web_port_as_restart_required_when_it_changes() {
        // Given an update changing the port the daemon is already listening on
        let settings = the_current_settings().with_web_port(9911).build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the port is persisted but named as waiting on a restart
        assert_eq!(update.config.listen.web_port, Some(9911));
        assert_eq!(update.restart_required, vec!["listen.web_port".to_string()]);
    }

    #[test]
    fn names_the_web_host_as_restart_required_when_it_changes() {
        // Given an update changing the interface the daemon is bound to
        let settings = the_current_settings().with_web_host("0.0.0.0").build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then the host is persisted but named as waiting on a restart
        assert_eq!(update.restart_required, vec!["listen.web_host".to_string()]);
    }

    #[test]
    fn names_nothing_as_restart_required_when_only_livekit_changes() {
        // Given an update touching only fields the daemon can apply to itself
        let settings = the_current_settings()
            .with_livekit_url("ws://10.0.0.5:7880")
            .build();

        // When it is applied
        let update = accepted(apply_update(&a_configured_daemon(), &settings));

        // Then nothing is deferred
        assert_eq!(update.restart_required, Vec::<String>::new());
    }
}
