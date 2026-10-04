//! How this daemon names and publishes its sessions' notifications.
//!
//! The notification domain itself — the bus, the event, the subscriber trait and the three
//! builders — moved to [`tddy_session_activity::session_notifications`] with `#unbundle` node 7,
//! and is re-exported below so a call site names one module rather than two.
//!
//! What could not go is here: a session's display label is read from
//! [`crate::session_list_enrichment`], which serves `ListSessions` — family C, which stays in the
//! daemon deliberately. [`SessionNotificationPublishing`] is the publish context built on that
//! label, so it stays with it.

pub use tddy_session_activity::session_notifications::*;

pub(crate) mod session_notification_publishing;
pub use session_notification_publishing::*;
