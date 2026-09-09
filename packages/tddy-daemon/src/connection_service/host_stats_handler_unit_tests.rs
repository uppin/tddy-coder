use super::*;
use crate::host_stats::{DiskUsage, HostStats};
use std::sync::atomic::{AtomicU32, Ordering};
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_service::proto::connection::{HostStatsEvent, StreamHostStatsRequest};

/// Deterministic host-stats double returning fixed per-core CPU and disk figures.
struct FakeHostStats {
    per_core_percent: Vec<f32>,
    available_bytes: u64,
    total_bytes: u64,
    project_dir: String,
    available_memory_bytes: u64,
    total_memory_bytes: u64,
    /// `None` models a platform with no load average — the case a real reading must never
    /// impersonate.
    load_average: Option<crate::host_stats::LoadAverage>,
}

impl HostStats for FakeHostStats {
    fn cpu_per_core_percent(&self) -> Vec<f32> {
        self.per_core_percent.clone()
    }
    fn memory(&self) -> crate::host_stats::MemoryUsage {
        crate::host_stats::MemoryUsage {
            available_bytes: self.available_memory_bytes,
            total_bytes: self.total_memory_bytes,
        }
    }
    fn logical_cores(&self) -> u32 {
        self.per_core_percent.len() as u32
    }
    fn load_average(&self) -> Option<crate::host_stats::LoadAverage> {
        self.load_average
    }
    fn disk_for_project_dir(&self) -> DiskUsage {
        DiskUsage {
            available_bytes: self.available_bytes,
            total_bytes: self.total_bytes,
            project_dir: self.project_dir.clone(),
        }
    }
}

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service() -> ConnectionServiceImpl {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == "valid" {
            Some("u".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        make_unit_config(),
        sessions_base_resolver,
        temp.path().to_path_buf(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

// --- StreamHostStats: single server-streaming host-info feed ---

/// A host-stats double whose readings advance on every call, so a later stream event can be
/// proven to reflect a *fresh* provider read (rather than a repeat of the first snapshot). The
/// Nth CPU read returns `[N.0]`; the Nth disk read reports `available_bytes = N`.
struct SequencedHostStats {
    cpu_reads: AtomicU32,
    disk_reads: AtomicU32,
    memory_reads: AtomicU32,
}

impl SequencedHostStats {
    fn new() -> Self {
        Self {
            cpu_reads: AtomicU32::new(0),
            disk_reads: AtomicU32::new(0),
            memory_reads: AtomicU32::new(0),
        }
    }
}

impl HostStats for SequencedHostStats {
    fn cpu_per_core_percent(&self) -> Vec<f32> {
        let nth = self.cpu_reads.fetch_add(1, Ordering::SeqCst) + 1;
        vec![nth as f32]
    }
    /// Advances on its own counter so a fast tick can be shown to re-read memory, not just CPU.
    fn memory(&self) -> crate::host_stats::MemoryUsage {
        let nth = self.memory_reads.fetch_add(1, Ordering::SeqCst) + 1;
        crate::host_stats::MemoryUsage {
            available_bytes: nth as u64,
            total_bytes: 100,
        }
    }
    fn logical_cores(&self) -> u32 {
        1
    }
    fn load_average(&self) -> Option<crate::host_stats::LoadAverage> {
        Some(crate::host_stats::LoadAverage {
            one_minute: 1.0,
            five_minutes: 5.0,
            fifteen_minutes: 15.0,
        })
    }
    fn disk_for_project_dir(&self) -> DiskUsage {
        let nth = self.disk_reads.fetch_add(1, Ordering::SeqCst) + 1;
        DiskUsage {
            available_bytes: nth as u64,
            total_bytes: 100,
            project_dir: "/home/dev/repos".to_string(),
        }
    }
}

/// Await the next stream event with a bounded timeout so a missing event fails loudly instead
/// of hanging the test.
async fn next_event(
    stream: &mut (impl Stream<Item = Result<HostStatsEvent, Status>> + Unpin),
) -> HostStatsEvent {
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("no host-stats event arrived within the timeout")
        .expect("host-stats stream closed unexpectedly")
        .expect("host-stats stream yielded an error")
}

#[tokio::test]
async fn stream_host_stats_rejects_an_invalid_token() {
    // Given a service that would stream host telemetry
    let service = make_unit_service().with_host_stats(Arc::new(FakeHostStats {
        per_core_percent: vec![10.0, 55.0, 90.0, 30.0],
        available_bytes: 42_100_000_000,
        total_bytes: 100_000_000_000,
        project_dir: "/home/dev/repos".to_string(),
        available_memory_bytes: 8_000_000_000,
        total_memory_bytes: 16_000_000_000,
        load_average: Some(crate::host_stats::LoadAverage {
            one_minute: 0.5,
            five_minutes: 0.4,
            fifteen_minutes: 0.3,
        }),
    }));

    // When an unauthenticated caller subscribes to the host-stats stream
    let result = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "bad-token".to_string(),
        }))
        .await;

    // Then the subscription is rejected as unauthenticated
    assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
}

#[tokio::test]
async fn stream_host_stats_emits_cpu_and_disk_immediately_on_subscribe() {
    // Given a service reporting four cores at 10 / 55 / 90 / 30 % and 42.1 GB free of 100 GB
    let service = make_unit_service().with_host_stats(Arc::new(FakeHostStats {
        per_core_percent: vec![10.0, 55.0, 90.0, 30.0],
        available_bytes: 42_100_000_000,
        total_bytes: 100_000_000_000,
        project_dir: "/home/dev/repos".to_string(),
        available_memory_bytes: 8_000_000_000,
        total_memory_bytes: 16_000_000_000,
        load_average: Some(crate::host_stats::LoadAverage {
            one_minute: 0.5,
            five_minutes: 0.4,
            fifteen_minutes: 0.3,
        }),
    }));

    // When an authenticated caller subscribes
    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the very first event carries both the current CPU and the current disk snapshot
    let event = next_event(&mut stream).await;
    let cpu = event.cpu.expect("first event must carry a CPU snapshot");
    let disk = event.disk.expect("first event must carry a disk snapshot");
    assert_eq!(cpu.per_core_percent, vec![10.0, 55.0, 90.0, 30.0]);
    assert_eq!(disk.available_bytes, 42_100_000_000);
    assert_eq!(disk.total_bytes, 100_000_000_000);
    assert_eq!(disk.project_dir, "/home/dev/repos");
}

#[tokio::test]
async fn stream_host_stats_refreshes_cpu_on_the_fast_cadence() {
    // Given a service whose provider advances on each read, a fast CPU cadence, and a disk
    // cadence too slow to fire within the test window
    let service = make_unit_service()
        .with_host_stats(Arc::new(SequencedHostStats::new()))
        .with_host_stats_intervals(Duration::from_millis(20), Duration::from_secs(30));

    // When a caller subscribes and reads two successive events
    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let first = next_event(&mut stream).await;
    let second = next_event(&mut stream).await;

    // Then each event's CPU reflects a fresh provider read (1st then 2nd), while the disk block
    // is unchanged because the slow cadence has not fired
    assert_eq!(first.cpu.expect("cpu").per_core_percent, vec![1.0]);
    assert_eq!(second.cpu.expect("cpu").per_core_percent, vec![2.0]);
    assert_eq!(
        second.disk.expect("disk").available_bytes,
        first.disk.expect("disk").available_bytes
    );
}

#[tokio::test]
async fn stream_host_stats_refreshes_disk_on_the_slow_cadence() {
    // Given a service whose provider advances on each read, a fast disk cadence, and a CPU
    // cadence too slow to fire within the test window
    let service = make_unit_service()
        .with_host_stats(Arc::new(SequencedHostStats::new()))
        .with_host_stats_intervals(Duration::from_secs(30), Duration::from_millis(20));

    // When a caller subscribes and reads two successive events
    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let first = next_event(&mut stream).await;
    let second = next_event(&mut stream).await;

    // Then each event's disk reflects a fresh provider read (1st then 2nd), while the CPU block
    // is unchanged because the slow cadence has not fired
    assert_eq!(first.disk.expect("disk").available_bytes, 1);
    assert_eq!(second.disk.expect("disk").available_bytes, 2);
    assert_eq!(
        second.cpu.expect("cpu").per_core_percent,
        first.cpu.expect("cpu").per_core_percent
    );
}
// --- host-resources (#hosts-screen 3/8): memory, load average and core count ---

/// Memory and load must arrive with the very first event, not only after a tick — a screen that
/// opens on a struggling host has to say so immediately.
#[tokio::test]
async fn stream_host_stats_emits_memory_and_load_immediately_on_subscribe() {
    let service = make_unit_service().with_host_stats(Arc::new(SequencedHostStats::new()));

    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .expect("subscribe")
        .into_inner();

    let event = next_event(&mut stream).await;

    let memory = event.memory.expect("the first event must carry memory");
    assert_eq!(memory.total_bytes, 100);
    let load = event.load.expect("this provider reports a load average");
    assert_eq!(load.one_minute, 1.0);
    let cpu = event.cpu.expect("the first event must carry cpu");
    assert_eq!(cpu.logical_cores, 1, "core count is reported explicitly");
}

/// Memory rides the *fast* tick with CPU, because it moves on the same timescale. Proven by the
/// sequenced provider: a second event must reflect a fresh memory read, not a repeat of the
/// first snapshot.
#[tokio::test]
async fn stream_host_stats_refreshes_memory_on_the_fast_cadence() {
    let service = make_unit_service()
        .with_host_stats(Arc::new(SequencedHostStats::new()))
        .with_host_stats_intervals(Duration::from_millis(20), Duration::from_secs(30));

    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .expect("subscribe")
        .into_inner();

    let first = next_event(&mut stream).await;
    let second = next_event(&mut stream).await;

    let first_memory = first.memory.expect("memory in the first event");
    let second_memory = second.memory.expect("memory in the second event");
    assert!(
        second_memory.available_bytes > first_memory.available_bytes,
        "a fast tick must re-read memory (first={}, second={})",
        first_memory.available_bytes,
        second_memory.available_bytes
    );
}

/// Disk stays on the slow tick. Adding memory to the fast tick must not drag disk along with it —
/// the whole reason there are two timers is that a disk walk is the expensive one.
#[tokio::test]
async fn stream_host_stats_still_refreshes_disk_on_the_slow_cadence() {
    let service = make_unit_service()
        .with_host_stats(Arc::new(SequencedHostStats::new()))
        .with_host_stats_intervals(Duration::from_millis(20), Duration::from_secs(30));

    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .expect("subscribe")
        .into_inner();

    let first = next_event(&mut stream).await;
    let second = next_event(&mut stream).await;

    assert_eq!(
        first.disk.expect("disk").available_bytes,
        second.disk.expect("disk").available_bytes,
        "the slow tick has not fired, so disk must be the unchanged snapshot"
    );
}

/// A provider with no load average must produce an event with **no** load block. Sending zeros
/// would make an unsupported platform indistinguishable from an idle machine.
#[tokio::test]
async fn stream_host_stats_marks_load_average_unreported_when_the_provider_has_none() {
    let service = make_unit_service().with_host_stats(Arc::new(FakeHostStats {
        per_core_percent: vec![1.0],
        available_bytes: 1,
        total_bytes: 2,
        project_dir: "/tmp".to_string(),
        available_memory_bytes: 3,
        total_memory_bytes: 4,
        load_average: None,
    }));

    let mut stream = service
        .stream_host_stats(Request::new(StreamHostStatsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .expect("subscribe")
        .into_inner();

    let event = next_event(&mut stream).await;

    assert!(
        event.load.is_none(),
        "a host with no load average must omit the block, not send zeros"
    );
    assert!(
        event.memory.is_some(),
        "memory is still reported on such a host"
    );
}
