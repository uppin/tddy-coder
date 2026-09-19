# oversized-file: livekit_peer_discovery.rs

**Location:** `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **1,565 production lines** (2,119 total) · budget 500 · **3.1× over**
**Thresholds breached:** length 1565 > 500
**Restructure:** three `extract_module --to_file` seams, one per plan — designed, not applied
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 1565 | 1520 → 1565 in this PR (`SandboxedCodebaseSupport` + the advertisement) |

## What would close it — designed seams

`extract_module` is blind to sibling seams within a single plan, so these are **three plans**, not
three lines of one plan.

| Seam | Items | Out | Parent after |
|---|---|---|---|
| A | `forward_start_session_via_livekit` … `forward_stream_read_host_document_via_livekit` (L1289–1565) | ~277 | 1288 |
| B | `connect_common_room_publish_metadata` … `common_room_discovery_cycle` (L856–1288) | ~433 | 855 |
| C | `CommonRoomPeerRegistry` … `aggregate_peer_project_entries` (L343–704) | ~362 | **~493** |

## Constraints a later session must know

- **Seam A's items are reached by module path from 9 files across 2 crates**
  (`tddy_daemon_livekit::livekit_peer_discovery::forward_*`), so `reexport: "named"` is **mandatory**.
- **Seam B** is mostly private helpers that the in-file `#[cfg(test)]` module (starts L1566) reaches
  through `super::*`. Use `reexport: "glob"` there.
- Apply in the order A → B → C: each shifts the coordinates of everything below it, and re-anchoring
  between plans is required anyway.
