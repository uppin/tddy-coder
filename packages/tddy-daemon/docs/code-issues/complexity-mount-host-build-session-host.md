# complexity: build_session_host

**Location:** `packages/tddy-daemon/src/runtime/mount_host.rs:227` — `Runtime::build_session_host`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **74 lines** (2026-11-10) · nesting depth 1 · 1 branch keyword · 5 parameters (+ `&self`) · 0 early returns
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 90 → 74 | 1 | #876 Tier C: `build_connection_impl`, `adopt_and_build_connection` and `daemon_launcher_deps` extracted. The remaining seams are refused: the tail builder chain offers only "extract into variable", sub-expression extractions name `SplitRoomHosting`, `TerminalExposure` or `RoomTokenMinter` (spelled fully-qualified in other files' field declarations, not imported here), and the launcher expression's `approval_routes` is generated as `&_`. |

## What would close it

Bring it under 60 lines by `extract_method` once the engine spells fully-qualified trait names in
generated signatures. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the
seam with `restructure check --deep` against a warm index (`./run-index-daemon`).
