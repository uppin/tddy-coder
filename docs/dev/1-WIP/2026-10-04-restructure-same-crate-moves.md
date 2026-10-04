# Changeset: restructure same-crate moves (`move_item`, `reparent_module`), and the `#carve` 16a items they unblock

**Date**: 2026-10-04
**Status**: 🚧 In Progress (planned and reviewed 2026-10-04; red tests approved; implementation next)
**Type**: Feature (engine operations) + Refactor (the lifecycle moves, applied through them)
**Branch**: `feature/restructure/same-crate-moves`, one PR, based on master `98a9686c` (the merge of #531)

## Initial Discovery

[2026-10-04-restructure-same-crate-moves-initial-discovery.md](./2026-10-04-restructure-same-crate-moves-initial-discovery.md).
State A below is distilled from it.

## Affected Packages

- **`tddy-code-restructuring`**: two new `RefactorKind`s, each in modules of its own; plan-codec rules;
  harness builders and four acceptance suites. `backends/rust.rs` only dispatches.
- **`tddy-tools`**: the `restructure warm` subcommand (client side).
- **`tddy-index-daemon`**, repo-root **`run-index-daemon`**: the script warms the checkout's root.
- **`tddy-session-lifecycle`**: the deferred `#carve` 16a items, applied with the new operations. No
  behaviour change; no public path changes; no consumer edits.
- Docs: `docs/ft/coder/rust-code-restructuring.md`, `warm-code-intelligence-daemon.md`, the
  `code-restructuring` skill and its `plan-schema.md`, package docs of the three touched packages.

## Related Feature Documentation

[Rust code restructuring](../../ft/coder/rust-code-restructuring.md) and the
[PRD](../../ft/coder/1-WIP/PRD-2026-10-04-restructure-same-crate-moves.md).

## Summary

`#carve` 16a deferred four steps because the engine has no same-crate move. This change adds
**`move_item`** (items into an existing module of the same crate, any file) and **`reparent_module`**
(a module file, with its directory, under another parent), fixes three frictions seen in that run, and
then **performs the deferred steps with the new operations**: the moves are the real-world test of the
operations. Where an operation refuses or needs a hand correction, the fix goes into the engine in this
PR, not around it.

## Background

See the PRD. The 16a changeset-history entry
(`packages/tddy-session-lifecycle/docs/changesets/2026-10-04-carve-lifecycle-ports-leaf-topics.md`)
records what was deferred and why; successors #532-#536 carry the same list.

## Prerequisites

Scan per `deferred-work/references/planning-cross-check.md`: the 33 `docs/dev/todo/` entries naming
restructure, lifecycle, extract or carve; the 5 code issues of `tddy-code-restructuring`; the 11 of
`tddy-session-lifecycle`. **No record is claimed by an open PR.** One is claimed by a **merged** one (below).

### Resolved here (the wrap deletes these)

| Entry | What closes it |
|---|---|
| [2026-10-04-restructure-extract-module-cannot-gather-items-from-several-files.md](../todo/2026-10-04-restructure-extract-module-cannot-gather-items-from-several-files.md) | `move_item` ✅ RESOLVED HERE |
| [2026-10-04-restructure-anchors-and-snapshot-friction-seen-in-carve-16a.md](../todo/2026-10-04-restructure-anchors-and-snapshot-friction-seen-in-carve-16a.md) | the path hint, `warm`, and the snapshot routing (its cause is found: below) ✅ RESOLVED HERE |

### Partly resolved (narrowed at the wrap, never deleted)

| Entry | What this change does |
|---|---|
| [2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing.md](../todo/2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing.md) | closes its **item 2** (`snapshot` of an item-anchored plan starts its own rust-analyzer, because there is no `Snapshot` RPC) with that RPC. Items 1 and 3-8 stay: the entry is narrowed, not deleted. This is where the 16a "snapshot crash" came from (reproduced 2026-10-04: with `TDDY_INDEX_SOCKET` set, `snapshot` of a plan with an item anchor still says "acquiring shared rust-analyzer client", and exits with `lsp server exited` where no rust-analyzer is on `PATH`; an empty `files` header is not the trigger) |

| Entry | What this change does |
|---|---|
| [2026-09-24-lifecycle-modules-to-re-parent-by-hand.md](../todo/2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) | closes the rows with an in-crate destination (D7). The three rows whose destination left the crate in #526 (`rpc_activity`, `first_admission_token`, `session_dir_lookup`) stay: they move straight to their receivers in node 17 |

### During (constraints on how the work is done)

| Entry | Constraint |
|---|---|
| [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | `backends/rust.rs` gains **dispatch lines only**; all logic in new modules (D4) |
| [2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md) | the new operations copy byte ranges; a test pins that comments survive |
| [2026-09-24-restructure-apply-leaves-the-lint-gate-red.md](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md) | the moves are linted (`assert_lints_clean`) |
| [2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md](../todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md) | `reparent_module` moves a module's directory children; the same shape, so a helper is shared rather than re-derived |
| [2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md](../todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md) | a `reexport: glob` facade must not dangle |
| [2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need.md](../todo/2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need.md) | the facade's visibility is at least what the moved items need |
| [2026-09-24-lifecycle-files-over-the-400-line-target.md](../todo/2026-09-24-lifecycle-files-over-the-400-line-target.md), [2026-09-24-lifecycle-functions-still-over-150-lines.md](../todo/2026-09-24-lifecycle-functions-still-over-150-lines.md) | the lifecycle moves grow no function; the 500-production-line file budget is respected |

### Answered / unrelated

- [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md): ℹ the same blind spot would hit a `move_item` that strands a sibling; the new check reads bodies too. Left open.
- [2026-09-24-lifecycle-topic-files-to-fold-into-existing-siblings.md](../todo/2026-09-24-lifecycle-topic-files-to-fold-into-existing-siblings.md): ℹ foldable with `reparent_module` later; out of scope.
- Everything else scanned: unrelated.

### Code issues

| Record | Verdict |
|---|---|
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` | ⚠ During: `rust.rs` must not grow beyond dispatch |
| `packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md` | ⚠ During (unclaimed): the facade writer is shared; do not worsen it |
| `packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md` | 🚧 **claimed by #537, which merged 2026-10-02**: the claim is stale (the record says "remainder unowned"), nothing is in flight, so there is no fork to put to the developer. The snapshot investigation may narrow it. The `Claimed by` line is the wrap's to correct |
| `packages/tddy-code-restructuring/docs/code-issues/{dead-code-plan-filehint-modified,oversized-file-test-binary}.md` | — Unrelated |
| the 11 records in `packages/tddy-session-lifecycle/docs/code-issues/` | ⚠ During, as in 16a: no function grows; re-measured at the wrap |

### Conflicting work in `docs/dev/1-WIP/`

`2026-09-17-restructure-refusal-truth-and-authoring-gates.md` (🚧, with its PRD) is an active
changeset on the same engine. Overlap: the `snapshot` subcommand only. It is mostly shipped by the code
and **unwrapped**; this change does not edit or wrap it. It needs its own pass.

## Scope

Commits are ordered engine, then docs, then lifecycle, so the diff can be read in that order (D9).

- [ ] **E0 baseline**: `./test -p tddy-code-restructuring -p tddy-tools -p tddy-index-daemon` once; record
  counts and the **names of every failing test** (live-rust-analyzer suites are `#[ignore]`d or grouped
  by `.config/nextest.toml`; list what ran).
- [ ] **E1 `move_item`**: `RefactorKind::MoveItem`; codec rules; a destination-and-name preflight that
  needs no server; the authored move (extract item text by byte range, insert into the destination,
  restore the moved items' `use` needs, widen visibility minimally and report it, re-point callers from
  the server's reference set or leave a `reexport` facade); logic in `backends/rust/item_move/`.
- [ ] **E2 `reparent_module`**: `RefactorKind::ReparentModule`; `git mv` of the file and directory; both
  parent forms on both ends; `mod` declaration moved with its visibility and attributes; path re-pointing
  and `super::` rebasing; logic in `backends/rust/module_reparent/`.
- [ ] **E3 ergonomics**: the repo-root hint in `owning_package`; `restructure warm` (tddy-tools client,
  `RestructureCommand::Warm`) and `run-index-daemon` warming by default (`--no-warm`); a `Snapshot` RPC
  on `code_index.CodeIndexService` (proto change, regenerated code under the drift gate
  `scripts/generated-code.sh`), `index_client` calling it and `answered_without_an_index` no longer
  keeping an item-anchored `snapshot` in process.
- [ ] **E4 docs**: `plan-schema.md` (op table; the sentence "Rust has no whole-symbol move" is replaced),
  `SKILL.md`, `rust-code-restructuring.md`, `warm-code-intelligence-daemon.md`, package docs.
- [ ] **L0 lifecycle baseline**: `./test -p tddy-session-lifecycle`: **575 passed, the 22 failures by name
  (list in the 16a history entry), 1 ignored**, re-run on this branch before the first move.
- [ ] **L1 M0.2 (T4 half)**: `write_claude_hooks_settings`, `resolve_start_session_claude_binary` into the
  T4 module, via `move_item`.
- [ ] **L2 M0.1**: `peer_session_answer`: `extract_module` for the first item, `move_item` for the other
  three (`reexport: none`), `reparent_module` to the right parent if the first extract landed under the
  wrong file.
- [ ] **L3 M0.6**: `SessionStdioEndpoint` to T1, `ExecToolRoute` beside `LocalExecTools`, via `move_item`.
- [ ] **L4 M0.4**: `reparent_module` for the rows in D7.
- [ ] **L5 gate**: after the last plan, once: `cargo fmt --check`, `cargo clippy -p <touched> --all-targets
  -D warnings`, `./test -p tddy-session-lifecycle` held to L0 by name, `restructure verify --against
  <ref before the first lifecycle plan>`, the comment-line multiset, and the acceptance checks below.
- [ ] **Code Quality**: scoped clippy and fmt on every touched package; no function over 150 lines grows; new
  files stay under 500 production lines.

## Technical changes

### Plan schema (State B)

```jsonl
{"op":"move_item","anchor":{"kind":"items","file":"…/split_start.rs","items":["pkg::connection_service::split_start::peer_has_no_such_session"],"fingerprints":["sha256:…"]},"to":"pkg::connection_service::peer_session_answer","reexport":"none"}
{"op":"reparent_module","anchor":{"kind":"items","file":"…/connection_service/svc_materialize_staged_attachment.rs","items":["pkg::connection_service::svc_materialize_staged_attachment::split_claude_cli_start"],"fingerprints":["sha256:…"]},"to":"pkg::connection_service::split_start","reexport":"none"}
```

| Field | `move_item` | `reparent_module` |
|---|---|---|
| `anchor` | `items` anchor (or a single `item` anchor), one file, contiguous run | an `items` anchor on the module's **`mod` declaration** in its old parent: the same engine-emitted kind as `move_item`, so the two are written alike |
| `to` | existing module path (D1) | existing parent module path |
| `reexport` | `glob` / `named` leave a facade and re-point nothing; `none`/absent re-points every caller (D3) | `glob` leaves `pub use`; `none`/absent re-points |

Refusals (static where the text answers them, so no server is spawned). `move_item`: missing destination;
name taken in the destination; destination is the item's own module; item inside an `impl` block (refused
when the server resolves it). `reparent_module`: missing destination; module name taken by the new parent;
destination inside the moved module; `#[path]` module. **Widening is not a refusal**: a private item the
moved code reaches is widened where it stays, and a moved private item its callers need is widened where
it lands, only as far as the callers need (the `extract_module` precedent).

### Code layout

- `plan.rs`: `MoveItem`, `ReparentModule` in `RefactorKind` (with the `moves_across_crates`-style predicate
  they do **not** satisfy: they stay inside one crate).
- `plan/codec.rs`: `to` required; `reexport` allowed on both; `also` and `to_file` refused.
- `backends/rust.rs`: supported-kinds entries and two dispatch blocks next to `:1151`; nothing else.
- `backends/rust/item_move/` and `backends/rust/module_reparent/`: the preflight, the survey (reusing
  `crate_move/survey.rs` for paths and `crate_move/moving/facade_writer.rs` for facades), the edit builders.
- `tests/harness/same_crate.rs` (new): fixture builders, so `tests/harness/mod.rs` (2,603 lines) is not grown.
- `tddy-tools`: `RestructureCommand::Warm` in `restructure_args.rs`, dial in `index_client.rs`.
- `run-index-daemon`: after the readiness line, `tddy-tools restructure warm` for the checkout root.

## Testing Plan

Fluent-tests style throughout (`.agents/skills/fluent-tests/`): Given/When/Then, named helpers, one
behaviour per test. Engine suites run against a live rust-analyzer over generated fixture workspaces
(`./test -p tddy-code-restructuring`, one server at a time); `assert_compiles_with_its_tests` and
`assert_lints_clean` are the oracles.

### Acceptance and unit tests (written first, failing)

- [ ] `packages/tddy-code-restructuring/tests/move_item_acceptance.rs`: 13 tests (+ `tests/same_crate/mod.rs`, the shared builders).
- [ ] `packages/tddy-code-restructuring/tests/reparent_module_acceptance.rs`: 12 tests.
- [ ] `packages/tddy-code-restructuring/tests/anchors_package_relative_path.rs`: 2 tests (the second, "still refuses it", is a guard that passes today and must keep passing).
- [ ] `packages/tddy-tools/tests/index_daemon_client_acceptance.rs` (4 tests added): a snapshot of an item-anchored plan goes to the daemon; a snapshot of a plan with no item anchors stays in process (a guard, passes today); `warm` with no daemon refuses naming `./run-index-daemon`; and one `#[ignore]`d production test (the daemon reports the root warm after `warm`).

### Lifecycle acceptance (the "real test")

There is no new Rust shape test: the 2026-09-25 "no shape tests" decision stands (D8). The moves **are**
the acceptance, and they are checked by the L5 gate plus these scripted checks, run after L4:

- **A1** `grep -rn "peer_has_no_such_session\|split_pairing\|resolve_worktree_root_for_session\|resolve_exec_tool_worktree"`: each free function is defined once, in `peer_session_answer`.
- **A2** no `T3` file imports `crate::connection_service::split_start` or `workspace_session` or `svc_resolve_os_user` for those four.
- **A3** `seeded_clone_guard.rs` defines neither `SessionStdioEndpoint` nor `ExecToolRoute`.
- **A4** `write_claude_hooks_settings` and `resolve_start_session_claude_binary` live in the T4 module and `hooks_and_urls.rs` no longer defines them.
- **A5** each D7 module sits under its destination parent: `find` shows the new path, the old directory holds no stale child.
- **A6** the baseline holds by test name; `cargo check --all-targets` on `tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-telegram-control`, `tddy-daemon`, `tddy-desktop`.
- **A7** no hand edit survives unexplained: every correction made after an engine apply is a `docs/dev/todo/` entry **or an engine fix in this PR**.

## Decisions & trade-offs

Recommended answers are first; **D1, D2, D3, D7 need your review before `/green`.**

- **D1: destination must exist, or the move creates it?** *Recommended: it must exist.* A move that creates a
  file is a `create_file` by another name (`SKILL.md`: "No `create_file`"). A new module is made by
  `extract_module` (from the first item) or by an earlier `reparent_module`, so M0.1 composes from
  operations that exist. Cost: M0.1 is three steps, not one. Alternative: `move_item` creates the module
  (`create: true`): shorter plans, a wider operation.
- **D2: vocabulary.** *Recommended: two new kinds.* `move_symbol` and `move_file` are claimed by the
  TypeScript backend ("TypeScript Move to file"); giving them a Rust meaning would make one name mean two
  transformations. Alternative: back `move_symbol` for Rust.
- **D3: what `reexport: none` means.** *Recommended: re-point callers.* Unlike `extract_module`, where `none`
  refuses when another file references the items, a move's purpose is a new home with callers following,
  and M0.1's acceptance forbids a facade (T3 callers must name the defining module). Cost: the two ops
  read the same field differently; documented in `plan-schema.md`.
- **D4: where the code lives.** New modules under `backends/rust/`, dispatch-only edits in `rust.rs`
  (open code issue + backlog entry). Cost: two more module trees to learn.
- **D5: snapshot.** *Recommended: add the `Snapshot` RPC.* The 16a "crash" is reproduced and understood: it is
  the documented gap in the live-plans backlog entry (item 2), not an empty-header bug. `snapshot` re-resolves
  an item-anchored plan's hints, which needs a server, and `answered_without_an_index` keeps it in process
  because there is no RPC, so a warm daemon is ignored and a machine with no rust-analyzer on `PATH` gets
  `lsp server exited`. Cost: a proto change (generated code and its drift gate) in a PR that is already large.
  Alternative: leave it and file nothing new (the entry already records it); the red test then moves to the backlog.
- **D6: warming.** *Recommended: `run-index-daemon` warms by default.* The first request is a minutes-long
  silent wait otherwise. Cost: starting the daemon now loads a graph unasked, hence `--no-warm`.
  Alternative: only report indexing in `--status`.
- **D7: M0.4 destinations (consent given 2026-10-04; destinations are yours to confirm).**
  | Module (under `src/connection_service/`) | New parent |
  |---|---|
  | `svc_materialize_staged_attachment/split_claude_cli_start.rs` | `split_start` |
  | `svc_resolve_os_user/session_attachment_materialization.rs` | `svc_materialize_staged_attachment` |
  | `svc_resolve_os_user/local_exec_tool_dispatch.rs` | `local_exec_tools` |
  | `svc_resolve_listed_worktree/session_room_opening.rs` | `svc_ensure_session_room_for_agents` |
  | `svc_turn_end_reporter/jail_env_builders.rs` | `svc_start_sandboxed_claude_cli_session` |
  | `svc_resolve_tddy_tools_path/svc_host_builders/presenter_observer_spawn.rs` | `src/presenter_observer_task` |
  | `svc_resolve_tddy_tools_path/svc_host_builders.rs` | `connection_service` (beside the struct) |
  | **not re-parented here** (destination left the crate in #526): `rpc_activity`, `first_admission_token`, `session_dir_lookup` | node 17, `move_module_to_crate` |
- **D8: "as a real test".** The moves are the acceptance; no shape test (the 2026-09-25 decision stands).
  An engine refusal during L1-L4 **stops the moves and is fixed in the engine in this PR**; a hand edit
  after an apply is a TODO or an engine fix, never silent.
- **D9: one PR.** *Chosen by the developer.* The diff mixes a tool change with a roughly 1k-line mechanical
  move. Mitigation: commit order engine, docs, lifecycle; each commit passes its own scoped gate, so
  review can go commit by commit. If review proves too heavy, the lifecycle commits are cut into a stacked
  PR without rework.
- **D10: the 2026-09-17 restructure changeset** is left alone (see Prerequisites).

## Validation Results

_(filled by `/validate-changes` and `/pr-wrap`)_

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Acceptance tests written and reviewed (approved as written, 2026-10-04)
- [x] Red-phase tests written (25 engine tests verified red: `unknown variant move_item` / `reparent_module`; the anchors test fails on the message; two guard tests pass today by design)
- [x] USER REVIEW: D1, D2, D3, D7 accepted as recommended; D5 decided: **the `Snapshot` RPC is in this PR** (2026-10-04)

## Final Checklist

- [ ] E0 baseline recorded; L0 baseline re-run on this branch
- [ ] E1-E4 delivered; every red test passes
- [ ] L1-L5 delivered through the engine; A1-A7 hold
- [ ] Scoped clippy/fmt clean on every touched package; no function grew
- [ ] Docs wrapped (`/wrap-context-docs`): feature doc, skill, package docs, one history entry per package
- [ ] Backlog reconciled as in Prerequisites; the stale `Claimed by` corrected
