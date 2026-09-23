# oversized-file: remote_git_service.rs

**Location:** `packages/tddy-worktree-service/src/remote_git_service.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` file-length gate on #508 (`#keyring` 1/9)
**Metrics:** **1,004 production lines** (1,004 total — the file has no `#[cfg(test)]` module) · budget 500 · **2× over**
**Thresholds breached:** length 1004 > 500
**Restructure:** `extract_module --to_file` — seams designed below, not applied
**Status:** Open — **unclaimed** · pre-existing (1,002 on master); #508 added two lines

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 1,004 | master 1,002 → 1,004 after #508 (`#keyring` 1/9: two doc comments re-worded off the shared secret) — split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |

## What would close it — designed seams

Module-level items throughout (line numbers against the 2026-09-23 tree):

| Seam | Items | Approx. lines |
|---|---|---|
| A — request resolution and argv | `GitVerb` … `daemon_identity` (L114–330) | ~217 |
| B — the child relay and its pumps | `GitChildRelay` … `exit_code_of` (L616–1004) | ~389 |

Both out leave the service itself — `RemoteGitServiceImpl`, stream slots, the RPC impl and the
client-frame relay (~400 lines) — under budget.

⚠ Line numbers must be re-derived with `restructure anchors` before a plan is written.
