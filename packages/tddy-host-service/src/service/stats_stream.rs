use super::HostServiceImpl;

use tddy_service::proto::host::{HostDiskStats, HostStatsEvent};

use tddy_service::proto::host::HostLoadStats;

use tddy_service::proto::host::HostMemoryStats;

use tddy_service::proto::host::HostCpuStats;

use crate::host_stats::HostStats;

use std::sync::Arc;

use tddy_rpc::Status;

use crate::stream::MpscHostStatsStream;

use tddy_rpc::Response;

use tddy_service::proto::host::StreamHostStatsRequest;

use tddy_rpc::Request;

impl HostServiceImpl {
    /// Body of [`HostService::stream_host_stats`]; the trait method delegates here.
    pub(crate) async fn stream_host_stats_impl(
        &self,
        request: Request<StreamHostStatsRequest>,
    ) -> Result<Response<MpscHostStatsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<HostStatsEvent>();
        let host_stats = Arc::clone(&self.host_stats);
        let cpu_interval = self.host_cpu_interval;
        let disk_interval = self.host_disk_interval;

        tokio::spawn(async move {
            let read_cpu = |hs: &Arc<dyn HostStats>| HostCpuStats {
                per_core_percent: hs.cpu_per_core_percent(),
                logical_cores: hs.logical_cores(),
            };
            // Memory and load ride the fast tick with CPU: they move on the same timescale, and a
            // third timer would cost a builder parameter for no user-visible gain.
            let read_memory = |hs: &Arc<dyn HostStats>| {
                let usage = hs.memory();
                HostMemoryStats {
                    available_bytes: usage.available_bytes,
                    total_bytes: usage.total_bytes,
                }
            };
            // `None` stays `None` all the way to the wire — a host that cannot report a load average
            // must not be indistinguishable from an idle one.
            let read_load = |hs: &Arc<dyn HostStats>| {
                hs.load_average().map(|avg| HostLoadStats {
                    one_minute: avg.one_minute,
                    five_minutes: avg.five_minutes,
                    fifteen_minutes: avg.fifteen_minutes,
                })
            };
            let read_disk = |hs: &Arc<dyn HostStats>| {
                let usage = hs.disk_for_project_dir();
                HostDiskStats {
                    available_bytes: usage.available_bytes,
                    total_bytes: usage.total_bytes,
                    project_dir: usage.project_dir,
                }
            };

            // Immediate emit: read both snapshots once so the footer populates on connect.
            let mut cpu = read_cpu(&host_stats);
            let mut disk = read_disk(&host_stats);
            let mut memory = read_memory(&host_stats);
            let mut load = read_load(&host_stats);
            if tx
                .send(HostStatsEvent {
                    cpu: Some(cpu.clone()),
                    disk: Some(disk.clone()),
                    memory: Some(memory),
                    load,
                })
                .is_err()
            {
                return;
            }

            // Two independent timers: the first tick of each fires after one full period (not
            // immediately), so a tick provably reflects a fresh read of only that metric.
            let now = tokio::time::Instant::now();
            let mut cpu_tick = tokio::time::interval_at(now + cpu_interval, cpu_interval);
            let mut disk_tick = tokio::time::interval_at(now + disk_interval, disk_interval);

            loop {
                tokio::select! {
                    _ = cpu_tick.tick() => {
                        cpu = read_cpu(&host_stats);
                        memory = read_memory(&host_stats);
                        load = read_load(&host_stats);
                    }
                    _ = disk_tick.tick() => {
                        disk = read_disk(&host_stats);
                    }
                }
                if tx
                    .send(HostStatsEvent {
                        cpu: Some(cpu.clone()),
                        disk: Some(disk.clone()),
                        memory: Some(memory),
                        load,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Ok(Response::new(MpscHostStatsStream { rx }))
    }
}
