# oversized-file: connection_service.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service.rs`
**Category:** oversized-file
**Detected:** 2026-10-07 — `/pr-wrap` step 3.5 on `#carve` 18/21 (#533), re-measured with the change history's `loc.py`
**Metrics:** **525 production lines** by the changeset's counter (2026-10-09; 582 on `origin/master` before #536; the 508 / 503 below are an earlier counter) · budget 500 (the inline-test-block rule; the naive count to the first `#[cfg(test)]` reads 18)
**Thresholds breached:** length 525 > 500
**Restructure:** group the facade blocks by receiver (see "What would close it"); #536 has landed the moves, so the re-measure the earlier note waited for is done: 525
**Status:** Open — narrowed 2026-10-09 by #536 (582 → 525, still over 500) — **unclaimed**; the 2026-10-07 deferral (the stack's other nodes editing this file) is over with #536
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-10-07 | 497 | `origin/master` (`480c8448`), before `#carve` 18/21 |
| 2026-10-07 | 503 | `#carve` 18/21: **+6**, three `mod` declarations (`split_ports`, `attached_initial_prompt`, `svc_split_delegators`) and their blank lines. **Crossed 500** |
| 2026-10-07 | 508 | `#carve` 19/21 (#534): **+5** over 503 — three `mod` lines (`launch_ports`, `svc_launch_delegators`, `svc_resume_sandboxed_claude_cli_session`) and their blank lines. Already over 500; growth deferred again, unchanged reason: #535 and #536 also edit this file. (A re-count at `/pr-wrap` step 3.5 with the inline-test-block rule read 504 → 509 on the same two trees; the +5 agrees, the base differs by one) |
| 2026-10-08 | 510 | 509 on `origin/master` → 510 after `#keyring` 9/9 (#516): **+1**, a `mod` line for the new host-session modules (inline-test-block rule). Already over 500; growth is incidental and the split stays with this record's own follow-up |
| 2026-10-08 | 513 | `#carve` 20/21 (#535): **+3** over 510 (`origin/master` after `#keyring` 9/9) — three `mod` lines (`svc_ensure_project_available_for_start`, `svc_index_workspace_worktree`, `daemon_hook_urls`). Still over 500; growth deferred again, unchanged reason |
| 2026-10-09 | 525 | `#carve` 21/21 (#536): **582 on `origin/master` (`468b368f9`) → 525**, by the changeset's counter (any line outside an inline `#[cfg(test)] mod … { }` block; the rows above used a counter that reads lower, so they are not comparable with these two). **−57**: the `mod` / `pub use` lines of every topic that moved out are gone, replaced by the facades (`pub use tddy_*::{…}`, 37 named modules nothing else names, and four `#[cfg(test)]` glob re-exports). **Still over 500, by 25**; the file now holds `DaemonSessionHost`, the receivers' facades and the wiring declarations. Remaining split: group the facade blocks by receiver into one declaration file each (`extract_module`), no node in flight edits this file |

## What grew it

The file holds the `DaemonSessionHost` struct, its `mod` and `pub use` lines, and what little else a host
needs. Every in-place conversion node of the `#carve` stack adds the modules it creates, so the file grows by
about three lines per node while the node removes nothing from it. `#carve` 18/21 added three modules to put
the split topic's handle, its moved-down `attached_initial_prompt` and the one test-only delegator in
their own files.

## What would close it

- **After the stack lands:** #536 moves the converted topics out of this crate, which removes their `mod`
  lines. Re-measure then; if the file is at or under 500, record the number and delete this record.
- **If it is still over:** group the `mod` declarations by topic into one declaration file per topic, with the
  engine (`extract_module`), after the stack so the rename fallout meets no open PR.

Related: [`2026-09-24-lifecycle-files-over-the-400-line-target`](../../../../docs/dev/todo/2026-09-24-lifecycle-files-over-the-400-line-target.md)
(the 2026-10-07 status section).
