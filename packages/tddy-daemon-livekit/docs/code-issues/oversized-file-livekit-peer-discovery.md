# oversized-file: livekit_peer_discovery.rs

**Location:** `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **1,407 production lines** (2026-10-20; 1,565 at detection) · budget 500 · **2.8× over**
**Thresholds breached:** length 1407 > 500
**Restructure:** seam A closed by #632 (the typed helpers moved to `tddy_daemon_kernel::peer_calls`); seams B and C designed, not applied
**Status:** Open — partly fixed 2026-10-20 (−293, #632, `#satellite` 10/13); regressed 2026-10-17 (+3 from #624, `#satellite` 2/13; deferred with the developer's consent); — regressed 2026-10-17 (+39 from #623, `#satellite` 1/13; deferred with the developer's consent); **unclaimed** — regressed 2026-10-04 (+22, #512)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 1565 | 1520 → 1565 in this PR (`SandboxedCodebaseSupport` + the advertisement) |
| 2026-09-23 | 1,638 | master 1,565 → 1,638 after #508 (`#keyring` 1/9: the signing-key advertisement fields, `AdvertisedSigningKey`, `peer_signing_public_keys`, the shared identity rule) — grown by #508; split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |
| 2026-10-04 | 1,636 | 1,638 on `master` (`6faef323`) → 1,636 after `#e2e-leg` 2/5 (#579): the forwarding call sites take a `CommonRoom` instead of the bare room slot (a type change at the seam, −2). Not grown; still over budget, the split still deferred |
| 2026-10-04 | 1,658 | 1,636 → 1,658 in #512 (`#keyring` 5/9): `forward_set_project_accounts_via_livekit`, the sibling of `forward_set_project_default_branch_via_livekit` — it lands in seam A's range. Split still deferred: #513 (`#keyring` 6/9) also edits this file, so a split now would conflict it |
| 2026-10-17 | 1,658 → 1,697 (+39) | `#satellite` 1/13 (#623): the discovery loop reports `Connecting` / `Connected` / `Unreachable` on the integration status board (`common_room_failure_state`, `common_room_lost_state`, a trailing `IntegrationReporter`). Grown; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` — and because #624 (`#satellite` 2/13) also edits this file |
| 2026-10-17 | 1,697 → 1,700 (+3) | `#satellite` 2/13 (#624): the two forwarding call sites build `CommonRoom::new(LiveKitPeerForwarder::new(slot), &config)` instead of `CommonRoom::from_config(slot, &config)`. Grown; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` — and because later `#satellite` nodes also edit this file |
| 2026-10-20 | 1,700 → 1,407 (−293) | `#satellite` 10/13 (#632): the thirteen `forward_*_via_livekit` helpers moved to `tddy_daemon_kernel::peer_calls` (seam A, by hand-written move rather than `extract_module`); the file keeps a `pub use` of each under its old name until node 11 moves the last callers. Narrowed, not closed |

## What would close it — designed seams

`extract_module` is blind to sibling seams within a single plan, so these are **three plans**, not
three lines of one plan.

| Seam | Items | Out | Parent after |
|---|---|---|---|
| ~~A~~ (done, #632) | `forward_start_session_via_livekit` … `forward_stream_read_host_document_via_livekit` (L1289–1565) | ~277 | 1288 |
| B | `connect_common_room_publish_metadata` … `common_room_discovery_cycle` (L856–1288) | ~433 | 855 |
| C | `CommonRoomPeerRegistry` … `aggregate_peer_project_entries` (L343–704) | ~362 | **~493** |

## Constraints a later session must know

- **Seam A is done** (#632). Its coordinates below are pre-move; re-anchor B and C before applying.
- **Seam B** is mostly private helpers that the in-file `#[cfg(test)]` module (starts L1566) reaches
  through `super::*`. Use `reexport: "glob"` there.
- Apply in the order A → B → C: each shifts the coordinates of everything below it, and re-anchoring
  between plans is required anyway.
