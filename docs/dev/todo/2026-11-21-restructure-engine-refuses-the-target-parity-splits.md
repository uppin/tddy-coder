# 2026-11-21 — The restructure engine refuses the two `#target-parity` sweep splits

**Category:** Defect
**Source:** `#target-parity` sweep (`refactor/target-parity-sweep`)

Two oversized-file records stay open because the restructure engine refuses the splits that close them,
and the developer chose (2026-11-21) to defer both until the engine is fixed rather than move code by hand:
`packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-hook-follower.md` (526 lines) and
`packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md` (831 lines). Both refusals
came from `restructure check --deep` against a warm index daemon; nothing was written.

## Defect 1 — the module walk ignores `#[path]` on a `mod` declaration

A `move_item` anywhere in `tddy-agent-launch` is refused, whatever its `reexport`:

```text
$ ./dev target/debug/tddy-tools restructure check tmp/restructure/follower.jsonl --deep
0: plan is malformed: `mod tests;` leads to neither `tests.rs` nor `tests/mod.rs` in packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs
Error: 1 finding(s) — see above. Nothing was written.
```

The plan was one `move_item` of `tddy_agent_launch::remote_hook_follower::{HookReportDelivery,
ActivityHookSink, <ActivityHookSink>, <ActivityHookSink as HookSink>}` with
`"name":"remote_hook_delivery","to":"tddy_agent_launch"`. The declaration it trips on is
`packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs.rs:26-27`:
`#[path = "restore_tabs_tests.rs"] mod tests;` — a file the move does not touch. The walk resolves a
`mod` by its name alone and does not read the attribute, so any crate with such a declaration cannot be
moved within.

## Defect 2 — no "extract into function" after `read_fields_through` on a trait-impl method body

The route the code-restructuring skill gives for moving a method body out of its type, applied to
`<WorktreeServiceImpl as WorktreeService>::list_worktree_directory`
(`packages/tddy-worktree-service/src/service.rs`): `read_fields_through` (`expr: self`) and
`extract_method` over the same item-relative range of the whole body, in one `group`:

```text
$ ./dev target/debug/tddy-tools restructure check tmp/restructure/service-c1.jsonl --deep
   note: read_fields_through: rebound every self (2 sites) through `service`
1: group `c1` is refused whole, because a member of it is: op 1: this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)
Error: 1 finding(s) — see above. Nothing was written.
```

The `extract_method` anchor is not carried past the `let service = self;` the first operation inserts,
so its range no longer covers whole statements. It cannot be written to follow the insert: item anchors
resolve against the tree the run opens on, and a range without a `fingerprint` is refused as
`plan is malformed: missing field fingerprint`.

## What would close it

Fix both in `tddy-code-restructuring`, then re-run the two splits through the engine:

1. ~~the `move_item` above (`reexport: outside`), which brings `remote_hook_follower.rs` to about 390
   lines and closes its record~~ — **done by hand** in `#orchestra` 2/9 (#980): the delivery side is in
   `remote_hook_delivery.rs` (184), the follower is at 386 and its record is closed. Defect 1 itself
   stays open, also recorded as `2026-11-21-restructure-ignores-path-attribute-modules.md`;
2. seams B and C of `service.rs` (five trait-method bodies into child modules of `service`) plus the third
   seam the record now names (the resolve helpers), which together take it under 500 and close its
   record.

Acceptance for the engine: a `check --deep` of each plan reports no finding.
