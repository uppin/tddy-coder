//! tddy-grpc: gRPC remote control for tddy-coder.
//!
//! Exposes TddyRemote service for programmatic control via bidirectional streaming:
//! clients send UserIntent, receive PresenterView events.

pub mod convert;
pub mod service;

pub use convert::{client_message_to_intent, event_to_server_message};
pub use service::TddyRemoteService;

pub mod gen {
    tonic::include_proto!("tddy.v1");
}

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod test_util {
    use tddy_core::{ActivityEntry, AppMode, PresenterView};

    /// Minimal PresenterView for tests (no-op).
    pub struct NoopView;

    impl PresenterView for NoopView {
        fn on_mode_changed(&mut self, _mode: &AppMode) {}
        fn on_activity_logged(&mut self, _entry: &ActivityEntry, _activity_log_len: usize) {}
        fn on_goal_started(&mut self, _goal: &str) {}
        fn on_state_changed(&mut self, _from: &str, _to: &str) {}
        fn on_workflow_complete(&mut self, _result: &Result<String, String>) {}
        fn on_agent_output(&mut self, _text: &str) {}
        fn on_inbox_changed(&mut self, _inbox: &[String]) {}
    }
}
