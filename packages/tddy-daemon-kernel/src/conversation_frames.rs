//! The live ACP frames of a daemon-run subagent conversation, one broadcast per conversation.
//!
//! A conversation's transcript is appended to disk one frame at a time, and each frame appended is
//! published here with its position in that transcript. A replay reads the transcript and then tails
//! this channel: it subscribes first and reads the transcript second, so a frame appended in between is
//! on the channel, and a frame appended before the read is in the transcript. The replay drops the live
//! frames the transcript already gave it, the ones whose `seq` is below the next seq it has sent.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::collections::HashMap;
use std::sync::Mutex;

use tddy_service::proto::acp::AcpAgentMessage;
use tokio::sync::broadcast;

/// One frame of a conversation's transcript, and where in it the frame sits.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversationFrame {
    /// The frame's 0-based position in the conversation's transcript.
    pub seq: u64,
    pub frame: AcpAgentMessage,
}

/// Every conversation's frame broadcast, keyed by session id and conversation id.
///
/// Held once by the daemon and shared by the recorder that publishes into it and the replay that tails
/// it, so both reach the same channel. A conversation's channel is created by its first subscriber, and a
/// publish with no subscriber is a no-op: the transcript on disk is the record, and the channel only
/// carries what a reader is waiting for.
#[derive(Default)]
pub struct ConversationFrames {
    senders: Mutex<HashMap<(String, String), broadcast::Sender<ConversationFrame>>>,
}

impl ConversationFrames {
    /// Broadcast capacity per conversation. A reader that falls this many frames behind misses some.
    pub const CAPACITY: usize = 1024;

    /// Subscribe to a conversation's frames, creating its channel on first use.
    ///
    /// Channels no reader holds any more are dropped here, so a conversation id that is subscribed to
    /// and then refused (it recorded nothing) does not stay in the map.
    pub fn subscribe(
        &self,
        session_id: &str,
        conversation_id: &str,
    ) -> broadcast::Receiver<ConversationFrame> {
        let mut senders = self.senders.lock().unwrap_or_else(|e| e.into_inner());
        senders.retain(|_, sender| sender.receiver_count() > 0);
        senders
            .entry((session_id.to_string(), conversation_id.to_string()))
            .or_insert_with(|| broadcast::channel(Self::CAPACITY).0)
            .subscribe()
    }

    /// Publish a frame of a conversation to its subscribers. A no-op when none is subscribed.
    pub fn publish(&self, session_id: &str, conversation_id: &str, frame: ConversationFrame) {
        let sender = self
            .senders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(session_id.to_string(), conversation_id.to_string()))
            .cloned();
        if let Some(sender) = sender {
            // `Err` means no live receiver is left; the transcript on disk still holds the frame.
            let _ = sender.send(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_service::acp_replay::agent_text_frame;

    fn frame(seq: u64, text: &str) -> ConversationFrame {
        ConversationFrame {
            seq,
            frame: agent_text_frame(text, 0),
        }
    }

    #[test]
    fn a_subscriber_receives_the_frames_published_after_it_subscribed() {
        let frames = ConversationFrames::default();
        let mut rx = frames.subscribe("session-1", "conv-1");

        frames.publish("session-1", "conv-1", frame(0, "first"));

        assert_eq!(rx.try_recv().expect("delivered"), frame(0, "first"));
    }

    #[test]
    fn a_frame_published_before_anyone_subscribed_is_not_delivered() {
        let frames = ConversationFrames::default();

        frames.publish("session-1", "conv-1", frame(0, "too early"));
        let mut rx = frames.subscribe("session-1", "conv-1");

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_conversation_is_keyed_by_its_session_as_well_as_its_id() {
        let frames = ConversationFrames::default();
        let mut rx = frames.subscribe("session-1", "conv-1");

        frames.publish("session-2", "conv-1", frame(0, "another session's"));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_subscription_nobody_holds_is_dropped_by_the_next_subscribe() {
        let frames = ConversationFrames::default();
        drop(frames.subscribe("session-1", "conv-refused"));

        let _held = frames.subscribe("session-1", "conv-1");

        assert_eq!(frames.senders.lock().expect("senders").len(), 1);
    }
}
