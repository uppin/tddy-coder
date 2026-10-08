# 2026-09-24 — 14 `tddy-session-lifecycle` files end between 400 and 493 production lines, over the destructure's ≤ 400 target

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
("Consent list" item 7; the Responsibility "every file ends under 500, with a target of ≤ 400")

## Why deferred

The destructure met its **hard** limit: no non-test file is at or over 500 production lines (it
began with 11, 2 of them over 1,000). The **target** of ≤ 400 was not met for 14 files, and no seam
for them was planned. Most are single-topic files that were never oversized. The rest are what the
blocked functions leave behind: a file cannot shrink while the function it holds cannot be cut. The
run ended at the developer's "file the TODOs and move on" (2026-09-24).

## The measurement

At `52e621a3`. A production line is a line outside every `#[cfg(test)]` item, with test-only files
excluded (the inline-test-block rule; the script is in the change history's "LoC assessment").
The naive count, to the first `#[cfg(test)]`, is in brackets where it differs.

| # | File (`src/`) | Production lines | Blocked by |
|---:|---|---:|---|
| 1 | `connection_service/svc_start_sandboxed_claude_cli_session.rs` | **493** | `start_sandboxed_claude_cli_session` (342): DRY #1, U |
| 2 | `connection_service/svc_split_context_from_codebase_host.rs` | 471 [457] | — never planned |
| 3 | `session_deletion.rs` | 459 | — never planned |
| 4 | `connection_service/svc_start_session_core.rs` | 455 | `start_session_core` (358): E4 |
| 5 | `cursor_cli_spawn.rs` | 445 | `spawn_cursor_cli_session_inner` (244): T |
| 6 | `connection_service/svc_spawn_split_agent.rs` | 445 | — its teardown already moved out |
| 7 | `connection_service/svc_start_sandboxed_cursor_cli_session.rs` | 445 | `start_sandboxed_cursor_cli_session` (414): DRY #1 |
| 8 | `connection_service/svc_resolve_listed_worktree.rs` | 443 | — never planned |
| 9 | `connection_service.rs` | 434 [18] | — the struct, `mod` / `pub use` lines, and what folding would take out |
| 10 | `connection_service/svc_ensure_session_room_for_agents.rs` | 429 | — never planned |
| 11 | `connection_service/svc_start_hosted_agent_clone.rs` | 425 | — never planned |
| 12 | `connection_service/session_coordinate_handlers.rs` | 453 | `session_entry_from_listing` is lifted as a free function in the file (`#carve` 20/21); moving it into `session_coordinate_handlers/session_list_entries.rs` is not done |
| 13 | `connection_service/svc_provision_agent_clone.rs` | 414 | — never planned |
| 14 | `split_session.rs` | 409 [382] | — never planned |

The five production functions still over 150 lines, and why each stopped, are
[their own TODO](./2026-09-24-lifecycle-functions-still-over-150-lines.md):
`start_sandboxed_cursor_cli_session` 414, `start_session_core` 358,
`start_sandboxed_claude_cli_session` 342, `spawn_claude_cli_session_inner` 255,
`spawn_cursor_cli_session_inner` 244.

For example, row 12. The rest of the file is the other handlers (start, connect, worktree snapshot,
streamed start) and their helpers:

```text
session_coordinate_handlers.rs      414
  list_sessions_at_session_coordinate   :38    ~128 lines, of which the SessionEntry literal is ~65
  → session_entry_from_listing into session_coordinate_handlers/session_list_entries.rs: ~300   (the function is lifted; the module move is open)
```

## What would close it

- **Rows 1, 4, 5, 7, 12** close when their functions do: the
  [functions TODO](./2026-09-24-lifecycle-functions-still-over-150-lines.md), the
  [jail-launch TODO](./2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md), and, for row 12,
  an `extract_module` of the lifted `session_entry_from_listing` into `session_list_entries.rs`.
  Row 9 shrinks with the [folding](./2026-09-24-lifecycle-topic-files-to-fold-into-existing-siblings.md)
  and [re-parenting](./2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) moves.
- **The other eight** need a discovery pass for their seams, then `extract_module` plans proven
  with `check --deep`, as the destructure did. The wiring split
  ([#526](https://github.com/uppin/tddy-coder/pull/526)) moves some of them out of the crate whole,
  so re-measure after it lands, before planning.
- Whether ≤ 400 is a target worth a PR of its own is the developer's call. 500 is the budget every
  code-issue record and the file-length gate use.

## Status 2026-10-04: files over 500 lines at the end of `#carve` 16a

The developer deferred the 500-line rule for 16a (2026-10-04): re-points and new declarations may
leave a file over the line, and doc comments are not to be trimmed to get under it. Whole-file line
counts (`wc -l`, tests included) of the files 16a touched that are over 500, base `37c8144f`:

| File (`src/`) | Base | Final | Grown by |
|---|---:|---:|---|
| `connection_service.rs` | 524 | 547 | M0.5: the `mod split_context_from_codebase_host_tests;` declaration with the 20-line doc comment that moved with the test (production lines unchanged: 18 before the first `#[cfg(test)]`) |
| `connection_service/svc_split_context_from_codebase_host.rs` | 803 | 621 | shrank in M0.5 (the host-building test left); 450 production lines, the rest is its second inline test module |

`cursor_cli_spawn.rs` (532) and `split_session.rs` (1,399) are over 500 whole-file lines at the base
too and were not touched. No function grew in 16a.

## Status 2026-10-07: `connection_service.rs` crosses 500 at `#carve` 18/21

`connection_service.rs` went from **497 to 503 production lines** (inline-test-block rule; `loc.py` from the
change history's "LoC assessment", `origin/master` against `#carve` 18/21's head). The growth is three
`mod` declarations and their blank lines: `split_ports`, `attached_initial_prompt` and `svc_split_delegators`.
It is the module-declaration hub, so each node of the stack that adds a module adds to it.

The developer consented to **deferring** the decomposition (2026-10-07, `/pr-wrap` step 3.5), for the reason the
step names for a stack branch: the dependents #534, #535 and #536 also edit this file, and #536 moves modules out
of it, so a restructure here would cascade its rename fallout into all three. The record is
`packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`. Revisit after the stack
lands: if #536 leaves the file under 500, close the record with the final measurement.

The gate's naive count, to the first `#[cfg(test)]`, reads 18 for this file, so it cannot see the crossing; that
is the gap in
[`2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use`](./2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md).
