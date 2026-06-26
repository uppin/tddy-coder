//! RFB VNC client — connection, framebuffer capture, and input forwarding.
//!
//! Ported from ~/Code/makers-lt/common/vnc-livekit/src/vnc_client.rs with adaptations
//! for the tddy-vnc architecture.
//!
//! # STUB
//! All methods currently return `Err("not implemented")`. Implementation comes in the
//! green phase.

/// State of an active VNC client connection.
pub struct VncClientState {
    _inner: (),
}

impl VncClientState {
    /// Connect to a VNC server at `host:port`, optionally authenticating with `password`.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn connect(_host: &str, _port: u16, _password: Option<&str>) -> anyhow::Result<Self> {
        anyhow::bail!("VncClientState::connect: not implemented")
    }

    /// Current framebuffer dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (0, 0)
    }

    /// Request a framebuffer update from the server.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn request_update(&mut self, _incremental: bool) -> anyhow::Result<()> {
        anyhow::bail!("VncClientState::request_update: not implemented")
    }

    /// Poll one VNC event and apply it to the framebuffer.
    ///
    /// Returns `true` if an update was applied, `false` if no event arrived within the
    /// timeout.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn update(&mut self) -> anyhow::Result<bool> {
        anyhow::bail!("VncClientState::update: not implemented")
    }

    /// Return a copy of the current RGBA framebuffer.
    pub fn get_framebuffer(&self) -> Vec<u8> {
        vec![]
    }

    // --- Input ---

    /// Send a pointer (mouse) event with the given position and button mask.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn mouse_move(&mut self, _x: u16, _y: u16) -> anyhow::Result<()> {
        anyhow::bail!("not implemented")
    }

    /// Press or release a mouse button.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn mouse_button(
        &mut self,
        _x: u16,
        _y: u16,
        _button_mask: u8,
        _pressed: bool,
    ) -> anyhow::Result<()> {
        anyhow::bail!("not implemented")
    }

    /// Press and release a mouse button (click).
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn mouse_click(&mut self, _x: u16, _y: u16, _button: u8) -> anyhow::Result<()> {
        anyhow::bail!("not implemented")
    }

    /// Send a keyboard key press or release event.
    ///
    /// `keysym` is an X11 keysym value.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn keyboard_key(&mut self, _keysym: u32, _pressed: bool) -> anyhow::Result<()> {
        anyhow::bail!("not implemented")
    }

    /// Type a single character (press + release).
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn keyboard_type(&mut self, _keysym: u32) -> anyhow::Result<()> {
        anyhow::bail!("not implemented")
    }

    /// Type a string of ASCII characters.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn keyboard_type_string(&mut self, _s: &str) -> anyhow::Result<()> {
        anyhow::bail!("not implemented")
    }
}
