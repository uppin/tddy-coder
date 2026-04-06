//! LiveKit room connection and screenshare video publishing.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use image::RgbaImage;
use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::prelude::*;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use tokio::sync::Mutex;

pub struct ScreenShareStreamer {
    inner: Arc<Mutex<Option<ScreenShareState>>>,
}

impl Default for ScreenShareStreamer {
    fn default() -> Self {
        Self::new()
    }
}

struct ScreenShareState {
    #[allow(dead_code)]
    room: Arc<Room>,
    video_source: NativeVideoSource,
    #[allow(dead_code)]
    _video_track: LocalVideoTrack,
    origin: Instant,
    width: u32,
    height: u32,
}

impl ScreenShareStreamer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(
        &self,
        url: &str,
        token: &str,
        track_name: &str,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            anyhow::bail!("streamer already started");
        }

        let (room, mut room_events) = Room::connect(url, token, RoomOptions::default())
            .await
            .context("failed to connect to LiveKit")?;

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
                    source: TrackSource::Screenshare,
                    video_codec: VideoCodec::H264,
                    ..Default::default()
                },
            )
            .await
            .context("failed to publish video track")?;

        *guard = Some(ScreenShareState {
            room: room_arc,
            video_source,
            _video_track: track,
            origin: Instant::now(),
            width,
            height,
        });

        Ok(())
    }

    pub async fn push_rgba_frame(&self, image: &RgbaImage) -> Result<()> {
        let guard = self.inner.lock().await;
        let state = guard.as_ref().context("streamer not started")?;

        let width = image.width();
        let height = image.height();
        if width != state.width || height != state.height {
            anyhow::bail!(
                "frame size {}x{} does not match stream {}x{}",
                width,
                height,
                state.width,
                state.height
            );
        }

        let abgr = rgba_to_abgr(image);
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
        };

        state.video_source.capture_frame(&video_frame);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        *guard = None;
        Ok(())
    }
}

fn rgba_to_abgr(img: &RgbaImage) -> Vec<u8> {
    let mut abgr = Vec::with_capacity(img.len());
    for p in img.pixels().map(|p| p.0) {
        abgr.push(p[3]);
        abgr.push(p[2]);
        abgr.push(p[1]);
        abgr.push(p[0]);
    }
    abgr
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba, RgbaImage};

    #[test]
    fn rgba_to_abgr_roundtrip_len() {
        let img: RgbaImage = ImageBuffer::from_fn(2, 2, |_, _| Rgba([1, 2, 3, 4]));
        let abgr = rgba_to_abgr(&img);
        assert_eq!(abgr.len(), 16);
    }
}
