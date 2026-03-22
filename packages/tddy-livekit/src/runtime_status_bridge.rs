//! Helpers for `ServerMessage` chunks containing
//! [`tddy_service::gen::SessionRuntimeStatus`] on the LiveKit bidi stream.

use prost::Message;

use tddy_service::gen::server_message;
use tddy_service::gen::ServerMessage;

/// Returns whether a decoded `ServerMessage` payload should be forwarded to subscribers.
pub fn session_runtime_status_envelope_should_forward(server_message_bytes: &[u8]) -> bool {
    log::trace!(
        "session_runtime_status_envelope_should_forward len={}",
        server_message_bytes.len()
    );
    match ServerMessage::decode(server_message_bytes) {
        Ok(msg) => matches!(
            msg.event,
            Some(server_message::Event::SessionRuntimeStatus(_))
        ),
        Err(e) => {
            log::debug!(
                "session_runtime_status_envelope_should_forward decode err: {}",
                e
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use tddy_service::gen::server_message;
    use tddy_service::gen::{ServerMessage, SessionRuntimeStatus};

    use super::*;

    #[test]
    fn session_runtime_status_server_message_is_forwarded_on_livekit() {
        let msg = ServerMessage {
            event: Some(server_message::Event::SessionRuntimeStatus(
                SessionRuntimeStatus {
                    session_id: "s".into(),
                    goal: "g".into(),
                    workflow_state: "w".into(),
                    elapsed_ms: 1,
                    agent: "a".into(),
                    model: "m".into(),
                    status_line: String::new(),
                },
            )),
        };
        let bytes = msg.encode_to_vec();
        assert!(
            session_runtime_status_envelope_should_forward(&bytes),
            "LiveKit bridge must forward SessionRuntimeStatus chunks on TddyRemote/Stream"
        );
    }
}
