# oversized-file: remote_git_service.rs

**Location:** `packages/tddy-worktree-service/src/remote_git_service.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` file-length gate on #508 (`#keyring` 1/9)
**Metrics:** **1,111 production lines** (1,111 total — the file has no `#[cfg(test)]` module) · budget 500 · **2× over**
**Thresholds breached:** length 1111 > 500
**Restructure:** `extract_module --to_file` — seams designed below, not applied
**Status:** Open — **unclaimed** · regressed 2026-11-16 by #915 (1,004 → 1,111: the streamed remote-shell port path added more than the deleted `ssh_batch_mode_argv` spawn branch removed) — split deferred to a follow-up after the `#remote-shell` stack lands because successor #916 also touches this file

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 1,004 | master 1,002 → 1,004 after #508 (`#keyring` 1/9: two doc comments re-worded off the shared secret) — split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |
| 2026-11-16 | 1,111 | #915 (`#remote-shell` 1/7): `ssh_batch_mode_argv` deleted, remote pack verbs stream through `tddy-remote-shell` (`spawn_remote_pack_verb`, `supervise_streamed`) — net +107. Not split: the PR's boundaries forbid it and #916 touches the file, so a split here would cascade conflicts through the stack. Follow-up after the stack lands |

## What would close it — designed seams

Module-level items throughout (line numbers against the 2026-09-23 tree):

| Seam | Items | Approx. lines |
|---|---|---|
| A — request resolution and argv | `GitVerb` … `daemon_identity` (L114–330) | ~217 |
| B — the child relay and its pumps | `GitChildRelay` … `exit_code_of` (L616–1004) | ~389 |

Both out leave the service itself — `RemoteGitServiceImpl`, stream slots, the RPC impl and the
client-frame relay (~400 lines) — under budget.

⚠ Line numbers must be re-derived with `restructure anchors` before a plan is written.
