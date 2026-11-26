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

/// The device screen inside a Simulator.app window.
///
/// The window's title bar is cropped off; everything below it is the device. `scale` is the
/// display's backing scale (2.0 on Retina), so the streamed frame is `device_area × scale` pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceViewport {
    device_area: Rect,
    scale: f64,
}

impl DeviceViewport {
    /// The viewport of a window whose frame is `window` (screen points), with a title bar
    /// `title_bar` points tall, on a display of backing `scale`.
    pub fn within_window(window: Rect, title_bar: f64, scale: f64) -> Self {
        let _ = (window, title_bar, scale);
        todo!("DeviceViewport::within_window")
    }

    /// The device area in screen points.
    pub fn device_area(&self) -> Rect {
        self.device_area
    }

    /// The streamed frame's size in pixels.
    pub fn pixel_size(&self) -> (u32, u32) {
        let _ = self.scale;
        todo!("DeviceViewport::pixel_size")
    }

    /// The pixel rows of a captured window frame that belong to the title bar, cropped off.
    pub fn title_bar_rows(&self, window_frame_height: u32) -> u32 {
        let _ = window_frame_height;
        todo!("DeviceViewport::title_bar_rows")
    }

    /// Where device pixel `(x, y)` of the streamed frame lies on screen.
    pub fn to_screen(&self, x: u32, y: u32) -> ScreenPoint {
        let _ = (x, y);
        todo!("DeviceViewport::to_screen")
    }
}
