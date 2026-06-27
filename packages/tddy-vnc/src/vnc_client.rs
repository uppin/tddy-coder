//! RFB VNC client — connection, framebuffer capture, and input forwarding.

use anyhow::{Context, Result};
use vnc::{
    ClientKeyEvent, ClientMouseEvent, PixelFormat, VncClient, VncConnector, VncEncoding, VncEvent,
    X11Event,
};

use tddy_screenshare::common::char_to_keysym;

/// State of an active VNC client connection.
pub struct VncClientState {
    client: VncClient,
    width: u32,
    height: u32,
    /// RGBA framebuffer (width × height × 4 bytes).
    framebuffer: Vec<u8>,
}

impl VncClientState {
    /// Connect to a VNC server at `host:port`, optionally authenticating with `password`.
    pub async fn connect(host: &str, port: u16, password: Option<&str>) -> Result<Self> {
        let tcp = tokio::net::TcpStream::connect(format!("{}:{}", host, port))
            .await
            .with_context(|| format!("failed to connect to VNC at {}:{}", host, port))?;

        let password = password.unwrap_or("").to_owned();
        let client = VncConnector::new(tcp)
            .set_auth_method(async move { Ok(password) })
            .add_encoding(VncEncoding::Tight)
            .add_encoding(VncEncoding::Zrle)
            .add_encoding(VncEncoding::CopyRect)
            .add_encoding(VncEncoding::Raw)
            .allow_shared(true)
            .set_pixel_format(PixelFormat::bgra())
            .build()
            .context("failed to build VncConnector")?
            .try_start()
            .await
            .context("VNC handshake failed")?
            .finish()
            .context("VNC negotiation failed")?;

        // dimensions are populated on the first SetResolution event
        Ok(Self {
            client,
            width: 0,
            height: 0,
            framebuffer: vec![],
        })
    }

    /// Current framebuffer dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Request a framebuffer update from the server.
    pub async fn request_update(&mut self, incremental: bool) -> Result<()> {
        let event = if incremental {
            X11Event::Refresh
        } else {
            X11Event::FullRefresh
        };
        self.client
            .input(event)
            .await
            .context("failed to send refresh request")
    }

    /// Poll one VNC event and apply it to the framebuffer.
    ///
    /// Returns `true` if an update was applied, `false` if no event arrived.
    pub async fn update(&mut self) -> Result<bool> {
        match self
            .client
            .poll_event()
            .await
            .context("VNC poll_event error")?
        {
            None => Ok(false),
            Some(event) => {
                self.apply_event(event)?;
                Ok(true)
            }
        }
    }

    /// Return the current RGBA framebuffer as a slice.
    pub fn get_framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Send a raw RFB pointer event with the current button mask state.
    ///
    /// `button_mask` follows the RFB convention: bit 0 = left, bit 1 = middle, bit 2 = right.
    pub async fn pointer_event(&mut self, x: u16, y: u16, button_mask: u8) -> Result<()> {
        self.client
            .input(X11Event::PointerEvent(ClientMouseEvent {
                position_x: x,
                position_y: y,
                bottons: button_mask,
            }))
            .await
            .context("failed to send pointer event")
    }

    // --- Input ---

    /// Send a pointer (mouse) move event.
    pub async fn mouse_move(&mut self, x: u16, y: u16) -> Result<()> {
        self.client
            .input(X11Event::PointerEvent(ClientMouseEvent {
                position_x: x,
                position_y: y,
                bottons: 0,
            }))
            .await
            .context("failed to send mouse_move")
    }

    /// Press or release a mouse button.
    ///
    /// `button_mask` follows the RFB convention: bit 0 = left, bit 1 = middle, bit 2 = right.
    pub async fn mouse_button(
        &mut self,
        x: u16,
        y: u16,
        button_mask: u8,
        pressed: bool,
    ) -> Result<()> {
        let mask = if pressed { button_mask } else { 0 };
        self.client
            .input(X11Event::PointerEvent(ClientMouseEvent {
                position_x: x,
                position_y: y,
                bottons: mask,
            }))
            .await
            .context("failed to send mouse_button")
    }

    /// Press and release a mouse button (click).
    pub async fn mouse_click(&mut self, x: u16, y: u16, button: u8) -> Result<()> {
        let mask = 1u8 << (button as u32);
        self.client
            .input(X11Event::PointerEvent(ClientMouseEvent {
                position_x: x,
                position_y: y,
                bottons: mask,
            }))
            .await
            .context("failed to send mouse press")?;
        self.client
            .input(X11Event::PointerEvent(ClientMouseEvent {
                position_x: x,
                position_y: y,
                bottons: 0,
            }))
            .await
            .context("failed to send mouse release")
    }

    /// Send a keyboard key press or release event.
    ///
    /// `keysym` is an X11 keysym value.
    pub async fn keyboard_key(&mut self, keysym: u32, pressed: bool) -> Result<()> {
        self.client
            .input(X11Event::KeyEvent(ClientKeyEvent {
                keycode: keysym,
                down: pressed,
            }))
            .await
            .context("failed to send keyboard_key")
    }

    /// Type a single character (press + release).
    pub async fn keyboard_type(&mut self, keysym: u32) -> Result<()> {
        self.keyboard_key(keysym, true).await?;
        self.keyboard_key(keysym, false).await
    }

    /// Type a string of ASCII characters.
    pub async fn keyboard_type_string(&mut self, s: &str) -> Result<()> {
        for c in s.chars() {
            if let Some(keysym) = char_to_keysym(c) {
                self.keyboard_type(keysym).await?;
            }
        }
        Ok(())
    }

    // --- Private ---

    fn apply_event(&mut self, event: VncEvent) -> Result<()> {
        match event {
            VncEvent::SetResolution(screen) => {
                self.width = screen.width as u32;
                self.height = screen.height as u32;
                self.framebuffer
                    .resize(self.width as usize * self.height as usize * 4, 0);
            }
            VncEvent::RawImage(rect, bgra) => {
                self.blit_bgra(rect, &bgra);
            }
            VncEvent::Copy(dst, src) => {
                self.copy_rect(dst, src);
            }
            // Ignore cursor, bell, clipboard, jpeg for now
            _ => {}
        }
        Ok(())
    }

    /// Blit a BGRA rect into the RGBA framebuffer.
    fn blit_bgra(&mut self, rect: vnc::Rect, bgra: &[u8]) {
        if self.width == 0 {
            return;
        }
        let mut src = 0usize;
        for row in 0..rect.height as u32 {
            let dst_row = rect.y as u32 + row;
            if dst_row >= self.height {
                break;
            }
            for col in 0..rect.width as u32 {
                let dst_col = rect.x as u32 + col;
                if dst_col >= self.width {
                    src += 4;
                    continue;
                }
                let dst = (dst_row * self.width + dst_col) as usize * 4;
                // BGRA → RGBA
                self.framebuffer[dst] = bgra[src + 2]; // R
                self.framebuffer[dst + 1] = bgra[src + 1]; // G
                self.framebuffer[dst + 2] = bgra[src]; // B
                self.framebuffer[dst + 3] = bgra[src + 3]; // A
                src += 4;
            }
        }
    }

    /// Copy one rect region to another within the RGBA framebuffer.
    fn copy_rect(&mut self, dst: vnc::Rect, src: vnc::Rect) {
        if self.width == 0 {
            return;
        }
        let w = self.width as usize;
        // Collect source pixels first to avoid aliasing.
        let mut tmp = Vec::with_capacity(src.width as usize * src.height as usize * 4);
        for row in 0..src.height as usize {
            let y = src.y as usize + row;
            for col in 0..src.width as usize {
                let x = src.x as usize + col;
                let i = (y * w + x) * 4;
                if i + 3 < self.framebuffer.len() {
                    tmp.extend_from_slice(&self.framebuffer[i..i + 4]);
                } else {
                    tmp.extend_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        let mut src_i = 0usize;
        for row in 0..dst.height as usize {
            let y = dst.y as usize + row;
            for col in 0..dst.width as usize {
                let x = dst.x as usize + col;
                let i = (y * w + x) * 4;
                if i + 3 < self.framebuffer.len() {
                    self.framebuffer[i..i + 4].copy_from_slice(&tmp[src_i..src_i + 4]);
                }
                src_i += 4;
            }
        }
    }
}

#[async_trait::async_trait]
impl tddy_screenshare::ScreenSharingClient for VncClientState {
    async fn connect(
        host: &str,
        port: u16,
        _username: Option<&str>,
        password: Option<&str>,
    ) -> anyhow::Result<Self> {
        VncClientState::connect(host, port, password).await
    }

    fn framebuffer_dimensions(&self) -> (u32, u32) {
        self.dimensions()
    }

    fn get_rgba_frame(&self) -> &[u8] {
        self.get_framebuffer()
    }

    async fn poll_events(&mut self) -> anyhow::Result<bool> {
        self.update().await
    }

    async fn request_frame_update(&mut self, incremental: bool) -> anyhow::Result<()> {
        self.request_update(incremental).await
    }

    async fn inject_pointer(&mut self, x: u32, y: u32, button_mask: u32) -> anyhow::Result<()> {
        self.pointer_event(x as u16, y as u16, button_mask as u8)
            .await
    }

    async fn inject_key(&mut self, keysym: u32, pressed: bool) -> anyhow::Result<()> {
        self.keyboard_key(keysym, pressed).await
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
