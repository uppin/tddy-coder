//! [`IosSimulatorClient`]: a booted simulator as a [`ScreenSharingClient`].

use std::sync::Arc;

use anyhow::Context as _;
use tddy_screenshare::{AdvertisedMethod, ConnectTarget, Credentials, ScreenSharingClient};

use crate::error::SimulatorError;
use crate::geometry::{DeviceViewport, ScreenPoint};
use crate::input::{key_for_keysym, InputEvent, InputSink, KeyStroke};
use crate::simctl::{DeviceState, SimDevice, Simctl};
use crate::window::{Frame, SimulatorWindow, SimulatorWindows};

/// RFB pointer mask bit for the left button: a touch on the device.
const LEFT_BUTTON: u32 = 1;

/// How many moves a scroll's swipe is made of, between its touch-down and its lift.
const SWIPE_STEPS: u32 = 4;

/// The framebuffer points a swipe against a scroll of `delta` from `start` passes through: the
/// touch-down at `start`, `steps` moves, and the lift at `start - delta`, kept on the `size` screen.
pub fn swipe_path(
    start: (u32, u32),
    delta: (i32, i32),
    size: (u32, u32),
    steps: u32,
) -> Vec<(u32, u32)> {
    let clamp = |value: f64, limit: u32| {
        value.round().clamp(0.0, f64::from(limit.saturating_sub(1))) as u32
    };
    let (sx, sy) = (f64::from(start.0), f64::from(start.1));
    let (ex, ey) = (sx - f64::from(delta.0), sy - f64::from(delta.1));
    (0..=steps + 1)
        .map(|i| {
            let f = f64::from(i) / f64::from(steps + 1);
            (
                clamp(sx + (ex - sx) * f, size.0),
                clamp(sy + (ey - sy) * f, size.1),
            )
        })
        .collect()
}

/// The three ports a simulator client runs over.
#[derive(Clone)]
pub struct SimulatorPorts {
    pub simctl: Arc<dyn Simctl>,
    pub windows: Arc<dyn SimulatorWindows>,
    pub input: Arc<dyn InputSink>,
}

/// A booted iOS Simulator's device screen, streamed as frames, with viewer input posted back.
///
/// The frame is pulled: each [`ScreenSharingClient::poll_events`] captures the window once.
pub struct IosSimulatorClient {
    ports: SimulatorPorts,
    window: SimulatorWindow,
    viewport: DeviceViewport,
    width: u32,
    height: u32,
    frame: Vec<u8>,
    /// Whether a frame has been captured yet: the first capture is always a change.
    captured: bool,
    left_held: bool,
    pointer: Option<(u32, u32)>,
}

impl IosSimulatorClient {
    /// Connect to the booted device `udid` through `ports`.
    ///
    /// Fails with a [`crate::SimulatorError`] when the device is unknown, not booted, or has no
    /// window on screen.
    pub async fn connect_with(ports: SimulatorPorts, udid: &str) -> anyhow::Result<Self> {
        let device = booted_device(ports.simctl.as_ref(), udid).await?;
        Self::connect_to_device(ports, &device)
    }

    /// Connect to `device`, already resolved and known to be booted.
    fn connect_to_device(ports: SimulatorPorts, device: &SimDevice) -> anyhow::Result<Self> {
        let window = device_window(ports.windows.as_ref(), device)?;
        Ok(Self::showing(ports, window))
    }

    /// A client streaming `window`.
    fn showing(ports: SimulatorPorts, window: SimulatorWindow) -> Self {
        let viewport = DeviceViewport::new(window.bounds, window.device_area, window.scale);
        let (width, height) = viewport.pixel_size();
        Self {
            ports,
            window,
            viewport,
            width,
            height,
            frame: vec![0; frame_len(width, height)],
            captured: false,
            left_held: false,
            pointer: None,
        }
    }

    /// The device area of a captured window frame, sized to the streamed frame: everything around
    /// the device is dropped, and a capture that came back a pixel off the expected size is cropped
    /// or padded.
    fn device_pixels(&self, window_frame: &Frame) -> Vec<u8> {
        let (left, top) = self.viewport.crop_offset();
        let mut pixels = vec![0; frame_len(self.width, self.height)];
        let row_bytes = self.width.min(window_frame.width.saturating_sub(left)) as usize * 4;
        let rows = self.height.min(window_frame.height.saturating_sub(top));
        for row in 0..rows {
            let source = (((top + row) * window_frame.width) + left) as usize * 4;
            let target = (row * self.width) as usize * 4;
            pixels[target..target + row_bytes]
                .copy_from_slice(&window_frame.rgba[source..source + row_bytes]);
        }
        pixels
    }

    fn post(&self, event: InputEvent) -> anyhow::Result<()> {
        self.ports.input.post(event)
    }
}

/// The window showing `device`, failing when there is none.
fn device_window(
    windows: &dyn SimulatorWindows,
    device: &SimDevice,
) -> anyhow::Result<SimulatorWindow> {
    Ok(windows
        .find(&device.name)
        .with_context(|| format!("finding the Simulator.app window for {}", device.udid))?
        .ok_or_else(|| SimulatorError::WindowNotFound {
            udid: device.udid.clone(),
            name: device.name.clone(),
        })?)
}

/// The device `udid` as simctl lists it, failing unless it is booted.
async fn booted_device(simctl: &dyn Simctl, udid: &str) -> anyhow::Result<SimDevice> {
    let device = simctl
        .list_devices()
        .await
        .with_context(|| format!("listing simulators to find {udid}"))?
        .into_iter()
        .find(|device| device.udid == udid)
        .ok_or_else(|| SimulatorError::DeviceNotFound { udid: udid.into() })?;
    if device.state != DeviceState::Booted {
        return Err(SimulatorError::DeviceNotBooted {
            udid: device.udid,
            state: device.state,
        }
        .into());
    }
    Ok(device)
}

fn frame_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

#[async_trait::async_trait]
impl ScreenSharingClient for IosSimulatorClient {
    /// Connect to the device `target.device` names, through the production ports.
    async fn connect(target: &ConnectTarget, _credentials: &Credentials) -> anyhow::Result<Self> {
        let udid = target.device.as_deref().ok_or(SimulatorError::NoDevice)?;
        production::connect(udid).await
    }

    /// A simulator asks for no sign-in.
    async fn inspect(_target: &ConnectTarget) -> anyhow::Result<Vec<AdvertisedMethod>> {
        Ok(vec![])
    }

    fn framebuffer_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn get_rgba_frame(&self) -> &[u8] {
        &self.frame
    }

    /// Capture one frame; `true` when it differs from the previous one.
    async fn poll_events(&mut self) -> anyhow::Result<bool> {
        let window_frame = self
            .ports
            .windows
            .capture(&self.window)
            .context("capturing the Simulator.app window")?;
        let pixels = self.device_pixels(&window_frame);
        let changed = !self.captured || pixels != self.frame;
        self.captured = true;
        self.frame = pixels;
        Ok(changed)
    }

    /// Frames are pulled by `poll_events`, so there is nothing to request.
    async fn request_frame_update(&mut self, _incremental: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn inject_pointer(&mut self, x: u32, y: u32, button_mask: u32) -> anyhow::Result<()> {
        log::debug!("ios-emulator: pointer ({x}, {y}) mask {button_mask:#b}");
        let point: ScreenPoint = self.viewport.to_screen(x, y);
        let held = button_mask & LEFT_BUTTON != 0;
        let moved = self.pointer != Some((x, y));
        let event = match (self.left_held, held) {
            (false, true) => Some(InputEvent::MouseDown(point)),
            (true, true) => moved.then_some(InputEvent::MouseDrag(point)),
            (true, false) => Some(InputEvent::MouseUp(point)),
            (false, false) => moved.then_some(InputEvent::MouseMove(point)),
        };
        self.left_held = held;
        self.pointer = Some((x, y));
        match event {
            Some(event) => self.post(event),
            None => Ok(()),
        }
    }

    async fn inject_key(&mut self, keysym: u32, pressed: bool) -> anyhow::Result<()> {
        log::debug!("ios-emulator: key {keysym:#x} pressed={pressed}");
        match key_for_keysym(keysym) {
            Some(KeyStroke::Text(c)) if pressed => self.post(InputEvent::Text(c.to_string())),
            Some(KeyStroke::Text(_)) => Ok(()),
            Some(KeyStroke::Special(key)) => self.post(InputEvent::Key { key, pressed }),
            None => Ok(()),
        }
    }

    /// A scroll is a swipe: the finger moves against the scroll, from `(x, y)` by `-delta`, as a
    /// thumb pushes content up to scroll down. Ignored while a touch is already down, which a swipe
    /// would end.
    async fn inject_wheel(
        &mut self,
        x: u32,
        y: u32,
        delta_x: i32,
        delta_y: i32,
    ) -> anyhow::Result<()> {
        if self.left_held || (delta_x == 0 && delta_y == 0) {
            return Ok(());
        }
        let path = swipe_path(
            (x, y),
            (delta_x, delta_y),
            (self.width, self.height),
            SWIPE_STEPS,
        );
        let last = path.len() - 1;
        for (index, (px, py)) in path.into_iter().enumerate() {
            let point = self.viewport.to_screen(px, py);
            let event = match index {
                0 => InputEvent::MouseDown(point),
                i if i == last => InputEvent::MouseUp(point),
                _ => InputEvent::MouseDrag(point),
            };
            self.post(event)?;
        }
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod production {
    use std::sync::Arc;

    use super::{booted_device, device_window, IosSimulatorClient, SimulatorPorts};
    use crate::dtuhid_xpc::DtuhidInput;
    use crate::macos::{require_permissions, XcapSimulatorWindows};
    use crate::simctl::XcrunSimctl;

    /// Connect through `xcrun simctl`, xcap and the device's `dtuhidd`. The device is resolved before the
    /// permissions are checked, so an unknown udid is reported as such whatever is granted.
    ///
    /// Input goes to the device's own `dtuhidd`, so it reaches the device whatever covers its window.
    pub(super) async fn connect(udid: &str) -> anyhow::Result<IosSimulatorClient> {
        let simctl = XcrunSimctl::default();
        let device = booted_device(&simctl, udid).await?;
        require_permissions()?;
        let windows = XcapSimulatorWindows;
        let window = device_window(&windows, &device)?;
        let ports = SimulatorPorts {
            simctl: Arc::new(simctl),
            windows: Arc::new(windows),
            input: Arc::new(DtuhidInput::connect(udid, window.device_area)?),
        };
        Ok(IosSimulatorClient::showing(ports, window))
    }
}

#[cfg(not(target_os = "macos"))]
mod production {
    use super::IosSimulatorClient;
    use crate::error::SimulatorError;

    /// A simulator runs only on macOS, so this build cannot bridge one.
    pub(super) async fn connect(udid: &str) -> anyhow::Result<IosSimulatorClient> {
        Err(SimulatorError::Unsupported { udid: udid.into() }.into())
    }
}
