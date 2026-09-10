# 2026-09-06 — `connection_service.rs` is 22,800 lines — resolved 2026-09-09

**Category:** Host tooling probe
**Status:** Resolved
**Source:** subagent-conversation-inference changeset, 2026-08-29; figure remeasured 2026-09-06

**Resolved 2026-09-09** by [#470](https://github.com/uppin/tddy-coder/pull/470). This entry named the seam the 2026-08-29 one had missed — *"the host
registry, the tooling probe and the prompt handlers"* — and that seam is exactly what #470 cut: those
handlers now live in `packages/tddy-host-service` and serve `host.HostService`.
[Changeset](../changesets/2026-09-09-unbundle-host-worktree-services.md).


  changeset adds 44 lines to it, and a split would bury a reviewable feature under a 22,800-line
- The host-facing group the `#hosts-screen` stack adds — the host registry, the tooling probe and
  the prompt handlers — is a further cohesive seam, and is not among the ones listed below.
