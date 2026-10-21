use super::ScreenSharingServiceImpl;

use crate::bridge_config::session_bridge_key;

use super::BridgeTarget;

use super::target_error_to_status;

use crate::screen_sharing_records::ScreenSharingTargetStore;

use std::sync::Arc;

use crate::bridge_config::browser_livekit_url;

use crate::bridge_config::screenshare_track_name;

use log::error;
use tddy_rpc::Status;

use tddy_service::proto::screen_sharing::StartStreamResponse;

use tddy_rpc::Response;

use tddy_service::proto::screen_sharing::StartStreamRequest;

use tddy_rpc::Request;

impl ScreenSharingServiceImpl {
    /// Body of [`ScreenSharingService::start_stream`]; the trait method delegates here.
    pub(crate) async fn start_stream_impl(
        &self,
        request: Request<StartStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let store = self.require_target_store()?;

        let metadata = tddy_core::session_metadata::read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("failed to read session metadata: {}", e)))?;
        let livekit_room = metadata.livekit_room.unwrap_or_default();

        let bridge_identity = format!("screenshare-{}-{}", req.session_id, req.target_id);
        let track_name = screenshare_track_name(&req.target_id);

        let livekit_url = self
            .config
            .as_deref()
            .map(browser_livekit_url)
            .unwrap_or_default();

        // Spawn the bridge process when daemon config is available. The start waits for that bridge
        // to report its desktop up, so a refused sign-in or a bridge that never came up fails this
        // call rather than handing back coordinates for a stream that will never carry a frame.
        let started = self
            .start_session_bridge(req, store, &livekit_room, &bridge_identity, &track_name)
            .await?;

        // With no bridge started there is no desktop to have measured, so the size is zero rather
        // than a guess.
        let (width, height) = started.map_or((0, 0), |bridge| (bridge.width, bridge.height));

        Ok(Response::new(StartStreamResponse {
            livekit_room,
            livekit_url,
            bridge_identity,
            track_name,
            width,
            height,
        }))
    }

    pub(crate) async fn start_session_bridge(
        &self,
        req: StartStreamRequest,
        store: &Arc<dyn ScreenSharingTargetStore>,
        livekit_room: &str,
        bridge_identity: &str,
        track_name: &str,
    ) -> Result<Option<crate::BridgeStarted>, Status> {
        let mut started = None;
        if let Some(ref config) = self.config {
            let target = store
                .list(&req.session_token)
                .map_err(target_error_to_status)?
                .into_iter()
                .find(|t| t.id == req.target_id);
            match target {
                Some(target) => {
                    let password = store
                        .password_for(&req.session_token, &req.target_id)
                        .map_err(target_error_to_status)?;
                    match BridgeTarget::from_proto(&target) {
                        Ok(bridge_target) => {
                            started = Some(
                                self.try_spawn_bridge(
                                    config,
                                    &bridge_target,
                                    target.username.clone(),
                                    password,
                                    bridge_identity,
                                    track_name,
                                    livekit_room,
                                    session_bridge_key(&req.session_id, &req.target_id),
                                )
                                .await?,
                            );
                        }
                        Err(e) => error!("start_stream: target '{}': {}", req.target_id, e),
                    }
                }
                None => error!("start_stream: target '{}' not found", req.target_id),
            }
        }
        Ok(started)
    }
}
