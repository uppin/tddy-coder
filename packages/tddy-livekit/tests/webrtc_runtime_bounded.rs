//! Joins that fail at ICE leave the process's thread and pipe counts where the first one did —
//! see `packages/tddy-livekit/src/webrtc_runtime.rs` for the leak this rules out.
//!
//! Run with: cargo test -p tddy-livekit --test webrtc_runtime_bounded

use std::time::Duration;

use livekit::webrtc::prelude::IceTransportsType;
use serial_test::serial;
use tddy_livekit::{ContinualGatheringPolicy, LiveKitParticipant, RoomOptions};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::{EchoServiceImpl, EchoServiceServer};
use tddy_testing_commons::wait::eventually;

const IDENTITY: &str = "daemon-behind-a-nat-it-cannot-hairpin";
/// Long enough for the SDK to tear down what it does release after a failed join — the signal
/// websocket and a tokio blocking thread or two — and for a leaked runtime to stand out past it.
const SETTLE: Duration = Duration::from_secs(30);

/// Options under which ICE can never connect: relay-only transport with no TURN server to relay
/// through, so the engine has no candidate pair to check and gives up at its ICE timeout. One join
/// attempt, so a failure costs one timeout rather than four.
fn options_that_never_complete_ice() -> RoomOptions {
    let mut options = tddy_livekit::tddy_room_options(ContinualGatheringPolicy::GatherOnce);
    options.rtc_config.ice_transport_type = IceTransportsType::Relay;
    options.join_retries = 0;
    options
}

/// A join through tddy's own participant that gets past the signal server and fails at ICE — the
/// failure the daemon's join loop repeats when its LiveKit server advertises unreachable candidates.
async fn a_join_that_fails_at_ice(livekit: &LiveKitTestkit, room: &str) {
    let token = livekit.generate_token(room, IDENTITY).expect("a token");
    let outcome = LiveKitParticipant::connect(
        &livekit.get_ws_url(),
        &token,
        EchoServiceServer::new(EchoServiceImpl),
        options_that_never_complete_ice(),
        None,
        None,
    )
    .await;
    let error = match outcome {
        Ok(_) => panic!("a relay-only join with no TURN server completed ICE"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("wait_pc_connection timed out"),
        "the join failed before ICE, which is not the failure under test: {error}"
    );
}

/// What a leaked libwebrtc runtime shows up in: the process's threads (three per runtime) and its
/// pipe descriptors (the network thread's wake-up pipe, two ends per runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessResources {
    threads: usize,
    pipe_descriptors: usize,
}

impl ProcessResources {
    fn now() -> Self {
        Self {
            threads: thread_count(),
            pipe_descriptors: pipe_descriptor_count(),
        }
    }

    fn within(self, baseline: Self) -> Result<Self, String> {
        if self.threads <= baseline.threads && self.pipe_descriptors <= baseline.pipe_descriptors {
            Ok(self)
        } else {
            Err(format!("{self:?} is above the baseline {baseline:?}"))
        }
    }
}

#[cfg(target_os = "linux")]
fn thread_count() -> usize {
    std::fs::read_dir("/proc/self/task")
        .expect("/proc/self/task")
        .count()
}

#[cfg(target_os = "macos")]
fn thread_count() -> usize {
    let listing = std::process::Command::new("ps")
        .args(["-M", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps -M");
    // One header line, then one line per thread.
    String::from_utf8_lossy(&listing.stdout).lines().count() - 1
}

#[cfg(target_os = "linux")]
const DESCRIPTOR_DIR: &str = "/proc/self/fd";
#[cfg(target_os = "macos")]
const DESCRIPTOR_DIR: &str = "/dev/fd";

fn pipe_descriptor_count() -> usize {
    use std::os::unix::fs::FileTypeExt;
    std::fs::read_dir(DESCRIPTOR_DIR)
        .expect("the process's descriptor listing")
        .filter_map(Result::ok)
        .filter(|entry| {
            std::fs::metadata(entry.path())
                .map(|meta| meta.file_type().is_fifo())
                .unwrap_or(false)
        })
        .count()
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn joins_that_fail_at_ice_leave_the_thread_and_pipe_counts_where_the_first_one_did() {
    // Given
    let livekit = LiveKitTestkit::start().await.expect("a LiveKit server");
    let room = LiveKitTestkit::unique_room("webrtc-runtime-bounded");
    a_join_that_fails_at_ice(&livekit, &room).await;
    let after_the_first_failed_join = ProcessResources::now();

    // When
    a_join_that_fails_at_ice(&livekit, &room).await;
    a_join_that_fails_at_ice(&livekit, &room).await;

    // Then
    eventually(
        "the process holds no more threads or pipe descriptors than after the first failed join",
        SETTLE,
        || ProcessResources::now().within(after_the_first_failed_join),
    )
    .await;
}
