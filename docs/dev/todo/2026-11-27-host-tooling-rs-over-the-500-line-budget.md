# 2026-11-27 — `host_tooling.rs` is over the 500-production-line budget

**Category:** Code quality (file length)
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), `/pr-wrap` step 3.5

- `packages/tddy-host-service/src/host_tooling.rs` went from **463 → 517** production lines (counted
  to the first `#[cfg(test)]`) in PR #1060. The growth is the iOS Simulator channel wiring: the
  `ios_simulators` field on `HostTooling`, the lister set up in `SubprocessHostToolingProbe::for_config`,
  and `listing_ios_simulators_from`.
- **Why deferred:** the developer accepted the increase (2026-11-27, "that increase is fine") rather
  than split the file inside the feature PR.
- **Fix shape:** move the simulator listing wiring next to `ios_simulators.rs`, the way the X11
  channels live in `x11_displays.rs` and are only called from `host_tooling.rs`. The desktop-probe and
  readings sub-modules (`host_tooling/desktop_probes.rs`, `host_tooling/readings.rs`) show the existing
  seam. Moves of pre-existing code go through the `code-restructuring` skill.
