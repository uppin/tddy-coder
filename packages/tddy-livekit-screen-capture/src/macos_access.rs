//! Screen Recording permission on macOS (`CGRequestScreenCaptureAccess` / `CGPreflightScreenCaptureAccess`).

#[cfg(target_os = "macos")]
mod imp {
    use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

    /// Ask the system to grant Screen Recording (may show a prompt). Safe to call repeatedly.
    pub fn request_screen_capture_access() {
        CGRequestScreenCaptureAccess();
    }

    pub fn screen_capture_granted() -> bool {
        CGPreflightScreenCaptureAccess()
    }

    /// After listing or before capture: explain when TCC still denies the **running app** (Terminal/Cursor vs raw binary).
    pub fn warn_if_screen_capture_denied() {
        if screen_capture_granted() {
            return;
        }
        log::warn!(
            "Screen Recording is not active for this process. The window list stays nearly empty until macOS grants access, \
             because the system reports most windows with sharing state \"none\" without it. \
             Enable Screen Recording for the app that is actually running this process — usually Terminal.app, iTerm, or Cursor — \
             under System Settings → Privacy & Security → Screen Recording. \
             Adding only the path to target/debug/tddy-livekit-screen-capture is often not enough when you launch from an IDE terminal. \
             After changing the checkbox, fully quit and reopen that host app, then run this command again."
        );
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn request_screen_capture_access() {}

    pub fn screen_capture_granted() -> bool {
        true
    }

    pub fn warn_if_screen_capture_denied() {}
}

pub use imp::{request_screen_capture_access, warn_if_screen_capture_denied};
