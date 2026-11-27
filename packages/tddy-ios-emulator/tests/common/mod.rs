//! In-memory ports and builders for `IosSimulatorClient` tests.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tddy_ios_emulator::geometry::below_title_bar;
use tddy_ios_emulator::{
    DeviceState, Frame, InputEvent, InputSink, IosSimulatorClient, Rect, SimDevice, Simctl,
    SimulatorPorts, SimulatorWindow, SimulatorWindows,
};

pub const IPHONE_UDID: &str = "8A1C2E4F-1111-4222-8333-944455556666";
pub const IPHONE_NAME: &str = "iPhone 16 Pro";

/// Title-bar pixels in a test window frame.
pub const TITLE_BAR_PIXEL: [u8; 4] = [200, 200, 200, 255];

/// A small Retina window: 4×5 points with a 1-point title bar at 2× → an 8×10 pixel frame whose
/// top two rows are title bar and whose device area is 8×8 pixels.
pub fn a_small_retina_window() -> SimulatorWindow {
    SimulatorWindow {
        id: 42,
        owner_pid: 4242,
        title: format!("{IPHONE_NAME} – iOS 18.2"),
        bounds: Rect {
            x: 100.0,
            y: 200.0,
            width: 4.0,
            height: 5.0,
        },
        device_area: below_title_bar(
            Rect {
                x: 100.0,
                y: 200.0,
                width: 4.0,
                height: 5.0,
            },
            1.0,
        ),
        scale: 2.0,
    }
}

/// Pixels of a window's chrome around the device: its sidebar and toolbar.
pub const CHROME_PIXEL: [u8; 4] = [30, 30, 30, 255];

/// A small Device Hub-like Retina window: 6×5 points with the device shown at (2, 1), 4×4 points —
/// a 2-point sidebar on its left and a 1-point toolbar above it — so a 12×10 pixel frame whose
/// device area is the 8×8 pixels at (4, 2).
pub fn a_small_retina_window_with_a_sidebar() -> SimulatorWindow {
    SimulatorWindow {
        id: 43,
        owner_pid: 4242,
        title: format!("{IPHONE_NAME} – iOS 18.2"),
        bounds: Rect {
            x: 100.0,
            y: 200.0,
            width: 6.0,
            height: 5.0,
        },
        device_area: Rect {
            x: 102.0,
            y: 201.0,
            width: 4.0,
            height: 4.0,
        },
        scale: 2.0,
    }
}

/// A 12×10 frame of [`a_small_retina_window_with_a_sidebar`]: chrome everywhere but the 8×8 pixels
/// at (4, 2), which are `device_pixel`.
pub fn a_window_frame_with_a_sidebar(device_pixel: [u8; 4]) -> Frame {
    let (width, height) = (12u32, 10u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        for column in 0..width {
            let on_device = (4..12).contains(&column) && (2..10).contains(&row);
            rgba.extend_from_slice(if on_device {
                &device_pixel
            } else {
                &CHROME_PIXEL
            });
        }
    }
    Frame {
        width,
        height,
        rgba,
    }
}

pub fn a_device(udid: &str, state: DeviceState) -> SimDevice {
    SimDevice {
        udid: udid.into(),
        name: IPHONE_NAME.into(),
        runtime: "iOS 18.2".into(),
        state,
    }
}

/// An 8×10 window frame: two title-bar rows, then a device area filled with `device_pixel`.
pub fn a_window_frame(device_pixel: [u8; 4]) -> Frame {
    let (width, height, title_rows) = (8u32, 10u32, 2u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let pixel = if row < title_rows {
            TITLE_BAR_PIXEL
        } else {
            device_pixel
        };
        for _ in 0..width {
            rgba.extend_from_slice(&pixel);
        }
    }
    Frame {
        width,
        height,
        rgba,
    }
}

/// An 8×8 device frame filled with `pixel`: what the client should stream for `a_window_frame`.
pub fn a_device_frame_of(pixel: [u8; 4]) -> Vec<u8> {
    pixel.repeat(8 * 8)
}

pub struct InMemorySimctl {
    devices: Vec<SimDevice>,
}

#[async_trait::async_trait]
impl Simctl for InMemorySimctl {
    async fn list_devices(&self) -> anyhow::Result<Vec<SimDevice>> {
        Ok(self.devices.clone())
    }
}

/// Windows that show one window and hand out scripted frames, repeating the last one.
pub struct ScriptedWindows {
    window: Option<SimulatorWindow>,
    frames: Mutex<VecDeque<Frame>>,
}

impl SimulatorWindows for ScriptedWindows {
    fn find(&self, device_name: &str) -> anyhow::Result<Option<SimulatorWindow>> {
        Ok(self
            .window
            .clone()
            .filter(|w| w.title.starts_with(device_name)))
    }

    fn capture(&self, _window: &SimulatorWindow) -> anyhow::Result<Frame> {
        let mut frames = self.frames.lock().unwrap();
        let frame = if frames.len() > 1 {
            frames.pop_front().unwrap()
        } else {
            frames.front().cloned().expect("no frame scripted")
        };
        Ok(frame)
    }
}

#[derive(Default)]
pub struct RecordingInput {
    events: Mutex<Vec<InputEvent>>,
}

impl RecordingInput {
    pub fn events(&self) -> Vec<InputEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl InputSink for RecordingInput {
    fn post(&self, event: InputEvent) -> anyhow::Result<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

/// A simulator host scene: the devices simctl lists, the window on screen, the frames it shows.
pub struct SimulatorScene {
    devices: Vec<SimDevice>,
    window: Option<SimulatorWindow>,
    frames: Vec<Frame>,
    pub input: Arc<RecordingInput>,
}

pub fn a_simulator_host() -> SimulatorScene {
    SimulatorScene {
        devices: vec![],
        window: None,
        frames: vec![a_window_frame([0, 0, 0, 255])],
        input: Arc::new(RecordingInput::default()),
    }
}

impl SimulatorScene {
    /// A booted iPhone with its window on screen.
    pub fn with_booted_iphone(mut self) -> Self {
        self.devices
            .push(a_device(IPHONE_UDID, DeviceState::Booted));
        self.window = Some(a_small_retina_window());
        self
    }

    /// A booted iPhone shown in `window`.
    pub fn with_booted_iphone_in(mut self, window: SimulatorWindow) -> Self {
        self.devices
            .push(a_device(IPHONE_UDID, DeviceState::Booted));
        self.window = Some(window);
        self
    }

    pub fn with_device(mut self, device: SimDevice) -> Self {
        self.devices.push(device);
        self
    }

    pub fn without_window(mut self) -> Self {
        self.window = None;
        self
    }

    pub fn showing_frames(mut self, frames: Vec<Frame>) -> Self {
        self.frames = frames;
        self
    }

    pub fn ports(&self) -> SimulatorPorts {
        SimulatorPorts {
            simctl: Arc::new(InMemorySimctl {
                devices: self.devices.clone(),
            }),
            windows: Arc::new(ScriptedWindows {
                window: self.window.clone(),
                frames: Mutex::new(self.frames.clone().into()),
            }),
            input: self.input.clone(),
        }
    }

    pub async fn connect(&self, udid: &str) -> anyhow::Result<IosSimulatorClient> {
        IosSimulatorClient::connect_with(self.ports(), udid).await
    }
}
