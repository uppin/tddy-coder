//! The connection to the session's **facilitating** daemon, and the identity its requests carry.
//!
//! One place, because the roster stream and the conversation RPCs must reach the same daemon over
//! the same transport: a stream that followed one daemon's roster while conversations opened on
//! another would resolve agent ids against a roster nobody was serving.
//!
//! The in-jail transport is not this module's choice — `detect_session_tool_transport` made it at
//! spawn. What is here is which of those transports can carry an RPC to the daemon at all, and what
//! envelope it needs: the sandbox socket identifies the session by the connection itself, while a
//! remote daemon resolves `session_id` against its own sessions base and authenticates
//! `session_token`, so an empty envelope there would find no session and no user.

use std::sync::Arc;

use crate::session_tool_client::{SessionToolEnvelope, SessionToolTransport};

/// Connect to the facilitating daemon over `transport`.
///
/// The sandbox socket gets a connection of its **own** per caller: it opens a fresh `UnixStream`
/// per dispatch, and a stream held for the process lifetime must not be torn down with a tool
/// call's. Over LiveKit the room session is shared and cached, as every other call over it is.
pub(crate) async fn connect_facilitating_daemon(
    transport: &SessionToolTransport,
) -> Result<(Arc<dyn tddy_rpc::RpcClientTransport>, SessionToolEnvelope), String> {
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            let client = crate::session_tool_client::connect_sandbox_ipc(socket_path).await?;
            // The socket identifies the session to the sandbox-runner, as it does for every tool
            // call over it, so the envelope stays empty.
            Ok((client, SessionToolEnvelope::default()))
        }
        #[cfg(feature = "livekit")]
        SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            let key = crate::session_tool_client::LiveKitRoomKey {
                url: url.clone(),
                room: room.clone(),
                token: token.clone(),
                server_identity: server_identity.clone(),
            };
            let session = crate::session_tool_client::livekit_session(&key).await?;
            if !session.peer_present() {
                return Err(format!(
                    "daemon \"{server_identity}\" is not in room \"{room}\""
                ));
            }
            Ok((
                Arc::clone(session.transport()),
                SessionToolEnvelope {
                    session_id: session_id.clone(),
                    session_token: session_token.clone(),
                    daemon_instance_id: daemon_instance_id.clone(),
                },
            ))
        }
        #[cfg(not(feature = "livekit"))]
        SessionToolTransport::LiveKit { .. } => Err(
            "this tddy-tools was built without the 'livekit' feature, so the session's daemon \
             cannot be reached at all"
                .to_string(),
        ),
        // Refused rather than served over a second transport: the HTTP relay reaches the daemon's
        // `ExecuteTool` only, and an `IncompleteLiveKit` environment is a split session whose
        // variables are broken, where a stray relay would answer from the wrong host.
        SessionToolTransport::DaemonHttp { .. } => Err(
            "the daemon-HTTP transport has no client for the roster and conversation RPCs"
                .to_string(),
        ),
        SessionToolTransport::IncompleteLiveKit { missing } => Err(format!(
            "a LiveKit environment is set but {} is empty or unset",
            missing.join(", ")
        )),
    }
}
