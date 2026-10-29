//! The two ports of [`super`], over Tauri: the operating system's notification centre through
//! `tauri-plugin-notification`, and the dashboard window through the application handle.
//!
//! Neither is exercised by a test — both need a running Tauri application. What they carry is
//! verified by hand (PR #769's manual verification); what decides *whether* and *what* to
//! show is tested against the ports' fakes in `tests/native_notifications_acceptance.rs`.

use tauri::{AppHandle, Manager, Url, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

use super::{
    on_notification_tap, DashboardWindow, DesktopNotice, NativeNotifier, TappedNotification,
};
use crate::MAIN_WINDOW_LABEL;

/// Shows a [`DesktopNotice`] through `tauri-plugin-notification`, with the label folded into the body
/// ([`DesktopNotice::body_with_label`]) because the plugin's desktop notification has no subtitle.
///
/// The plugin's desktop permission API always answers "granted" without asking, so a refusal by
/// the operating system is not observable here; macOS asks on the first notification and silently
/// drops it when refused. The plugin also shows the notification on a task of its own and drops that
/// task's error, so only a failure before the task starts reaches the caller.
pub struct TauriNotifier {
    app: AppHandle,
}

impl TauriNotifier {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl NativeNotifier for TauriNotifier {
    fn show(&self, notice: &DesktopNotice) -> anyhow::Result<()> {
        let mut builder = self
            .app
            .notification()
            .builder()
            .title(&notice.title)
            .body(notice.body_with_label());
        // The route rides along as the payload, so a tap on the notice can name its session again.
        for (key, value) in notice.route.tap_extra() {
            builder = builder.extra(key, value);
        }
        if notice.sound {
            // The operating system's own default alert, the one an agent asks for by name.
            builder = builder.sound("default");
        }
        builder.show()?;
        Ok(())
    }
}

/// Open the session a click on a notification this desktop showed is about: register the plugin's
/// action handler, which maps the click back to its route and opens it in the dashboard window
/// (`#home-inbox` 11/11). The plugin runs the handler on a background thread; a failure to open the
/// session is logged, as there is no caller to return it to.
pub fn register_tap_handler(app: &AppHandle) -> anyhow::Result<()> {
    let window = TauriDashboardWindow::new(app.clone());
    app.notification().on_action(move |performed| {
        let tap = TappedNotification {
            action_id: performed.action_id().to_string(),
            extra: performed
                .notification()
                .map(|notification| {
                    notification
                        .extra()
                        .iter()
                        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        };
        if let Err(e) = on_notification_tap(&window, &tap) {
            log::warn!("[tddy-desktop] a notification click could not open its session: {e:#}");
        }
    })?;
    Ok(())
}

/// The dashboard window, looked up by label on every call: it is created after the daemon — and so
/// after the subscriber that holds this — and may be closed and reopened.
pub struct TauriDashboardWindow {
    app: AppHandle,
}

impl TauriDashboardWindow {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// The dashboard window, if it is open.
    fn window(&self) -> Option<WebviewWindow> {
        self.app.get_webview_window(MAIN_WINDOW_LABEL)
    }

    fn open_window(&self) -> anyhow::Result<WebviewWindow> {
        self.window()
            .ok_or_else(|| anyhow::anyhow!("the dashboard window is not open"))
    }
}

impl DashboardWindow for TauriDashboardWindow {
    fn is_focused(&self) -> bool {
        // A window whose focus cannot be read is taken as not focused, so the operator is told.
        self.window()
            .is_some_and(|window| window.is_focused().unwrap_or(false))
    }

    fn location(&self) -> Option<Url> {
        self.window()?.url().ok()
    }

    fn bring_to_front(&self) -> anyhow::Result<()> {
        let window = self.open_window()?;
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        Ok(())
    }

    fn navigate(&self, url: Url) -> anyhow::Result<()> {
        self.open_window()?.navigate(url)?;
        Ok(())
    }
}
