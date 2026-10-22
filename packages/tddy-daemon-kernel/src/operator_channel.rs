//! The operator channel: the port a session's elicitations and attention notifications reach an
//! operator's chat through, without the core learning which chat it is.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-22-telegram-satellite-integration.md`, changes 1 and 2.
//! Telegram is the one channel today; the daemon's wiring installs it, or [`NoOperatorChannel`] on
//! a daemon with no `telegram:` block.
//!
//! A channel is reached two ways, and both end here:
//!
//! - **presenter events** — a workflow session's presenter stream, in order (elicitations, state
//!   lines), handed over by the session's presenter observer: [`OperatorChannel::on_presenter_event`];
//! - **notifications** — the attention-worthy events off the session-notification bus, which
//!   `tddy_session_activity::session_notifications::OperatorChannelSubscriber` registers a channel
//!   on: [`OperatorChannel::wants_notification`] and [`OperatorChannel::on_notification`].
//!
//! The presenter observer runs whether or not a channel is wired — the notification bus wants the
//! same events, and they are what light a workflow session's drawer indicator. That is why the
//! rule for "this presenter mode waits on a human" is the core's ([`awaits_operator`]) and not a
//! channel's: it must hold on a daemon that has no channel at all.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::{ModeChanged, ServerMessage};

use crate::session_notification::SessionNotification;

/// A chat an operator is reached on.
#[async_trait]
pub trait OperatorChannel: Send + Sync {
    /// The id the channel logs under and reports on the integration status board.
    fn id(&self) -> &'static str;

    /// Handle one presenter event of `session_id`, in stream order.
    ///
    /// An error ends that session's presenter observer, exactly as a failed stream read does: the
    /// channel is telling the observer it can no longer follow the session.
    async fn on_presenter_event(
        &self,
        session_id: &str,
        event: &ServerMessage,
    ) -> anyhow::Result<()>;

    /// Whether the channel wants `notification`. Asked before [`Self::on_notification`], so a
    /// channel that sends over a network is never handed what it would only discard.
    fn wants_notification(&self, notification: &SessionNotification) -> bool;

    /// Deliver `notification`. An error is logged by the bus and never reaches the RPC that
    /// published it.
    async fn on_notification(&self, notification: &SessionNotification) -> anyhow::Result<()>;
}

/// The port as the session host and the notification bus hold it.
pub type SharedOperatorChannel = Arc<dyn OperatorChannel>;

/// The channel of a daemon with no chat surface: it wants no notification and accepts every
/// presenter event, so a session's presenter observer goes on running for the notification bus.
pub struct NoOperatorChannel;

impl NoOperatorChannel {
    /// [`NoOperatorChannel`]'s [`OperatorChannel::id`] — how a caller tells that no channel is
    /// wired, and so that nothing but the notification bus would follow a presenter stream.
    pub const ID: &'static str = "none";
}

#[async_trait]
impl OperatorChannel for NoOperatorChannel {
    fn id(&self) -> &'static str {
        Self::ID
    }

    async fn on_presenter_event(
        &self,
        _session_id: &str,
        _event: &ServerMessage,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn wants_notification(&self, _notification: &SessionNotification) -> bool {
        false
    }

    async fn on_notification(&self, _notification: &SessionNotification) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Whether the presenter mode `mode_changed` switches to waits on the operator: document review,
/// markdown viewer, feature input, select, multi-select and text input do. The autonomous modes
/// (`Running`, `Done`) and a mode change that names no mode do not.
///
/// The one rule for it. The drawer indicator turns a session's dot yellow on it, and a chat channel
/// opens and closes its elicitation queue on it, so the two cannot disagree about a gate.
pub fn awaits_operator(mode_changed: &ModeChanged) -> bool {
    let Some(variant) = mode_changed
        .mode
        .as_ref()
        .and_then(|mode| mode.variant.as_ref())
    else {
        return false;
    };
    match variant {
        Variant::DocumentReview(_)
        | Variant::MarkdownViewer(_)
        | Variant::FeatureInput(_)
        | Variant::Select(_)
        | Variant::MultiSelect(_)
        | Variant::TextInput(_) => true,
        Variant::Running(_) | Variant::Done(_) => false,
    }
}
