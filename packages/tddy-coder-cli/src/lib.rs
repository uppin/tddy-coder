//! The coder's room participant over LiveKit: [`LiveKitParticipantHost`] implements
//! `tddy-coder`'s `SessionParticipantHost` port, so the coder library never names LiveKit.

pub mod livekit_participant_host;

pub use livekit_participant_host::LiveKitParticipantHost;
