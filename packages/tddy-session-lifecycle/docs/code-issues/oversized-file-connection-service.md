# oversized-file: connection_service.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service.rs`
**Category:** oversized-file
**Detected:** 2026-10-07 — `/pr-wrap` step 3.5 on `#carve` 18/21 (#533), re-measured with the change history's `loc.py`
**Metrics:** **508 production lines** (503 at #533) · budget 500 (the inline-test-block rule; the naive count to the first `#[cfg(test)]` reads 18)
**Thresholds breached:** length 508 > 500
**Restructure:** none planned; wait for #536 (the moves that leave this crate a wiring crate), then re-measure
**Status:** Open — deferred by the developer 2026-10-07 (stack branch: #534, #535 and #536 also edit this file) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-10-07 | 497 | `origin/master` (`480c8448`), before `#carve` 18/21 |
| 2026-10-07 | 503 | `#carve` 18/21: **+6**, three `mod` declarations (`split_ports`, `attached_initial_prompt`, `svc_split_delegators`) and their blank lines. **Crossed 500** |
| 2026-10-07 | 508 | `#carve` 19/21 (#534): **+5** over 503 — three `mod` lines (`launch_ports`, `svc_launch_delegators`, `svc_resume_sandboxed_claude_cli_session`) and their blank lines. Already over 500; growth deferred again, unchanged reason: #535 and #536 also edit this file. (A re-count at `/pr-wrap` step 3.5 with the inline-test-block rule read 504 → 509 on the same two trees; the +5 agrees, the base differs by one) |
| 2026-10-08 | 510 | 509 on `origin/master` → 510 after `#keyring` 9/9 (#516): **+1**, a `mod` line for the new host-session modules (inline-test-block rule). Already over 500; growth is incidental and the split stays with this record's own follow-up |
| 2026-10-08 | 510 | `#carve` 20/21 (#535): **+2** over 508 (`wc -l` 571 -> 573 against `origin/master`) — two `mod` lines (`svc_ensure_project_available_for_start`, `svc_index_workspace_worktree`). Still over 500; growth deferred again, unchanged reason: #536 also edits this file. **Regressed, kept open** |

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
