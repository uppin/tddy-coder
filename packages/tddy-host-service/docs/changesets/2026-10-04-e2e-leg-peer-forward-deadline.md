# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

`HostServiceImpl::with_common_room` keeps its slot-taking signature and builds the `CommonRoom` from the
host's own `DaemonConfig`; the host's forwards go through it. `service.rs` production lines 939 -> 934
(`oversized-file-service` stays open, row added).
