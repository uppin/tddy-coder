//! Shared ACP glue for tddy.
//!
//! Houses the pure event/intent [`mapping`] between tddy's internal workflow types and the Agent
//! Client Protocol, used by both the `tddy-coder --acp` agent (workflow → ACP) and the session-host
//! bridge (ACP → the web's `TddyRemote` stream). The unified ACP client and the agent
//! implementation land here too as the extraction proceeds.

pub mod mapping;
