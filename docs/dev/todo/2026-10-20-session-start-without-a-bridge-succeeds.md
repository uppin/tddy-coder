# 2026-10-20 — a session-scope start that spawns no bridge still succeeds

**Category:** Behaviour gap
**Source:** `#host-desktops` 6/8 (`feature/host-desktops/bridge-ready`, PR #658)

- `packages/tddy-screen-sharing/src/screen_sharing_service.rs` `start_stream`: three branches spawn no
  bridge and still answer `Ok(StartStreamResponse)`: the daemon has no config (`self.config` is
  `None`), the target id is not in the session's store (`error!("… not found")`), or the stored
  target does not convert to a `BridgeTarget` (`error!("start_stream: target …")`). Since #658 the
  reply carries `width: 0, height: 0` there instead of the old guessed 1920×1080.
- The browser then mounts a viewer on a room no bridge joins, which is the "Connecting… forever"
  failure #658 fixed for refused sign-ins and silent bridges, still reachable through these paths.
- **Why deferred:** AC-SS-9's acceptance test
  `tests/screen_sharing_service_acceptance.rs::opening_a_desktop_returns_the_coordinates_a_viewer_needs`
  pins success for a service built without daemon config, and #658's PRD covers bridges that were
  started, not starts that never spawn one. Changing it needs a decision on that AC first.
- **Fix shape:** `NotFound` for an unknown target, `InvalidArgument` for an unconvertible one, and
  `FailedPrecondition` for a daemon without config. The AC-SS-9 test then builds its service with a
  stand-in bridge, as `tests/bridge_readiness_acceptance.rs` already does.
