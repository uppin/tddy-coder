//! CLI: list capture targets or stream a monitor/window to LiveKit.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use tddy_livekit_screen_capture::capture::{format_target_list, parse_target, resolve_target};
use tddy_livekit_screen_capture::config::{load_config_file, resolve_stream_config, CliOverrides};
#[cfg(target_os = "macos")]
use tddy_livekit_screen_capture::macos_access::request_screen_capture_access;
use tddy_livekit_screen_capture::streamer::ScreenShareStreamer;

#[derive(Parser, Debug)]
#[command(name = "tddy-livekit-screen-capture")]
#[command(about = "Capture a screen or window and stream to LiveKit as screenshare video.")]
struct Cli {
    /// List monitors and windows that can be streamed.
    #[arg(long)]
    list: bool,

    /// YAML config file with `livekit` and optional `fps` (same shape as tddy-coder).
    #[arg(short = 'c', long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Frames per second (overrides config file).
    #[arg(long)]
    fps: Option<u32>,

    /// LiveKit room (overrides config file).
    #[arg(long)]
    room: Option<String>,

    /// Participant identity (overrides config file).
    #[arg(long)]
    identity: Option<String>,

    /// Capture target from `--list`, e.g. `monitor:0` or `window:12345`.
    #[arg(value_name = "TARGET")]
    target: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    if cli.list {
        let text = format_target_list().context("failed to enumerate capture targets")?;
        print!("{}", text);
        return Ok(());
    }

    let target_str = cli
        .target
        .as_deref()
        .context("TARGET is required when not using --list")?;

    #[cfg(target_os = "macos")]
    request_screen_capture_access();

    let config_path = cli
        .config
        .as_deref()
        .context("-c/--config is required when not using --list")?;

    let file = load_config_file(config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    let resolved = resolve_stream_config(
        Some(&file),
        &CliOverrides {
            room: cli.room.clone(),
            identity: cli.identity.clone(),
            fps: cli.fps,
        },
    )
    .context("invalid LiveKit configuration")?;

    let spec = parse_target(target_str).context("invalid TARGET")?;
    let stream_target = resolve_target(&spec).context("failed to resolve TARGET")?;

    let (width, height) = stream_target
        .width_height()
        .context("failed to read capture size")?;

    let label = stream_target
        .label_for_track()
        .context("failed to build track label")?;
    let track_name = format!("screen-{}", label);

    log::info!(
        "Connecting to LiveKit room={} identity={} size={}x{} fps={}",
        resolved.room,
        resolved.identity,
        width,
        height,
        resolved.fps
    );

    let streamer = ScreenShareStreamer::new();
    streamer
        .start(&resolved.url, &resolved.token, &track_name, width, height)
        .await
        .context("failed to start LiveKit screenshare")?;

    let frame_duration = Duration::from_millis(1000 / u64::from(resolved.fps.max(1)));

    let run = async {
        loop {
            let start = Instant::now();
            let image = tokio::task::spawn_blocking({
                let t = stream_target.clone();
                move || t.capture_image()
            })
            .await
            .context("capture task join failed")?
            .map_err(|e| anyhow::anyhow!("screen capture failed: {}", e))?;

            streamer
                .push_rgba_frame(&image)
                .await
                .context("failed to push video frame")?;

            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                tokio::time::sleep(frame_duration - elapsed).await;
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("signal received, stopping");
        }
        r = run => {
            r?;
        }
    }

    streamer.stop().await?;
    Ok(())
}
