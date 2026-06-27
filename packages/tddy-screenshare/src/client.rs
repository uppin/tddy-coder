/// Protocol-agnostic screen-sharing client trait.
///
/// Implemented by `VncClientState` (tddy-vnc) and `RdpClient` (tddy-rdp).
/// The generic bridge runner in `bridge.rs` is parameterised over this trait.
#[async_trait::async_trait]
pub trait ScreenSharingClient: Sized + Send + 'static {
    /// Open a connection to the remote desktop at `host:port`.
    ///
    /// `password` is `None` for password-less targets.
    async fn connect(host: &str, port: u16, password: Option<&str>) -> anyhow::Result<Self>;

    /// Current framebuffer dimensions `(width, height)` in pixels.
    ///
    /// Returns `(0, 0)` until the server sends initial screen dimensions.
    fn framebuffer_dimensions(&self) -> (u32, u32);

    /// Reference to the current RGBA framebuffer (width × height × 4 bytes).
    fn get_rgba_frame(&self) -> &[u8];

    /// Process one pending event from the server.
    ///
    /// Returns `true` if an event was processed, `false` if no events were pending.
    async fn poll_events(&mut self) -> anyhow::Result<bool>;

    /// Request the next framebuffer update from the server.
    ///
    /// `incremental = false` requests a full refresh; `true` requests only changed
    /// regions. Implementations that use a push protocol (e.g. RDP) may treat this
    /// as a no-op.
    async fn request_frame_update(&mut self, incremental: bool) -> anyhow::Result<()>;

    /// Send a pointer (mouse) event to the server.
    ///
    /// `button_mask` follows the RFB convention: bit 0 = left, bit 1 = middle,
    /// bit 2 = right.
    async fn inject_pointer(&mut self, x: u32, y: u32, button_mask: u32) -> anyhow::Result<()>;

    /// Send a keyboard event to the server.
    ///
    /// `keysym` is an X11 keysym value; `pressed` is `true` for key-down.
    async fn inject_key(&mut self, keysym: u32, pressed: bool) -> anyhow::Result<()>;

    /// Cleanly close the connection (best-effort; errors are ignored by the bridge).
    async fn stop(&mut self) -> anyhow::Result<()>;
}
