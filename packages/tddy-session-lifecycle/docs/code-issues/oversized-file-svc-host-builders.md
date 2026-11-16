# oversized-file: svc_host_builders.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs`
**Category:** oversized-file
**Detected:** 2026-11-10 — `/pr-wrap` step 3.5 file-length gate on #876 (`#agent-tools` 33/33 sweep), production lines counted to the first `#[cfg(test)]`
**Metrics:** **493 → 531 production lines** (2026-11-10) · budget 500
**Thresholds breached:** length 531 > 500
**Restructure:** `move_item` / `move_impl_members` into a submodule — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-10 | 493 → 531 | #876: the Tier C extracts out of `DaemonSessionHost::new` (112 → 97) added `default_agent_tool_sockets`, `HostStores` and `build_host_stores` beside it |
| 2026-11-16 | 531 → 533 | #920 (`#remote-shell` 6/7): the host gains the remote-hook followers registry (2 lines). Touched, not closed. **Not split**: successor PR #921 (`feature/remote-shell/coder-hosts`) also modifies this file (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |

## What would close it

Move `default_agent_tool_sockets`, `HostStores` and `build_host_stores` (about 53 lines) into `svc_host_builders::host_stores` with one `move_item` carrying `name`, or move the `with_*` setters out with `move_impl_members`. Anchor with `tddy-tools restructure anchors`, never by hand, prove the seam with
`restructure check --deep` against a warm index (`./run-index-daemon`), and confirm the file and the
new module are each ≤ 500 production lines.
