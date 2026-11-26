//! [`IosSimulatorClient`]: a booted simulator as a [`ScreenSharingClient`].

use std::sync::Arc;

use tddy_screenshare::{AdvertisedMethod, ConnectTarget, Credentials, ScreenSharingClient};

use crate::input::InputSink;
use crate::simctl::Simctl;
use crate::window::SimulatorWindows;

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
}

impl IosSimulatorClient {
    /// Connect to the booted device `udid` through `ports`.
    ///
    /// Fails with a [`crate::SimulatorError`] when the device is unknown, not booted, or has no
    /// window on screen.
    pub async fn connect_with(ports: SimulatorPorts, udid: &str) -> anyhow::Result<Self> {
        let _ = (udid, &ports.simctl, &ports.windows, &ports.input);
        todo!("IosSimulatorClient::connect_with")
    }
}

#[async_trait::async_trait]
impl ScreenSharingClient for IosSimulatorClient {
    /// Connect to the device `target.device` names, through the production ports.
    async fn connect(target: &ConnectTarget, _credentials: &Credentials) -> anyhow::Result<Self> {
        let _ = target;
        todo!("IosSimulatorClient::connect")
    }

    /// A simulator asks for no sign-in.
    async fn inspect(_target: &ConnectTarget) -> anyhow::Result<Vec<AdvertisedMethod>> {
        todo!("IosSimulatorClient::inspect")
    }

    fn framebuffer_dimensions(&self) -> (u32, u32) {
        let _ = &self.ports;
        todo!("IosSimulatorClient::framebuffer_dimensions")
    }

    fn get_rgba_frame(&self) -> &[u8] {
        todo!("IosSimulatorClient::get_rgba_frame")
    }

    /// Capture one frame; `true` when it differs from the previous one.
    async fn poll_events(&mut self) -> anyhow::Result<bool> {
        todo!("IosSimulatorClient::poll_events")
    }

    /// Frames are pulled by `poll_events`, so there is nothing to request.
    async fn request_frame_update(&mut self, _incremental: bool) -> anyhow::Result<()> {
        todo!("IosSimulatorClient::request_frame_update")
    }

    async fn inject_pointer(&mut self, x: u32, y: u32, button_mask: u32) -> anyhow::Result<()> {
        let _ = (x, y, button_mask);
        todo!("IosSimulatorClient::inject_pointer")
    }

    async fn inject_key(&mut self, keysym: u32, pressed: bool) -> anyhow::Result<()> {
        let _ = (keysym, pressed);
        todo!("IosSimulatorClient::inject_key")
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        todo!("IosSimulatorClient::stop")
    }
}
