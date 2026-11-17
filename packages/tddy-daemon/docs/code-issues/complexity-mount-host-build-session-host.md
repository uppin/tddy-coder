# complexity: build_session_host

**Location:** `packages/tddy-daemon/src/runtime/mount_host.rs:227` — `Runtime::build_session_host`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **74 lines** (2026-11-10) · nesting depth 1 · 1 branch keyword · 5 parameters (+ `&self`) · 0 early returns
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — partially fixed by #924 (`#remote-shell` 8/11): restored to its before-the-stack length (73); still over 60 — unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 90 → 74 | 1 | #876 Tier C: `build_connection_impl`, `adopt_and_build_connection` and `daemon_launcher_deps` extracted. The remaining seams are refused: the tail builder chain offers only "extract into variable", sub-expression extractions name `SplitRoomHosting`, `TerminalExposure` or `RoomTokenMinter` (spelled fully-qualified in other files' field declarations, not imported here), and the launcher expression's `approval_routes` is generated as `&_`. |
| 2026-11-12 | 74 → 73 | 1 | #879 (`#orchestrators` 2/11): the headless-files sweep and the new PR-stack startup sweep share one `sweep_what_the_last_run_left` call. The file is 499 production lines (489 at the base) |
| 2026-11-16 | 73 → 73 | 1 | #918 (`#remote-shell` 4/7): re-measured — `build_session_host` unchanged. The file grew 499 → 521 with the Coder wiring, then the sweep pair moved to `runtime/last_run_sweep.rs` and the Coder wiring to `coder_credential_source::wire_coder` in the same PR, leaving it at 473 production lines. This finding is untouched and stays open |
| 2026-11-16 | 73 → 82 | #921 (`#remote-shell` 7/7): `build_session_host` wires the daemon's Coder credential source into the connection host when vaults are present. Touched, not closed — left as deferred at #876 |
| 2026-11-16 | 82 → 73 | 1 | #924 (`#remote-shell` 8/11) sweep: the Coder credential wiring this stack added was extracted; restored to its before-the-stack length. Still over 60, so this record stays open on the remainder |
| 2026-11-17 | 73 → 73 | 1 | #929 (`#activity-wake` 1/3): re-measured — `build_session_host` untouched; the agent-end forwarder starts from its own `BuildContext::forward_agent_ends`. The file grew by 21 production lines; its split is stack-shared with #930/#931 and deferred to a follow-up after the stack lands. Unchanged, stays open |

## What would close it

Bring it under 60 lines by `extract_method` once the engine spells fully-qualified trait names in
generated signatures. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the
seam with `restructure check --deep` against a warm index (`./run-index-daemon`).
