//! A click on a notification the desktop showed, back to the session it is about
//! (`#home-inbox` 11/11).
//!
//! `tauri-plugin-notification` 2.5 and later report a click on a desktop notification to
//! `Notification::on_action` as action [`TAP_ACTION_ID`], carrying the `extra` payload the
//! notification was shown with. The desktop shows each notice with its route as that payload
//! ([`SessionRoute::tap_extra`]); a tap maps it back ([`session_route_from_tap`]) and opens it
//! ([`on_notification_tap`] → [`super::open_session_route`]).
//!
//! The plugin's types stay in `tauri_host`: what is here is plain data, so the mapping and the
//! handler run in tests against the fake window.

use std::collections::HashMap;

use super::{DashboardWindow, SessionRoute};

/// The action id the plugin reports for a click on the notification itself.
pub const TAP_ACTION_ID: &str = "tap";

/// The `extra` key a notice's session id is shown under.
pub const EXTRA_SESSION_ID: &str = "tddy_session_id";

/// The `extra` key a notice's host — the daemon instance the session lives on — is shown under.
pub const EXTRA_HOST: &str = "tddy_host";

/// An action the operator performed on a notification, as the plugin reports it: which action, and
/// the string values of the `extra` payload the notification was shown with (empty when the
/// platform reported no notification).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TappedNotification {
    pub action_id: String,
    pub extra: HashMap<String, String>,
}

impl SessionRoute {
    /// The `extra` payload a notice for this route is shown with, so a tap on it can name it again.
    pub fn tap_extra(&self) -> HashMap<String, String> {
        HashMap::from([
            (EXTRA_SESSION_ID.to_string(), self.session_id.clone()),
            (EXTRA_HOST.to_string(), self.host.clone()),
        ])
    }
}

/// The session `tap` opens: the route its payload names, when it is a click on the notification
/// itself ([`TAP_ACTION_ID`]) on a notice this desktop showed — one carrying both
/// [`EXTRA_SESSION_ID`] and [`EXTRA_HOST`]. Any other action, or a payload that names no session,
/// opens nothing.
pub fn session_route_from_tap(tap: &TappedNotification) -> Option<SessionRoute> {
    if tap.action_id != TAP_ACTION_ID {
        return None;
    }
    let named = |key: &str| tap.extra.get(key).filter(|value| !value.is_empty());
    Some(SessionRoute {
        session_id: named(EXTRA_SESSION_ID)?.clone(),
        host: named(EXTRA_HOST)?.clone(),
    })
}

/// The tap handler: open the session `tap` names in `window` ([`super::open_session_route`]); a tap
/// that names none is ignored.
pub fn on_notification_tap(
    window: &dyn DashboardWindow,
    tap: &TappedNotification,
) -> anyhow::Result<()> {
    match session_route_from_tap(tap) {
        Some(route) => super::open_session_route(window, &route),
        None => Ok(()),
    }
}
