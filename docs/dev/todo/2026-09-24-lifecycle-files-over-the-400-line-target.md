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
| 12 | `connection_service/session_coordinate_handlers.rs` | 414 | `session_entry_from_listing`, not started |
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
  → session_entry_from_listing into session_coordinate_handlers/session_list_entries.rs: ~300
```

## What would close it

- **Rows 1, 4, 5, 7, 12** close when their functions do: the
  [functions TODO](./2026-09-24-lifecycle-functions-still-over-150-lines.md), the
  [jail-launch TODO](./2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md) and the
  [`session_entry_from_listing` TODO](./2026-09-24-lifecycle-session-entry-from-listing-not-started.md).
  Row 9 shrinks with the [folding](./2026-09-24-lifecycle-topic-files-to-fold-into-existing-siblings.md)
  and [re-parenting](./2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) moves.
- **The other eight** need a discovery pass for their seams, then `extract_module` plans proven
  with `check --deep`, as the destructure did. The wiring split
  ([#526](https://github.com/uppin/tddy-coder/pull/526)) moves some of them out of the crate whole,
  so re-measure after it lands, before planning.
- Whether ≤ 400 is a target worth a PR of its own is the developer's call. 500 is the budget every
  code-issue record and the file-length gate use.
