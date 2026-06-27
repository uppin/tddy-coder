//! RDP client skeleton implementing `ScreenSharingClient`.
//!
//! FIXME: This is a placeholder. Replace the `connect()` body with an IronRDP-based
//! implementation:
//!   1. TCP connect to `host:port`.
//!   2. TLS handshake (rustls or native-tls).
//!   3. IronRDP `ClientConnector` → `ActiveStage` via `ironrdp-tokio::TokioClientConnector`.
//!   4. Decode `ActiveStageOutput::GraphicsUpdate` into the RGBA framebuffer.
//!
//! See IronRDP examples at https://github.com/Devolutions/IronRDP for API details.

/// RDP client state for an active IronRDP session.
pub struct RdpClient {
    width: u32,
    height: u32,
    /// RGBA framebuffer (width × height × 4 bytes).
    framebuffer: Vec<u8>,
    // FIXME: Add IronRDP active session and framed stream fields here.
    // Example (not yet imported):
    //   active: ironrdp_session::ActiveStage,
    //   framed: ironrdp_tokio::TokioFramed<tokio::net::TcpStream>,
}

#[async_trait::async_trait]
impl tddy_screenshare::ScreenSharingClient for RdpClient {
    async fn connect(host: &str, port: u16, _password: Option<&str>) -> anyhow::Result<Self> {
        // FIXME: Implement RDP connection via IronRDP.
        // See crate-level doc for the required steps.
        anyhow::bail!(
            "RDP client not yet implemented — cannot connect to {}:{} (FIXME: add IronRDP)",
            host,
            port
        )
    }

    fn framebuffer_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn get_rgba_frame(&self) -> &[u8] {
        &self.framebuffer
    }

    async fn poll_events(&mut self) -> anyhow::Result<bool> {
        // FIXME: Read one RDP frame via IronRDP `ActiveStage::process()` and apply
        // `GraphicsUpdate` outputs to `self.framebuffer`.
        Ok(false)
    }

    async fn request_frame_update(&mut self, _incremental: bool) -> anyhow::Result<()> {
        // RDP is a push protocol — the server sends updates without explicit requests.
        Ok(())
    }

    async fn inject_pointer(&mut self, _x: u32, _y: u32, _button_mask: u32) -> anyhow::Result<()> {
        // FIXME: Build and send an RDP fast-path pointer event PDU.
        Ok(())
    }

    async fn inject_key(&mut self, _keysym: u32, _pressed: bool) -> anyhow::Result<()> {
        // FIXME: Convert X11 keysym to RDP scancode and send a fast-path keyboard event PDU.
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        // FIXME: Close the TCP/TLS connection gracefully.
        Ok(())
    }
}
