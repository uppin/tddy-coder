# 2026-09-14 — SSH config Host alias listing

**Type:** Feature

Added `ListSshConfigHosts` on `host.HostService` with an in-tree `ssh_config` parser (`Include`,
wildcard skip, first-seen dedupe). Unreadable config surfaces as `ProbeOutcome::Failed`; missing
config is an empty alias list. Handler peer-forwards like `ListHostKeyCandidates`. Documented in
[host-service.md](../host-service.md) and [host-tooling-probe.md](../host-tooling-probe.md).
