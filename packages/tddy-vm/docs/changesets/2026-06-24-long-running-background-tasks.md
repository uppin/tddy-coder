# 2026-06-24 — **Long-running background Tasks

**Type:** Feature

VM build fold-in** — `build.rs`: `VmBuildTaskBody` implements `TaskBody` with cooperative cancel via `CancellationToken` + SIGINT to `make` PID; `build_vm_image_from_spec` returns `bool` (true=success) mapping correctly to `Completed`/`Failed`/`Cancelled`; `service.rs`: `VmServiceImpl::new` accepts shared `TaskRegistry`; `BuildVmImage` spawns `VmBuildTaskBody` and streams its progress channel back as `BuildVmImageProgress`. Tests: 2 fold-in acceptance tests. Feature [daemon/background-tasks.md](../../../../docs/ft/daemon/background-tasks.md). (tddy-vm)
