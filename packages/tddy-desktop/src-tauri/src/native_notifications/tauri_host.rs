//! The two ports of [`super`], over Tauri: the operating system's notification centre through
//! `tauri-plugin-notification`, and the dashboard window through the application handle.
//!
//! Neither is exercised by a test — both need a running Tauri application. What they carry is
//! verified by hand (the changeset's manual verification); what decides *whether* and *what* to
//! show is tested against the ports' fakes in `tests/native_notifications_acceptance.rs`.

use tauri::{AppHandle, Url};

use super::{DashboardWindow, DesktopNotice, NativeNotifier};

/// Shows a [`DesktopNotice`] through `tauri-plugin-notification`, with the label folded into the body
/// ([`DesktopNotice::body_with_label`]) because the plugin's desktop notification has no subtitle.
///
/// The plugin's desktop permission API always answers "granted" without asking, so a refusal by
/// the operating system is not observable here; macOS asks on the first notification and silently
/// drops it when refused.
pub struct TauriNotifier {
    // TODO(desktop-notify): read by `show`
    #[allow(dead_code)]
    app: AppHandle,
}

impl TauriNotifier {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl NativeNotifier for TauriNotifier {
    fn show(&self, notice: &DesktopNotice) -> anyhow::Result<()> {
        // TODO(desktop-notify): implement
        let _ = notice;
        Err(anyhow::anyhow!(
            "TODO(desktop-notify): TauriNotifier::show is not implemented"
        ))
    }
}

/// The dashboard window, looked up by label on every call: it is created after the daemon — and so
/// after the subscriber that holds this — and may be closed and reopened.
pub struct TauriDashboardWindow {
    // TODO(desktop-notify): read by every method
    #[allow(dead_code)]
    app: AppHandle,
}

impl TauriDashboardWindow {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl DashboardWindow for TauriDashboardWindow {
    fn is_focused(&self) -> bool {
        // TODO(desktop-notify): implement
        false
    }

    fn location(&self) -> Option<Url> {
        // TODO(desktop-notify): implement
        None
    }

    fn bring_to_front(&self) -> anyhow::Result<()> {
        // TODO(desktop-notify): implement
        Err(anyhow::anyhow!(
            "TODO(desktop-notify): TauriDashboardWindow::bring_to_front is not implemented"
        ))
    }

    fn navigate(&self, url: Url) -> anyhow::Result<()> {
        // TODO(desktop-notify): implement
        let _ = url;
        Err(anyhow::anyhow!(
            "TODO(desktop-notify): TauriDashboardWindow::navigate is not implemented"
        ))
    }
}
