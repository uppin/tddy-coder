//! LiveKit video track publisher — pushes VNC framebuffer as an H.264 video stream.
//!
//! Pattern mirrors `tddy-livekit-screen-capture/src/streamer.rs`:
//! RGBA → ABGR → I420 → `NativeVideoSource::capture_frame` → WebRTC H.264 pipeline.
//!
//! # STUB
//! All methods currently return `Err("not implemented")`. Implementation comes in the
//! green phase.

/// Publishes a VNC session's framebuffer as a LiveKit video track.
pub struct VncStreamer {
    _inner: (),
}

impl VncStreamer {
    /// Connect to a LiveKit room and publish a video track named `track_name`.
    ///
    /// Uses `TrackSource::Camera` to distinguish the VNC track from a screenshare.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn start(
        _url: &str,
        _token: &str,
        _track_name: &str,
        _width: u32,
        _height: u32,
    ) -> anyhow::Result<Self> {
        anyhow::bail!("VncStreamer::start: not implemented")
    }

    /// Push a raw RGBA frame into the video track.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn push_rgba_frame(
        &self,
        _rgba: &[u8],
        _width: u32,
        _height: u32,
    ) -> anyhow::Result<()> {
        anyhow::bail!("VncStreamer::push_rgba_frame: not implemented")
    }

    /// Disconnect from the room and unpublish the track.
    ///
    /// # Errors
    /// **STUB — always errors.**
    pub async fn stop(self) -> anyhow::Result<()> {
        anyhow::bail!("VncStreamer::stop: not implemented")
    }
}
