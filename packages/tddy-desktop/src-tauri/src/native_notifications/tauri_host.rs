//! The two ports of [`super`], over Tauri: the operating system's notification centre through
//! `tauri-plugin-notification`, and the dashboard window through the application handle.
//!
//! Neither is exercised by a test — both need a running Tauri application. What they carry is
//! verified by hand (the changeset's manual verification); what decides *whether* and *what* to
//! show is tested against the ports' fakes in `tests/native_notifications_acceptance.rs`.

use tauri::{AppHandle, Manager, Url, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

use super::{DashboardWindow, DesktopNotice, NativeNotifier};
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
        if notice.sound {
            // The operating system's own default alert, the one an agent asks for by name.
            builder = builder.sound("default");
        }
        builder.show()?;
        Ok(())
    }
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
