//! Where the device screen sits inside its Simulator.app window, in screen points and in pixels.

/// A rectangle in macOS global screen points (origin top-left of the main display).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A point in macOS global screen points: where a synthetic mouse event is posted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

/// The device screen inside the window that shows it.
///
/// Everything in the window outside the device area — a title bar, a sidebar, a toolbar — is
/// cropped off. `scale` is the display's backing scale (2.0 on Retina), so the streamed frame is
/// `device_area × scale` pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceViewport {
    window: Rect,
    device_area: Rect,
    scale: f64,
}

impl DeviceViewport {
    /// The viewport of a device shown at `device_area` inside a window whose frame is `window`
    /// (both in screen points), on a display of backing `scale`.
    pub fn new(window: Rect, device_area: Rect, scale: f64) -> Self {
        Self {
            window,
            device_area,
            scale,
        }
    }

    /// The viewport of a window whose device is everything below a title bar `title_bar` points
    /// tall — Simulator.app's layout.
    pub fn within_window(window: Rect, title_bar: f64, scale: f64) -> Self {
        Self::new(window, below_title_bar(window, title_bar), scale)
    }

    /// The device area in screen points.
    pub fn device_area(&self) -> Rect {
        self.device_area
    }

    /// The streamed frame's size in pixels.
    pub fn pixel_size(&self) -> (u32, u32) {
        (
            (self.device_area.width * self.scale).round() as u32,
            (self.device_area.height * self.scale).round() as u32,
        )
    }

    /// Where the device area starts within a captured window frame, in pixels: everything to its
    /// left and above it is cropped off.
    pub fn crop_offset(&self) -> (u32, u32) {
        (
            ((self.device_area.x - self.window.x) * self.scale).round() as u32,
            ((self.device_area.y - self.window.y) * self.scale).round() as u32,
        )
    }

    /// Where device pixel `(x, y)` of the streamed frame lies on screen.
    pub fn to_screen(&self, x: u32, y: u32) -> ScreenPoint {
        ScreenPoint {
            x: self.device_area.x + f64::from(x) / self.scale,
            y: self.device_area.y + f64::from(y) / self.scale,
        }
    }
}

/// The part of `window` below a title bar `title_bar` points tall.
pub fn below_title_bar(window: Rect, title_bar: f64) -> Rect {
    Rect {
        x: window.x,
        y: window.y + title_bar,
        width: window.width,
        height: window.height - title_bar,
    }
}
