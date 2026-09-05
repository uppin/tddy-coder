//! `daemon_config.DaemonConfigService`: the daemon's own settings, read and written by its UI.
//!
//! The YAML file the daemon was loaded from stays the source of truth. An update validates,
//! rewrites that file, and applies to the running process what can be applied — naming in the
//! response whatever it could not, rather than accepting a change that quietly does nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::daemon_config::{
    ClientAllowedAgent, DaemonConfigService as DaemonConfigServiceTrait, GetClientConfigRequest,
    GetClientConfigResponse, GetConfigRequest, GetConfigResponse, UpdateConfigRequest,
    UpdateConfigResponse,
};

use crate::config::{DaemonConfig, LiveKitConfig};
use crate::daemon_settings::{apply_update, redacted_settings};

/// Applies a new LiveKit configuration to the running common-room connection.
///
/// Injected so this service never learns how the connection is supervised — and so a test can
/// observe that a URL change actually reached it.
pub trait CommonRoomSupervisor: Send + Sync + 'static {
    /// Disconnect the current common room, if any, and connect the one `livekit` describes.
    /// `None` leaves the daemon disconnected.
    fn reconfigure(&self, livekit: Option<LiveKitConfig>);
}

/// Decides whether the caller's `session_token` may read or write the daemon's configuration.
/// `true` admits the call; `false` refuses it with `UNAUTHENTICATED`.
pub type SessionTokenAuthenticator = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Serves the daemon's configuration.
pub struct DaemonConfigServiceImpl {
    /// The YAML file an update rewrites. `None` when the daemon was started without one, which
    /// makes every update a refusal — there is nowhere to persist it.
    config_path: Option<PathBuf>,
    config: Arc<Mutex<DaemonConfig>>,
    common_room: Arc<dyn CommonRoomSupervisor>,
    authenticate: SessionTokenAuthenticator,
}

impl DaemonConfigServiceImpl {
    pub fn new(
        config_path: Option<PathBuf>,
        config: Arc<Mutex<DaemonConfig>>,
        common_room: Arc<dyn CommonRoomSupervisor>,
        authenticate: SessionTokenAuthenticator,
    ) -> Self {
        Self {
            config_path,
            config,
            common_room,
            authenticate,
        }
    }

    /// Admit the caller, or refuse the call. The daemon's configuration holds its LiveKit
    /// credentials, so an unauthenticated read is a credential leak and an unauthenticated write is
    /// a takeover.
    fn admit(&self, session_token: &str) -> Result<(), Status> {
        if (self.authenticate)(session_token) {
            return Ok(());
        }
        Err(Status::unauthenticated(
            "a valid session token is required to read or write the daemon configuration",
        ))
    }

    /// The file an update writes to, or a refusal when the daemon was started without one — a
    /// daemon with nowhere to persist an update must say so rather than accept a change that is
    /// lost on restart.
    fn writable_config_path(&self) -> Result<&Path, Status> {
        self.config_path.as_deref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon was started without a config file, so there is nowhere to persist an update",
            )
        })
    }
}

/// Replace `path`'s contents with `config` as YAML, atomically: the new file is written beside the
/// target and renamed over it, so a write that fails midway leaves the operator's configuration
/// exactly as it was rather than truncated.
///
/// TODO: re-serializing loses the comments in the operator's file (`dev.desktop.yaml` is heavily
/// commented). Preserving them needs a comment-aware YAML editor, which is a new dependency and so
/// the developer's call.
fn write_config(path: &Path, config: &DaemonConfig) -> Result<(), Status> {
    let yaml = serde_yaml::to_string(config)
        .map_err(|e| Status::internal(format!("failed to serialize the daemon config: {e}")))?;
    let dir = path.parent().ok_or_else(|| {
        Status::internal(format!(
            "config path {} names no directory to write into",
            path.display()
        ))
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| {
        Status::internal(format!(
            "failed to create a temporary file in {}: {e}",
            dir.display()
        ))
    })?;
    std::io::Write::write_all(&mut temp, yaml.as_bytes())
        .map_err(|e| Status::internal(format!("failed to write the daemon config: {e}")))?;
    // A rename carries the temp file's permissions, not the target's, so the file the operator
    // created keeps the mode they gave it.
    if let Ok(existing) = std::fs::metadata(path) {
        temp.as_file()
            .set_permissions(existing.permissions())
            .map_err(|e| {
                Status::internal(format!("failed to set the daemon config permissions: {e}"))
            })?;
    }
    temp.persist(path).map_err(|e| {
        Status::internal(format!(
            "failed to replace {} with the updated config: {e}",
            path.display()
        ))
    })?;
    Ok(())
}

#[async_trait]
impl DaemonConfigServiceTrait for DaemonConfigServiceImpl {
    async fn get_config(
        &self,
        request: Request<GetConfigRequest>,
    ) -> Result<Response<GetConfigResponse>, Status> {
        let request = request.into_inner();
        self.admit(&request.session_token)?;

        let config = self.config.lock().await;
        Ok(Response::new(GetConfigResponse {
            settings: Some(redacted_settings(&config)),
            // The path the daemon was started with, so the UI can name the file it is editing.
            // Empty when there is none, as the response's own contract says.
            config_path: self
                .config_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
        }))
    }

    async fn update_config(
        &self,
        request: Request<UpdateConfigRequest>,
    ) -> Result<Response<UpdateConfigResponse>, Status> {
        let request = request.into_inner();
        self.admit(&request.session_token)?;
        let settings = request.settings.ok_or_else(|| {
            Status::invalid_argument("settings are required to update the config")
        })?;
        let path = self.writable_config_path()?;

        // Held across the validate-write-adopt sequence: two concurrent updates each merging onto
        // the configuration they read would leave the file and the running daemon disagreeing.
        let mut config = self.config.lock().await;
        // Validated before the file is touched, so a refused update leaves it untouched.
        let update = apply_update(&config, &settings)?;
        write_config(path, &update.config)?;
        let livekit = update.config.livekit.clone();
        *config = update.config;
        drop(config);

        if update.reconnect_common_room {
            self.common_room.reconfigure(livekit);
        }

        Ok(Response::new(UpdateConfigResponse {
            restart_required: update.restart_required,
        }))
    }

    async fn get_client_config(
        &self,
        request: Request<GetClientConfigRequest>,
    ) -> Result<Response<GetClientConfigResponse>, Status> {
        // Deliberately ungated, unlike every other method here. This is the payload that tells the
        // app there *is* a daemon to sign in to, so it is read before any session token exists — a
        // gate here would make a desktop webview unable to bootstrap. It is the same snapshot the
        // daemon already serves unauthenticated at `GET /api/config`, and it carries no secrets:
        // a LiveKit URL and room name, the agent allowlist, the debug mask and the instance id.
        // `session_token` stays on the request so a signed-in caller may send it; it is not read.
        let _ = request.into_inner();

        let config = self.config.lock().await;
        Ok(Response::new(GetClientConfigResponse {
            livekit_url: config.livekit.as_ref().and_then(|lk| lk.url.clone()),
            // Session rooms are joined per session, so the startup payload names none.
            livekit_room: None,
            common_room: config
                .livekit
                .as_ref()
                .and_then(|lk| lk.common_room.clone()),
            daemon_mode: Some(true),
            // The same allowlist rows the HTTP `/api/config` snapshot carries: config entries only,
            // because assistants come and go while the daemon runs and `ListAgents` is their live
            // source.
            allowed_agents: crate::agent_list_mapping::agent_allowlist_rows(&config, &[])
                .into_iter()
                .map(|row| ClientAllowedAgent {
                    id: row.id,
                    label: row.display_label,
                })
                .collect(),
            debug: config.debug.clone(),
            daemon_instance_id: Some(crate::livekit_peer_discovery::local_instance_id_for_config(
                &config,
            )),
        }))
    }
}
