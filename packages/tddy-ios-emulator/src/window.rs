//! The window port: the Simulator.app window showing a device, and its pixels.

use crate::geometry::Rect;

/// A Simulator.app window, located on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct SimulatorWindow {
    pub id: u32,
    /// The process that owns the window.
    pub owner_pid: u32,
    pub title: String,
    /// The window frame in screen points, title bar included.
    pub bounds: Rect,
    /// Where the device screen is shown, in screen points: inside `bounds`.
    pub device_area: Rect,
    /// Backing scale of the display the window is on.
    pub scale: f64,
}

/// One captured RGBA image (`width × height × 4` bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub trait SimulatorWindows: Send + Sync {
    /// The window showing the device called `device_name`; `None` when Simulator.app shows none.
    fn find(&self, device_name: &str) -> anyhow::Result<Option<SimulatorWindow>>;

    /// Capture the whole window, everything around the device included, in pixels.
    fn capture(&self, window: &SimulatorWindow) -> anyhow::Result<Frame>;
}
