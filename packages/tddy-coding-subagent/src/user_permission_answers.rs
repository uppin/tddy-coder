//! A permission request the main agent handed to the user, answered from the web.
//!
//! The trait and who-answered record live in `tddy-discovery`, beside the request they answer, so
//! a conversation can expose its parked requests without depending on this crate. They are
//! re-exported here, where they have always been named.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

pub use tddy_discovery::subagent::{AnsweredBy, PermissionAnswering};

use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView};

use crate::parked_permissions::ParkedPermissions;

impl PermissionAnswering for ParkedPermissions {
    fn answer_as(&self, answer: &PermissionAnswer, by: AnsweredBy) -> Result<(), String> {
        self.answer_recording(answer, by)
    }

    fn answered_by(&self, request_id: &str) -> Option<AnsweredBy> {
        self.answerer_of(request_id)
    }

    fn deferred_to_user(&self, request_id: &str) -> bool {
        self.is_deferred_to_user(request_id)
    }

    fn pending_request(&self) -> Option<PermissionRequestView> {
        self.pending()
    }
}
