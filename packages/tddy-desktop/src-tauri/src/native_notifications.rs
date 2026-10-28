//! Native notifications for Tddy Desktop: tell the operator a session needs them, and open that
//! session when they click.
//!
//! The application's process is a `tddy-daemon`, whose sessions raise notifications onto a bus. The
//! desktop registers a [`DesktopNotificationSubscriber`] on it (`RuntimeOptions::with_notification_subscriber`)
//! and shows what the operator needs to act on through the operating system's notification centre.
//!
//! Everything the operating system does is behind a port, so the behaviour here runs in tests with
//! an in-memory fake and never reaches the real thing:
//!
//! - [`NativeNotifier`] shows one [`DesktopNotice`];
//! - [`DashboardWindow`] is the window a click acts on, and the one whose focus and route say
//!   whether the operator is already looking at a session.
//!
//! [`desktop_notice_for`] decides what a notification becomes; the subscriber adds what only a
//! running desktop knows — which session the focused window shows, and what it said last.
//!
//! PRD: `docs/ft/desktop/1-WIP/PRD-2026-10-25-desktop-notifications.md`.
//!
//! # Clicks
//!
//! **Not delivered.** `tauri-plugin-notification` 2.4 — the last line that builds against the Tauri
//! this application is locked to (2.11) — shows a desktop notification fire-and-forget and reports
//! no click. Plugin 2.5 and later do (`Notification::on_action`, action `tap`) but need Tauri 2.12
//! and Rust 1.90. [`open_session_route`] is the whole click handler and is tested here; *what calls
//! it* is `docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`.
//!
//! # Subtitle and permission
//!
//! The plugin's desktop notification has no subtitle, so [`DesktopNotice::body_with_label`] folds
//! the label into the body. And its permission API always answers "granted" on desktop: macOS
//! asks on the first notification and silently drops it when refused, which nothing here can see.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tauri::Url;
use tddy_daemon_kernel::notification_markdown::to_plain_text;
use tddy_session_lifecycle::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
    SessionNotificationSubscriber,
};

mod tauri_host;

pub use tauri_host::{TauriDashboardWindow, TauriNotifier};

/// How many characters of an agent's body a native notification carries. Operating-system centres
/// truncate further on their own, so more is only wasted.
pub const BODY_LIMIT: usize = 300;

/// What a desktop knows about itself that decides which notifications are its operator's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopContext {
    /// The OS user the desktop runs as: the only operator whose sessions it shows.
    pub os_user: String,
    /// This desktop's own daemon instance, which a session route names as `?host=`.
    pub daemon_instance_id: String,
}

/// A session, addressed the way the dashboard opens one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRoute {
    pub session_id: String,
    /// The daemon instance the session lives on.
    pub host: String,
}

impl SessionRoute {
    /// The dashboard's hash route to this session: `/sessions/<id>?host=<instance>` (without the
    /// `#`).
    pub fn fragment(&self) -> String {
        // Built by `Url` rather than by hand, so the ids are encoded the way the dashboard reads them
        // back: a space in the host is `+` and an `&` is `%26`, as a query string has it.
        let mut addressed = Url::parse("tddy:///").expect("a constant URL parses");
        addressed
            .path_segments_mut()
            .expect("a URL with an empty host has path segments")
            .extend(["sessions", self.session_id.as_str()]);
        addressed.query_pairs_mut().append_pair("host", &self.host);
        format!(
            "{}?{}",
            addressed.path(),
            addressed.query().unwrap_or_default()
        )
    }

    /// `dashboard` — whatever page the dashboard is on, carrying whatever path and query — moved to
    /// this session's route: the same origin, the app's root page and [`Self::fragment`] as the
    /// hash.
    pub fn url_within(&self, dashboard: &Url) -> Url {
        let mut url = dashboard.clone();
        url.set_path("/");
        url.set_query(None);
        url.set_fragment(Some(&self.fragment()));
        url
    }

    /// Whether a dashboard page at `location` is on this session's route. The dashboard writes its
    /// own session routes the same way [`Self::fragment`] does, so an exact match is the session.
    fn is_shown_at(&self, location: &Url) -> bool {
        location.fragment() == Some(self.fragment().as_str())
    }
}

/// One native notification, as the operator sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotice {
    /// The agent's own title, or "`<label>` needs you".
    pub title: String,
    /// The session's label.
    pub subtitle: String,
    /// The agent's body as plain text cut to [`BODY_LIMIT`] characters, or the notification's fixed
    /// text.
    pub body: String,
    /// Whether the operator should hear it: the agent asked for a sound, or the session is asking
    /// a question.
    pub sound: bool,
    /// Where a click on it goes.
    pub route: SessionRoute,
}

impl DesktopNotice {
    /// The body as a notification centre that has no subtitle shows it: the session label as the
    /// first line, then the body — the label alone when the body is empty.
    pub fn body_with_label(&self) -> String {
        if self.body.is_empty() {
            self.subtitle.clone()
        } else {
            format!("{}\n{}", self.subtitle, self.body)
        }
    }
}

/// The operating system's notification centre.
pub trait NativeNotifier: Send + Sync {
    /// Show `notice`. An error means the operator was not told, and is the caller's to report.
    fn show(&self, notice: &DesktopNotice) -> anyhow::Result<()>;
}

/// The dashboard window: what a click acts on, and what says whether the operator is already
/// looking at a session.
pub trait DashboardWindow: Send + Sync {
    /// Whether the window has the operator's focus.
    fn is_focused(&self) -> bool;

    /// The page the window is on, hash included; `None` before the window exists.
    fn location(&self) -> Option<Url>;

    /// Show the window, unminimise it and give it focus.
    fn bring_to_front(&self) -> anyhow::Result<()>;

    /// Load `url` in the window.
    fn navigate(&self, url: Url) -> anyhow::Result<()>;
}

/// What `notification` becomes on this desktop, or `None` when it is not for the operator's
/// attention here: not an attention notification, or another OS user's session.
///
/// Pure — what depends on the running desktop (focus, the previous notice) is the subscriber's.
pub fn desktop_notice_for(
    notification: &SessionNotification,
    context: &DesktopContext,
) -> Option<DesktopNotice> {
    if notification.kind != SessionNotificationKind::AttentionRequired
        || notification.os_user != context.os_user
    {
        return None;
    }
    let pending_question = notification.source == SessionNotificationSource::PendingQuestion;
    let (title, body, agent_asked_for_sound) = match &notification.agent {
        Some(agent) => (
            agent.title.clone(),
            to_plain_text(&agent.body_markdown, BODY_LIMIT),
            agent.sound,
        ),
        None => (
            format!("{} needs you", notification.label),
            notification.text.clone(),
            false,
        ),
    };
    Some(DesktopNotice {
        title,
        subtitle: notification.label.clone(),
        body,
        sound: agent_asked_for_sound || pending_question,
        route: SessionRoute {
            session_id: notification.session_id.clone(),
            host: context.daemon_instance_id.clone(),
        },
    })
}

/// The click handler: bring the dashboard window to the front and open `route` in it.
pub fn open_session_route(
    window: &dyn DashboardWindow,
    route: &SessionRoute,
) -> anyhow::Result<()> {
    let location = window
        .location()
        .ok_or_else(|| anyhow::anyhow!("the dashboard window has no page to open a session in"))?;
    let target = route.url_within(&location);
    // Brought forward before it navigates: a window still hidden would leave the operator looking at
    // whatever it showed before the click.
    window.bring_to_front()?;
    window.navigate(target)
}

/// Shows the daemon's attention notifications for the desktop operator's sessions natively.
pub struct DesktopNotificationSubscriber {
    context: DesktopContext,
    notifier: Arc<dyn NativeNotifier>,
    window: Arc<dyn DashboardWindow>,
    /// The notice last shown per session, so an identical one is not shown twice in a row.
    last_shown: Mutex<HashMap<String, DesktopNotice>>,
}

impl DesktopNotificationSubscriber {
    pub fn new(
        context: DesktopContext,
        notifier: Arc<dyn NativeNotifier>,
        window: Arc<dyn DashboardWindow>,
    ) -> Self {
        Self {
            context,
            notifier,
            window,
            last_shown: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SessionNotificationSubscriber for DesktopNotificationSubscriber {
    fn name(&self) -> &'static str {
        "desktop"
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        desktop_notice_for(notification, &self.context).is_some()
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        let Some(notice) = desktop_notice_for(notification, &self.context) else {
            return Ok(());
        };
        if self.operator_is_looking_at(&notice.route) {
            return Ok(());
        }
        // A poisoned map still holds every notice it recorded, and losing one repeat check costs at
        // most one repeated notification, so the guard is recovered rather than the subscriber dropped.
        let mut last_shown = self
            .last_shown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if last_shown.get(&notice.route.session_id) == Some(&notice) {
            return Ok(());
        }
        self.notifier.show(&notice)?;
        last_shown.insert(notice.route.session_id.clone(), notice);
        Ok(())
    }
}

impl DesktopNotificationSubscriber {
    /// Whether the operator already has `route` in front of them: the dashboard window has focus and
    /// its page is on that session.
    fn operator_is_looking_at(&self, route: &SessionRoute) -> bool {
        self.window.is_focused()
            && self
                .window
                .location()
                .is_some_and(|location| route.is_shown_at(&location))
    }
}
