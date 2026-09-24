//! Static file HTTP server for serving the tddy-web bundle.
//!
//! Used when --web-port and --web-bundle-path are both provided.

use std::path::PathBuf;

use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::{ServeDir, ServeFile};

/// One backend row for [`ClientConfig::allowed_agents`] (daemon `allowed_agents` YAML).
#[derive(Clone, serde::Serialize)]
pub struct ClientAllowedAgent {
    pub id: String,
    pub label: String,
}

/// What the serving daemon's `--workspace-tools` jail confines, as [`ClientConfig`] reports it.
///
/// The same wire shape the common-room advertisement publishes
/// (`tddy_daemon_livekit::livekit_peer_discovery::SandboxedCodebaseSupport`), deliberately: the web
/// reads one key, `sandboxed_codebase: { confines_filesystem }`, whichever source described the
/// host. This is a separate type only because `tddy-coder` does not depend on the daemon's LiveKit
/// crate, and a web server that served a page would not be able to name it if it did not.
#[derive(Clone, Copy, serde::Serialize)]
pub struct ClientSandboxedCodebaseSupport {
    /// Whether the jail confines filesystem writes outside the checkout. **True on macOS**, where
    /// Seatbelt denies paths outside the trees the jail holds; **false on Linux**, where the
    /// cgroups jail shares the host filesystem root. The web turns this into the caveat it shows
    /// beside the control, so getting it backwards promises confinement the kernel does not give.
    pub confines_filesystem: bool,
}

/// Client-visible server config, served at /api/config.
#[derive(Clone, serde::Serialize)]
pub struct ClientConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_room: Option<String>,
    /// Shared presence room (daemon `livekit.common_room`). Browser joins for participant list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_room: Option<String>,
    /// When true, server is tddy-daemon; show ConnectionScreen instead of ConnectionForm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_mode: Option<bool>,
    /// Daemon: same allowlist as `ListAgents` / `allowed_agents` in YAML (for UI before RPC hydrates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agents: Vec<ClientAllowedAgent>,
    /// Browser DEBUG mask (`debug`-package namespaces, e.g. `tddy:term:*`). From daemon `debug:` YAML;
    /// the web app enables scoped `[tddy]` diagnostics from this. None = off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<String>,
    /// The serving daemon's own instance id (`livekit_peer_discovery::local_instance_id_for_config`).
    /// Every daemon's own LiveKit advertisement self-labels `"{id} (this daemon)"` from its own
    /// perspective, so the web cannot otherwise tell which common-room daemon actually served this
    /// bundle; the daemon selector uses this to default its selection and to show "(this daemon)"
    /// next to the correct entry. `None` for the standalone (non-daemon) tddy-coder web server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_instance_id: Option<String>,
    /// Whether the serving daemon joins its common room (`livekit.enabled`). `false` means the
    /// operator switched LiveKit off, so the page constructs no `Room` and mints no token. `None`
    /// for the standalone (non-daemon) tddy-coder web server, which has no such switch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_enabled: Option<bool>,
    /// What this daemon's `--workspace-tools` jail confines, so the page can offer the **sandboxed
    /// codebase** placement — and say what it does not guarantee.
    ///
    /// The common room advertises the same capability, but a daemon with no common room advertises
    /// nothing, and that is the deployment the placement was designed for. This is the serving
    /// daemon's own self-description, so the page can read the capability of the host that handed
    /// it to it with no LiveKit configured at all.
    ///
    /// `None` is a host that does not serve the placement: a daemon that predates this key, an OS
    /// with no sandbox backend, or the standalone (non-daemon) tddy-coder web server. The key is
    /// left off the wire entirely then, so a reader cannot mistake absent for "present and confines
    /// nothing" — the control is disabled with the reason rather than offered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandboxed_codebase: Option<ClientSandboxedCodebaseSupport>,
    /// Which GitHub sign-in flow the serving daemon's `auth.AuthService` serves: `"redirect"` or
    /// `"device"` — the same values `GetClientConfig` carries in `auth_flow`. `None` is a host that
    /// serves no GitHub sign-in (a daemon without `github:`), and the key is then left off the
    /// wire; the dashboard reads that absence as "no sign-in configured", never as either flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_flow: Option<String>,
}

/// Serve static files from `bundle_path` on the given `host` and `port`.
/// When `rpc_router` is provided, it is merged before the static file fallback (e.g. ConnectRPC at /rpc).
/// When `client_config` is provided, it is served at GET /api/config.
/// Unmatched routes fall back to index.html for SPA client-side routing.
pub async fn serve_web_bundle(
    host: impl AsRef<str>,
    port: u16,
    bundle_path: PathBuf,
    rpc_router: Option<Router>,
    client_config: Option<ClientConfig>,
) -> anyhow::Result<()> {
    serve_web_bundle_with_shutdown(
        host,
        port,
        bundle_path,
        rpc_router,
        client_config,
        std::future::pending(),
    )
    .await
}

/// Same as [`serve_web_bundle`], but stops the HTTP server when `shutdown` completes (graceful shutdown).
pub async fn serve_web_bundle_with_shutdown<F>(
    host: impl AsRef<str>,
    port: u16,
    bundle_path: PathBuf,
    rpc_router: Option<Router>,
    client_config: Option<ClientConfig>,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let host = host.as_ref();
    let index_path = bundle_path.join("index.html");
    let service = ServeDir::new(&bundle_path)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&index_path));
    let mut app = Router::new();
    if let Some(config) = client_config {
        app = app.route(
            "/api/config",
            get(move || {
                let config = config.clone();
                async move { Json(config) }
            }),
        );
    }
    if let Some(rpc) = rpc_router {
        app = app.merge(rpc);
    }
    app = app.fallback_service(service);
    let addr = (host, port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind web server {}:{}: {}", host, port, e))?;
    log::info!(
        "Web server serving {} on {}:{}",
        bundle_path.display(),
        host,
        port
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("web server error: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> ClientConfig {
        ClientConfig {
            livekit_url: None,
            livekit_room: None,
            common_room: None,
            daemon_mode: None,
            allowed_agents: vec![],
            debug: None,
            daemon_instance_id: None,
            livekit_enabled: None,
            sandboxed_codebase: None,
            auth_flow: None,
        }
    }

    #[test]
    fn debug_mask_is_omitted_from_api_config_when_none() {
        let json = serde_json::to_value(empty_config()).expect("serialize");
        assert!(
            json.get("debug").is_none(),
            "debug must be skipped when None: {json}"
        );
    }

    #[test]
    fn debug_mask_is_serialized_when_set() {
        let cfg = ClientConfig {
            debug: Some("tddy:term:*".to_string()),
            ..empty_config()
        };
        let json = serde_json::to_value(cfg).expect("serialize");
        assert_eq!(
            json.get("debug").and_then(|v| v.as_str()),
            Some("tddy:term:*")
        );
    }

    /// The serving daemon's own instance id — needed so the web can default the daemon selector
    /// to the daemon that actually served this bundle (every daemon's advertisement self-labels
    /// "(this daemon)" from its own perspective, so the web cannot infer this any other way).
    #[test]
    fn daemon_instance_id_is_omitted_from_api_config_when_none() {
        let json = serde_json::to_value(empty_config()).expect("serialize");
        assert!(
            json.get("daemon_instance_id").is_none(),
            "daemon_instance_id must be skipped when None: {json}"
        );
    }

    #[test]
    fn daemon_instance_id_is_serialized_when_set() {
        let cfg = ClientConfig {
            daemon_instance_id: Some("udoo".to_string()),
            ..empty_config()
        };
        let json = serde_json::to_value(cfg).expect("serialize");
        assert_eq!(
            json.get("daemon_instance_id").and_then(|v| v.as_str()),
            Some("udoo")
        );
    }

    /// The jail capability the web gates the **sandboxed codebase** placement on. A daemon with no
    /// common room advertises it nowhere else, so this key is that deployment's only source.
    #[test]
    fn the_jail_capability_is_omitted_from_api_config_when_none() {
        // Given a host that serves no jailed-codebase placement
        let cfg = empty_config();

        // When it describes itself to the page it serves
        let json = serde_json::to_value(cfg).expect("serialize");

        // Then the key is off the wire entirely, so absent cannot be read as "confines nothing"
        assert!(
            json.get("sandboxed_codebase").is_none(),
            "sandboxed_codebase must be skipped when None: {json}"
        );
    }

    #[test]
    fn a_jail_that_confines_the_filesystem_is_serialized_as_the_advertisement_spells_it() {
        // Given a host whose jail denies writes outside the checkout
        let cfg = ClientConfig {
            sandboxed_codebase: Some(ClientSandboxedCodebaseSupport {
                confines_filesystem: true,
            }),
            ..empty_config()
        };

        // When it describes itself to the page it serves
        let json = serde_json::to_value(cfg).expect("serialize");

        // Then it uses the one wire shape the web reads, whichever source described the host
        assert_eq!(
            json.get("sandboxed_codebase"),
            Some(&serde_json::json!({ "confines_filesystem": true }))
        );
    }

    #[test]
    fn a_jail_that_shares_the_filesystem_root_is_serialized_as_confining_nothing_of_it() {
        // Given a host whose jail confines process and network but not writes
        let cfg = ClientConfig {
            sandboxed_codebase: Some(ClientSandboxedCodebaseSupport {
                confines_filesystem: false,
            }),
            ..empty_config()
        };

        // When it describes itself to the page it serves
        let json = serde_json::to_value(cfg).expect("serialize");

        // Then the boolean is carried honestly rather than skipped as a falsy default — the page
        // turns it into the caveat it shows, and a dropped `false` would read as unadvertised
        assert_eq!(
            json.get("sandboxed_codebase"),
            Some(&serde_json::json!({ "confines_filesystem": false }))
        );
    }
}
