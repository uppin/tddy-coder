// TODO(restructure): hand-moved — extract_method refused under #[async_trait]; see docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md
//
// The bodies of the four host-target RPCs, moved out of the `#[async_trait]` impl byte for byte; the
// trait methods in `screen_sharing_service.rs` delegate here.
use super::ScreenSharingServiceImpl;
use crate::bridge_config::{
    browser_livekit_url, host_bridge_identity, host_bridge_key, host_livekit_room,
    host_target_as_bridge_target, host_target_to_proto, prepare_bridge, screenshare_track_name,
};
use tddy_daemon_kernel::config::{resolve_x11vnc_binary_path, ScreenSharingConfig};
use tddy_host_service::host_desktop_targets::HostDesktopTarget;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    AddHostTargetRequest, AddHostTargetResponse, ListHostTargetsRequest, ListHostTargetsResponse,
    StartHostStreamRequest, StartStreamResponse, StopHostStreamRequest, StopHostStreamResponse,
};

impl ScreenSharingServiceImpl {
    pub(crate) async fn list_host_targets_impl(
        &self,
        request: Request<ListHostTargetsRequest>,
    ) -> Result<Response<ListHostTargetsResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;

        let targets = scope
            .targets
            .list(&req.daemon_instance_id)
            .map_err(Status::internal)?;

        Ok(Response::new(ListHostTargetsResponse {
            targets: targets.iter().map(host_target_to_proto).collect(),
        }))
    }

    pub(crate) async fn add_host_target_impl(
        &self,
        request: Request<AddHostTargetRequest>,
    ) -> Result<Response<AddHostTargetResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;

        // Refused rather than wrapped: proto has no 16-bit integer, so a port past 65535 arrives
        // here intact and a cast would quietly attach a desktop to some other port entirely.
        let port = u16::try_from(req.port).map_err(|_| {
            Status::invalid_argument(format!(
                "{} is not a port a desktop can listen on",
                req.port
            ))
        })?;

        // No password field, and none accepted: a host desktop's password is prompted when it is
        // opened, never stored beside the address of the machine it opens.
        let target_id = scope
            .targets
            .add(
                &req.daemon_instance_id,
                HostDesktopTarget {
                    // Assigned by the store; whatever is sent here would be overwritten.
                    target_id: String::new(),
                    label: req.label,
                    host: req.host,
                    port,
                    protocol: req.protocol,
                    username: req.username,
                    display: req.x11_display,
                },
            )
            .map_err(Status::internal)?;

        Ok(Response::new(AddHostTargetResponse { target_id }))
    }

    /// Starts a bridge for a host-scoped target, returning the same coordinates the session-scoped
    /// call does — the browser overlay already consumes exactly those fields.
    pub(crate) async fn start_host_stream_impl(
        &self,
        request: Request<StartHostStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        let req = request.into_inner();
        let operator = self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;
        let config = self.require_config()?;

        let target = scope
            .targets
            .list(&req.daemon_instance_id)
            .map_err(Status::internal)?
            .into_iter()
            .find(|t| t.target_id == req.target_id)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "host {} has no desktop target {}",
                    req.daemon_instance_id, req.target_id
                ))
            })?;

        let livekit_room = host_livekit_room(config)?;
        let bridge_identity = host_bridge_identity(&req.daemon_instance_id, &req.target_id);
        let track_name = screenshare_track_name(&req.target_id);

        // Settled before anybody is asked for a secret. A desktop this host cannot bridge — no
        // LiveKit to publish through, no protocol to speak — is refused here, so an operator is
        // never made to type a password into a dialog for a stream that could not have started.
        let prepared = prepare_bridge(
            config,
            &host_target_as_bridge_target(&target),
            target.username.clone(),
            &bridge_identity,
            &track_name,
            &livekit_room,
            &*self.room_tokens,
        )
        .map_err(Status::failed_precondition)?;

        // Asked for, not looked up: nothing on this host stores a desktop password, and the browser
        // could not have sent one — `HostPromptEvent` is the only place this host's public key is
        // published, so a caller has nothing to encrypt under until it has been asked. Read once,
        // handed to the bridge over its stdin, and dropped when this call returns.
        let password = self
            .prompt_for_desktop_password(scope, &operator, &target)
            .await?;

        // Reported: these coordinates tell a browser to mount an overlay on a room, and a bridge that
        // never joined it leaves the operator watching nothing having already typed a secret. The
        // size is the one the bridge measured, so input lands where the operator aimed.
        let bridge_key = host_bridge_key(&req.daemon_instance_id, &req.target_id);
        let mut prepared = prepared.for_password(password);
        // An X11 display has no VNC server until one is started for it, and it is started before the
        // bridge so the bridge has something to connect to.
        if let Some(display) = target.display {
            // A reopen stops the old bridge *before* its x11vnc. Stopping the x11vnc first makes the
            // old bridge lose its VNC server and exit on its own while its key still names it, and
            // its reaper then stops whatever x11vnc the key has by then — the one being started
            // here. Forgotten first, the old bridge's exit no longer reaches the new x11vnc.
            self.terminate_bridge(&bridge_key).await;
            let x11vnc = self
                .x11vnc
                .start(
                    &bridge_key,
                    &resolve_x11vnc_binary_path(&screen_sharing_of(config)),
                    display,
                )
                .await?;
            prepared.spawn_config.host = "127.0.0.1".to_string();
            prepared.spawn_config.port = x11vnc.port;
        }
        let started = match self.spawn_bridge(prepared, bridge_key.clone()).await {
            Ok(started) => started,
            Err(status) => {
                // A start that failed leaves no x11vnc behind; stopping a key with none is a no-op.
                self.x11vnc.stop(&bridge_key).await;
                return Err(status);
            }
        };

        Ok(Response::new(StartStreamResponse {
            livekit_room,
            livekit_url: browser_livekit_url(config),
            bridge_identity,
            track_name,
            width: started.width,
            height: started.height,
        }))
    }

    pub(crate) async fn stop_host_stream_impl(
        &self,
        request: Request<StopHostStreamRequest>,
    ) -> Result<Response<StopHostStreamResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        // Checked here as it is on every other host-scoped call: a daemon that serves no host
        // desktops cannot have been holding one open, and `ok: true` for a stop it could not have
        // performed tells the browser to tear down an overlay over a bridge still running.
        self.require_host_scope()?;

        let bridge_key = host_bridge_key(&req.daemon_instance_id, &req.target_id);
        self.terminate_bridge(&bridge_key).await;
        self.x11vnc.stop(&bridge_key).await;

        Ok(Response::new(StopHostStreamResponse { ok: true }))
    }
}

/// The daemon's `screen_sharing:` block, or the defaults of one when it has none.
fn screen_sharing_of(config: &tddy_daemon_kernel::config::DaemonConfig) -> ScreenSharingConfig {
    config.screen_sharing.clone().unwrap_or_default()
}
