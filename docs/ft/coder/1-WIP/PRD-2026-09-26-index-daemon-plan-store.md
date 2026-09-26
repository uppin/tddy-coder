# Index daemon plan store — plans are loaded once, executed by reference, and flushed back - PRD

**Date**: 2026-09-26
**PRD Type**: Enhancement + Bug Fix

## Affected Features

- **Primary Feature**: [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md) — new
  `LoadPlans` / `UnloadPlans` / `ListPlans` RPCs; `Check`, `Apply` and `PlanStatus` read a loaded
  plan instead of the file; run state per plan.
- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `restructure load`,
  `unload`, `plans`; the stable op `id`; the plan stops being a never-rewritten command log.
- **Related surface**: `.agents/skills/code-restructuring/` — the authoring loop (load, check, apply)
  and `references/plan-schema.md`.

## Summary

Every `Check`, `Apply` and `PlanStatus` re-reads the JSONL from disk, so what executes is whatever
the file says at that moment — including anchors that earlier operations of the same plan already
made stale. This PRD makes the executor work from a **plan store**: plans are loaded into memory once,
indexed by plan and by a stable operation `id`, executed by reference, and **written back** as they
change — after changes (eventually) and always on unload and on exit.

The store is a library type, so the same code serves a long-lived daemon, which holds plans across
requests, and a one-shot run, which loads, applies and flushes inside one process.

## Background

- `packages/tddy-index-daemon/src/apply.rs:44` — `Plan::parse(&read_to_string(options.plan()?)?)`
  on every run; no plan state anywhere in the daemon.
- `references/plan-schema.md` states "The plan is a **command log** and is never rewritten."
  Anchors written against the snapshot are translated by the in-memory ledger during a run and
  never saved, so a `--resume` after a crash, or `--from N`, re-reads anchors the tree has moved
  past.
- Code issue
  [`stale-repo-scoped-restructure-state-apply.md`](../../../packages/tddy-index-daemon/docs/code-issues/stale-repo-scoped-restructure-state-apply.md):
  the daemon's apply uses `StatePaths::under(root)`, so a plan that never ran is refused with
  "a journal already exists — pass --resume" because a different plan completed under that root.

## Proposed Changes

### What's Changing

1. **Stable operation ids.** Every op carries `"id"`. `load` assigns one to an op without it and
   the next flush writes it back; two ops with one id are refused as malformed. Journal, events and
   `--from` name ops by id (the index is still shown).
2. **`PlanStore`** in `tddy-code-restructuring`: `load(paths)`, `unload(paths)`, `unload_all()`,
   `list()`, `get(plan)`, lookup by `(plan, op id)`. Keyed by the plan's workspace-relative path
   under one root.
3. **The executor references the store.** `Check`, `Apply` and `PlanStatus` resolve their plan
   through the store; the file is read only by `load`. `Apply` on a plan that is not loaded **loads
   it implicitly** and leaves it loaded.
4. **The applied plan stays current.** After each applied operation the store rewrites the pending
   ops of *that plan*: item-anchor hints and relative ranges translated through the operation's
   edits, fingerprints of items the operation edited recomputed, and `file` hints following a file
   the operation moved (the ledger's renames). A resumed or `--from` run therefore reads anchors that
   match the tree it runs on.
5. **Flush.** A changed plan is marked dirty and written back (temp file + rename) shortly after it
   changes — eventually consistent — and synchronously at the end of a run, on `unload`, and on
   shutdown (`^C`/`SIGTERM` in served mode, process exit in single-shot mode). A flush never
   clobbers a plan whose file changed on disk since it was loaded: it is refused, naming the plan and
   saying to unload and reload.
6. **RPCs** on `code_index.CodeIndexService`: `LoadPlans{workspace_root, plans[]}`,
   `UnloadPlans{workspace_root, plans[], all}`, `ListPlans{workspace_root}`, each answering the
   loaded plans with their op counts and journal state.
7. **CLI**: `tddy-tools restructure load <plan…>`, `unload <plan…> | --all`, `plans`; the same
   subcommands on the `tddy-index-daemon` single-shot command line. Without a daemon, `load` /
   `unload` / `plans` are refused as needing one (`TDDY_INDEX_SOCKET`); `apply` and `check` still
   work in process, over a store that lives for that one invocation.
8. **Plan-scoped run state** in the daemon's apply loop (`StatePaths::for_plan`), closing the code
   issue.

### What's Staying the Same

- Plans on disk remain the source a human edits and reviews; the store is a cache of them that
  writes back, never a replacement.
- Operation semantics, refusal classes, the ledger inside a run, the journal's write-ahead order.
- Plans the daemon holds for *other* plans are not refreshed by this PR — that is its successor's.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: new `plan_store` module; `plan.rs` (`id`, serialisation back to JSONL
  preserving op order and unknown-free fields); `runner` (run by reference; per-op refresh of the
  applied plan).
- `tddy-index-daemon`: proto (three RPCs), `service.rs`, `apply.rs` (store-backed, `for_plan`),
  `serve.rs` / `main.rs` (flush on shutdown), `cli.rs` (subcommands), `index.rs` (store per root).
- `tddy-tools`: `index_client.rs` routes the new subcommands.

### User Impact

The plan file on disk tells the truth after every operation. Resume, `--from` and the next
`check` see anchors that match the tree, and a second plan under one root is no longer refused.

## Acceptance Criteria

- [ ] `load` of a plan whose ops lack ids assigns them, and the flushed file carries them.
- [ ] Two ops with one id are refused as malformed on load.
- [ ] Editing the plan file after `load` does not change what `Apply` executes; `Apply` runs the
      loaded ops.
- [ ] After op 1 of a two-op plan inserts lines above op 2's item, the flushed file's op 2 carries the
      updated hint; a `--resume` from op 2 applies the same edit a clean run does.
- [ ] After an op edits the item op 2 is anchored in, op 2's fingerprint is recomputed and op 2
      applies.
- [ ] A dirty plan is on disk within the flush interval, at run end, on unload, and after `SIGTERM`
      to a served daemon.
- [ ] A flush onto a plan file changed on disk since load is refused, naming the plan; the file is
      untouched.
- [ ] `Apply` of an unloaded plan loads it; `ListPlans` then lists it.
- [ ] `UnloadPlans{all}` flushes and drops every plan of that root.
- [ ] A second plan under a root where a first plan completed through the daemon applies without
      `--resume` (code issue closed).
- [ ] `restructure load` without a daemon is refused as needing one; `restructure apply` without a
      daemon still applies and flushes.

## References

- [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md)
- [Rust code restructuring](../rust-code-restructuring.md)
