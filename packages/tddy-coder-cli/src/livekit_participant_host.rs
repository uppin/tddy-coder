//! [`LiveKitParticipantHost`]: serves the coder's RPC surface as a LiveKit room participant.
//!
//! Two modes, chosen by the join: with API credentials and `reconnect`, a token is minted for the
//! room and the participant rejoins on its own (`run_with_reconnect_metadata`); with a fixed token
//! it connects once, publishes participant metadata from a watcher, and ends with the room or on
//! shutdown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_coder::participant_host::{
    ParticipantAuth, ParticipantJoin, ParticipantOptions, ServedParticipant, SessionParticipantHost,
};
use tddy_livekit::{tddy_room_options, LiveKitParticipant, TokenGenerator};

/// How often a fixed-token participant looks at the shutdown flag.
const SHUTDOWN_POLL: Duration = Duration::from_millis(100);

/// Serves the coder's surface through `tddy-livekit`.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveKitParticipantHost;

/// Mints room tokens for other participants from the coder's API credentials.
struct GeneratorTokenProvider(TokenGenerator);

impl tddy_service::TokenProvider for GeneratorTokenProvider {
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
        self.0
            .generate_for(room, identity)
            .map_err(|e| e.to_string())
    }
    fn ttl_seconds(&self) -> u64 {
        self.0.ttl().as_secs()
    }
}

#[async_trait::async_trait]
impl SessionParticipantHost for LiveKitParticipantHost {
    async fn serve(
        &self,
        join: ParticipantJoin,
        surface: tddy_rpc::MultiRpcService,
        options: ParticipantOptions,
    ) -> Result<ServedParticipant, String> {
        let ParticipantJoin {
            url,
            identity,
            auth,
        } = join;
        match (auth, options.reconnect) {
            (
                ParticipantAuth::Credentials {
                    api_key,
                    api_secret,
                    room,
                    ttl,
                },
                true,
            ) => {
                let generator = TokenGenerator::new(api_key, api_secret, room, identity, ttl);
                Ok(ServedParticipant::new(tokio::spawn(async move {
                    LiveKitParticipant::run_with_reconnect_metadata(
                        &url,
                        &generator,
                        surface,
                        tddy_room_options(tddy_livekit::ContinualGatheringPolicy::GatherOnce),
                        options.shutdown,
                        options.metadata,
                        options.codex_oauth_watch,
                        options.projects_registry_dir,
                    )
                    .await
                })))
            }
            (
                ParticipantAuth::Credentials {
                    api_key,
                    api_secret,
                    room,
                    ttl,
                },
                false,
            ) => {
                let token = TokenGenerator::new(api_key, api_secret, room, identity.clone(), ttl)
                    .generate()
                    .map_err(|e| format!("token generation for {identity} failed: {e}"))?;
                serve_once(&url, &identity, &token, surface, options).await
            }
            (ParticipantAuth::Token(token), false) => {
                serve_once(&url, &identity, &token, surface, options).await
            }
            (ParticipantAuth::Token(_), true) => Err(format!(
                "cannot serve {identity} with reconnect: a fixed token cannot be re-minted"
            )),
        }
    }

    fn token_provider(
        &self,
        api_key: &str,
        api_secret: &str,
        ttl: Duration,
    ) -> Result<Arc<dyn tddy_service::TokenProvider>, String> {
        // The generator's own room and identity serve only `generate()`; the token service always
        // asks for a room and identity of its own (`generate_for`).
        Ok(Arc::new(GeneratorTokenProvider(TokenGenerator::new(
            api_key.to_string(),
            api_secret.to_string(),
            String::new(),
            String::new(),
            ttl,
        ))))
    }
}

/// Connect once with `token`, then run until the room disconnects or shutdown is set.
async fn serve_once(
    url: &str,
    identity: &str,
    token: &str,
    surface: tddy_rpc::MultiRpcService,
    options: ParticipantOptions,
) -> Result<ServedParticipant, String> {
    let ParticipantOptions {
        metadata,
        codex_oauth_watch,
        projects_registry_dir,
        shutdown,
        ..
    } = options;
    let participant = LiveKitParticipant::connect(
        url,
        token,
        surface,
        tddy_room_options(tddy_livekit::ContinualGatheringPolicy::GatherOnce),
        codex_oauth_watch,
        projects_registry_dir,
    )
    .await
    .map_err(|e| format!("connecting {identity} to {url}: {e}"))?;
    log::info!("READY");

    let metadata_task = metadata.map(|rx| {
        tddy_livekit::spawn_local_participant_metadata_watcher(
            rx,
            participant.room().local_participant().clone(),
            participant.metadata_publish_lock(),
        )
    });
    Ok(ServedParticipant::new(tokio::spawn(async move {
        tokio::select! {
            _ = participant.run() => log::info!("LiveKit participant disconnected"),
            _ = shutdown_requested(&shutdown) => {}
        }
        if let Some(task) = metadata_task {
            task.abort();
        }
    })))
}

/// Resolves once `shutdown` is set.
async fn shutdown_requested(shutdown: &AtomicBool) {
    while !shutdown.load(Ordering::Relaxed) {
        tokio::time::sleep(SHUTDOWN_POLL).await;
    }
}
