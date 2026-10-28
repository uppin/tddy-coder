//! The desktop under test, shared by the native-notification suites: builders for what the daemon
//! publishes, the two ports' in-memory fakes, and the fluent assertions over what was shown.

#![allow(dead_code)] // each suite uses its own part of this module

use std::sync::{Arc, Mutex};

use tauri::Url;
use tddy_daemon_kernel::session_notification::{
    AgentNotification, NotificationLevel, QuestionNotice,
};
use tddy_desktop::native_notifications::{
    DashboardWindow, DesktopContext, DesktopNotice, DesktopNotificationSubscriber, NativeNotifier,
    SessionRoute,
};
use tddy_session_lifecycle::session_notifications::{
    notification_for_activity_status, SessionNotification, SessionNotificationBus,
    SessionNotificationKind, SessionNotificationSource,
};

/// The OS user this desktop runs as: the only operator whose sessions it shows.
pub const THE_OPERATOR: &str = "operator";
/// The desktop's own daemon instance, which the session route names as `?host=`.
pub const THIS_DESKTOP: &str = "desktop-7";
pub const THE_SESSION: &str = "01900000-0000-7000-8000-AABB00000001";
pub const ANOTHER_SESSION: &str = "01900000-0000-7000-8000-AABB00000002";
pub const THE_LABEL: &str = "api-gateway";
pub const A_MOMENT: u64 = 1_761_400_000_000;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The route a click on `session_id`'s notification opens, on this desktop.
pub fn route_to(session_id: &str) -> SessionRoute {
    SessionRoute {
        session_id: session_id.to_string(),
        host: THIS_DESKTOP.to_string(),
    }
}

/// What the operator sees for a daemon-worded notification: the fixed text, under a title that
/// names the session as needing them.
pub fn a_needs_you_notice_saying(text: &str) -> DesktopNotice {
    DesktopNotice {
        title: format!("{THE_LABEL} needs you"),
        subtitle: THE_LABEL.to_string(),
        body: text.to_string(),
        sound: false,
        route: route_to(THE_SESSION),
    }
}

/// The notification a claude-cli hook's status report raises for [`THE_SESSION`], built by the
/// daemon's own builder so the text under test is the text the daemon really sends.
pub fn a_status_report(status: &str) -> SessionNotification {
    a_status_report_for(THE_SESSION, status)
}

pub fn a_status_report_for(session_id: &str, status: &str) -> SessionNotification {
    notification_for_activity_status(session_id, THE_OPERATOR, THE_LABEL, status, A_MOMENT)
        .unwrap_or_else(|| panic!("the daemon raises no notification for status {status}"))
}

pub fn an_agent_notification() -> AgentNotificationBuilder {
    AgentNotificationBuilder {
        title: "Done".to_string(),
        body_markdown: String::new(),
        sound: false,
    }
}

pub struct AgentNotificationBuilder {
    title: String,
    body_markdown: String,
    sound: bool,
}

impl AgentNotificationBuilder {
    pub fn titled(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn with_body_markdown(mut self, body_markdown: &str) -> Self {
        self.body_markdown = body_markdown.to_string();
        self
    }

    /// The agent asked for an audible alert (`tddy-tools notify --sound`).
    pub fn asking_for_a_sound(mut self) -> Self {
        self.sound = true;
        self
    }

    /// What `ReportOperatorNotification` publishes for [`THE_SESSION`]: an attention notification
    /// of source `Agent`, its `text` the flattened line Telegram would send verbatim.
    pub fn built(self) -> SessionNotification {
        SessionNotification {
            session_id: THE_SESSION.to_string(),
            os_user: THE_OPERATOR.to_string(),
            label: THE_LABEL.to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::Agent,
            text: format!("{}: {}", self.title, self.body_markdown),
            agent: Some(AgentNotification {
                title: self.title,
                body_markdown: self.body_markdown,
                level: NotificationLevel::Info,
                sound: self.sound,
                link: format!("#/sessions/{THE_SESSION}?host={THIS_DESKTOP}"),
            }),
            question: None,
            at_unix_ms: A_MOMENT,
        }
    }
}

/// What a claude-cli session raises when it asks the operator a question: an attention
/// notification of source `PendingQuestion`, worded by the daemon like any other.
pub fn a_pending_question_notification() -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::PendingQuestion,
        text: format!("❓ Session {THE_LABEL}: asks — Run `cargo test`?"),
        question: Some(QuestionNotice::default()),
        ..a_status_report("WaitingForInput")
    }
}

/// What a desktop knows about itself, as the subscriber is built with it.
pub fn this_desktops_context() -> DesktopContext {
    DesktopContext {
        os_user: THE_OPERATOR.to_string(),
        daemon_instance_id: THIS_DESKTOP.to_string(),
    }
}

pub trait NotificationOwnedBy {
    fn owned_by(self, os_user: &str) -> Self;
}

impl NotificationOwnedBy for SessionNotification {
    fn owned_by(mut self, os_user: &str) -> Self {
        self.os_user = os_user.to_string();
        self
    }
}

// ---------------------------------------------------------------------------
// The desktop under test
// ---------------------------------------------------------------------------

pub fn a_desktop() -> DesktopBuilder {
    DesktopBuilder {
        window: a_dashboard_window_at("tauri://localhost/"),
    }
}

pub struct DesktopBuilder {
    window: FakeDashboardWindow,
}

impl DesktopBuilder {
    /// The operator is looking at `session_id`: the window has focus and its route names it.
    pub fn with_a_focused_window_showing(mut self, session_id: &str) -> Self {
        self.window = a_dashboard_window_at(&format!(
            "tauri://localhost/#/sessions/{session_id}?host={THIS_DESKTOP}"
        ))
        .focused();
        self
    }

    /// The window has the operator's page on `session_id` but another application has focus.
    pub fn with_an_unfocused_window_showing(mut self, session_id: &str) -> Self {
        self.window = a_dashboard_window_at(&format!(
            "tauri://localhost/#/sessions/{session_id}?host={THIS_DESKTOP}"
        ));
        self
    }

    /// The real subscriber, registered on a real bus, over an in-memory notifier.
    pub fn start(self) -> Desktop {
        let notifier = Arc::new(InMemoryNotifier::default());
        let subscriber = DesktopNotificationSubscriber::new(
            this_desktops_context(),
            Arc::clone(&notifier) as Arc<dyn NativeNotifier>,
            Arc::new(self.window) as Arc<dyn DashboardWindow>,
        );
        Desktop {
            bus: SessionNotificationBus::new().with_subscriber(Arc::new(subscriber)),
            notifier,
        }
    }
}

pub struct Desktop {
    bus: SessionNotificationBus,
    notifier: Arc<InMemoryNotifier>,
}

impl Desktop {
    pub async fn publish(&self, notification: SessionNotification) {
        self.bus.publish(notification).await;
    }

    pub fn shown(&self) -> Vec<DesktopNotice> {
        self.notifier.shown.lock().expect("the notifier").clone()
    }
}

/// The notification centre, in memory: it keeps every notice it was asked to show.
#[derive(Default)]
pub struct InMemoryNotifier {
    shown: Mutex<Vec<DesktopNotice>>,
}

impl NativeNotifier for InMemoryNotifier {
    fn show(&self, notice: &DesktopNotice) -> anyhow::Result<()> {
        self.shown
            .lock()
            .expect("the notifier")
            .push(notice.clone());
        Ok(())
    }
}

/// What was done to the dashboard window, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowEvent {
    BroughtToFront,
    Navigated(String),
}

/// The dashboard window, in memory: where it is, whether it has focus, and what was done to it.
pub struct FakeDashboardWindow {
    focused: bool,
    location: Url,
    events: Mutex<Vec<WindowEvent>>,
}

pub fn a_dashboard_window_at(location: &str) -> FakeDashboardWindow {
    FakeDashboardWindow {
        focused: false,
        location: Url::parse(location).expect("a test url"),
        events: Mutex::new(Vec::new()),
    }
}

impl FakeDashboardWindow {
    pub fn focused(mut self) -> Self {
        self.focused = true;
        self
    }

    pub fn assert_events(&self, expected: Vec<WindowEvent>) {
        assert_eq!(
            *self.events.lock().expect("the window"),
            expected,
            "what was done to the window"
        );
    }
}

impl DashboardWindow for FakeDashboardWindow {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn location(&self) -> Option<Url> {
        Some(self.location.clone())
    }

    fn bring_to_front(&self) -> anyhow::Result<()> {
        self.events
            .lock()
            .expect("the window")
            .push(WindowEvent::BroughtToFront);
        Ok(())
    }

    fn navigate(&self, url: Url) -> anyhow::Result<()> {
        self.events
            .lock()
            .expect("the window")
            .push(WindowEvent::Navigated(url.to_string()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

pub trait ShownNotices {
    /// The native notifications shown, whole and in order.
    fn assert_exactly(&self, expected: Vec<DesktopNotice>);

    /// The sessions the shown notifications open, in order.
    fn assert_sessions(&self, expected: &[&str]);
}

impl ShownNotices for Vec<DesktopNotice> {
    fn assert_exactly(&self, expected: Vec<DesktopNotice>) {
        assert_eq!(*self, expected, "the native notifications shown");
    }

    fn assert_sessions(&self, expected: &[&str]) {
        let shown: Vec<&str> = self
            .iter()
            .map(|notice| notice.route.session_id.as_str())
            .collect();
        assert_eq!(shown, expected, "the sessions shown natively");
    }
}
