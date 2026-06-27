//! LiveKit video track publisher — pushes a screen-sharing framebuffer as H.264.
//!
//! Pattern: RGBA → ABGR → I420 → `NativeVideoSource::capture_frame` → WebRTC H.264 pipeline.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::prelude::*;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};

use crate::common::rgba_to_abgr;

struct ScreenSharingStreamerState {
    /// Owned room, present when `start()` created the connection.
    /// `None` when the caller (e.g. `LiveKitParticipant`) owns the room.
    #[allow(dead_code)]
    _room: Option<Arc<Room>>,
    video_source: NativeVideoSource,
    #[allow(dead_code)]
    _video_track: LocalVideoTrack,
    origin: Instant,
}

/// Publishes a screen-sharing framebuffer as a LiveKit video track.
pub struct ScreenSharingStreamer {
    state: Arc<tokio::sync::Mutex<Option<ScreenSharingStreamerState>>>,
    width: u32,
    height: u32,
}

impl ScreenSharingStreamer {
    /// Connect to a LiveKit room and publish a video track named `track_name`.
    ///
    /// Use this when the bridge owns the room connection. If a `LiveKitParticipant` already
    /// holds the room, prefer [`Self::from_local_participant`] to avoid a second connection.
    pub async fn start(url: &str, token: &str, track_name: &str, width: u32, height: u32) -> Result<Self> {
        let (room, mut room_events) = Room::connect(url, token, RoomOptions::default())
            .await
            .context("failed to connect to LiveKit room")?;

        let room_arc = Arc::new(room);
        tokio::spawn(async move { while room_events.recv().await.is_some() {} });

        let video_source = NativeVideoSource::new(VideoResolution { width, height }, false);

        let track = LocalVideoTrack::create_video_track(
            track_name,
            RtcVideoSource::Native(video_source.clone()),
        );

        room_arc
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    video_codec: VideoCodec::H264,
                    ..Default::default()
                },
            )
            .await
            .context("failed to publish screen-sharing video track")?;

        let state = ScreenSharingStreamerState {
            _room: Some(room_arc),
            video_source,
            _video_track: track,
            origin: Instant::now(),
        };

        Ok(Self {
            state: Arc::new(tokio::sync::Mutex::new(Some(state))),
            width,
            height,
        })
    }

    /// Publish a video track on an existing `LocalParticipant`.
    ///
    /// Use this when a `LiveKitParticipant` already owns the room (e.g. for serving the
    /// `ScreenSharingInputService` over the data channel from the same connection). The
    /// caller is responsible for keeping the room alive.
    pub async fn from_local_participant(
        local: LocalParticipant,
        track_name: &str,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let video_source = NativeVideoSource::new(VideoResolution { width, height }, false);

        let track = LocalVideoTrack::create_video_track(
            track_name,
            RtcVideoSource::Native(video_source.clone()),
        );

        local
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    video_codec: VideoCodec::H264,
                    ..Default::default()
                },
            )
            .await
            .context("failed to publish screen-sharing video track")?;

        let state = ScreenSharingStreamerState {
            _room: None,
            video_source,
            _video_track: track,
            origin: Instant::now(),
        };

        Ok(Self {
            state: Arc::new(tokio::sync::Mutex::new(Some(state))),
            width,
            height,
        })
    }

    /// Push a raw RGBA frame into the video track.
    pub async fn push_rgba_frame(&self, rgba: &[u8], width: u32, height: u32) -> Result<()> {
        anyhow::ensure!(
            width == self.width && height == self.height,
            "frame size {}x{} does not match stream {}x{}",
            width,
            height,
            self.width,
            self.height
        );

        let guard = self.state.lock().await;
        let state = guard.as_ref().context("streamer already stopped")?;

        let abgr = rgba_to_abgr(rgba);
        let mut i420_buffer = I420Buffer::new(width, height);
        let (stride_y, stride_u, stride_v) = i420_buffer.strides();
        let (data_y, data_u, data_v) = i420_buffer.data_mut();

        yuv_helper::abgr_to_i420(
            &abgr,
            width * 4,
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            width as i32,
            height as i32,
        );

        let ts = state.origin.elapsed().as_micros() as i64;
        let video_frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            buffer: i420_buffer,
            timestamp_us: ts,
            frame_metadata: None,
        };

        state.video_source.capture_frame(&video_frame);
        Ok(())
    }

    /// Disconnect from the room and unpublish the track.
    pub async fn stop(self) -> Result<()> {
        let mut guard = self.state.lock().await;
        *guard = None;
        Ok(())
    }
}
