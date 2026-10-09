# Changeset: `restructure anchors` says why its server never started; static `check` verifies an item anchor's module

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Bug fix and hygiene. It touches the `tddy-lsp` error surface, a static-check rule, the plan
header and removes dead code. It adds no new operation.
**Stack**: `#reshape` 12/19, branch `feature/reshape/anchors-outline`, green wave 1.
PR title: `fix(code-restructuring,lsp): anchors names why its server never started; static check verifies item modules (#reshape 12/19)`.
Base in the linear stack: `feature/reshape/move-item-paths` (K=11). **Real edges: none.** The node
consumes no other node's behaviour, and no node consumes its behaviour. It is in the line only because
`gh stack` needs a line.

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-09-reshape-anchors-outline-initial-discovery.md). Exploration 1 is the
whole-work discovery. Exploration 2 is this node's: the dead `anchor_for` path, 103/3 warm daemon
answers, the dropped startup failure, the half-done static-check todo, and the dead `modified` field.

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no record
claimed by an open PR in this node's path. The empty-outline record says `**Claimed by:** none`.
**There is no 🚧 claimed issue in the path, so no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [broken-restructure-anchors-empty-outline.md](../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md) | ✅ **RESOLVED HERE** | Repo-scale measurement: cold inside and outside `./dev`, warm, and warm again (M1). The cold path's `lsp server exited` becomes a refusal that names the program and the reason (M2–M3). The dead `places_of` path its diagnosis names is deleted (M5). **Deleted at wrap**, with the final measurement in the change-history entry. If M1's warm or in-shell cold leg fails, stop and re-plan |
| [dead-code-plan-filehint-modified.md](../../../packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md) | ✅ **RESOLVED HERE** | The field, its writer, `rfc3339` and its test re-export are removed (M6). **Deleted at wrap**, with the final `grep` recorded |
| [2026-10-02-static-check-cannot-verify-item-anchors.md](../todo/2026-10-02-static-check-cannot-verify-item-anchors.md) | ✅ **RESOLVED HERE** | Static `check` verifies the crate/module prefix (M4). Its other half, the range shape, is already checked at parse by `Anchor::validate` (`plan.rs:65-110`). **Deleted at wrap** |
| [2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md) | — Unrelated (reference) | The v1 range rebase stays deferred. `plan_store/refresh.rs` is not touched |
| [2026-10-02-the-daemon-anchors-path-resolves-an-item-twice.md](../todo/2026-10-02-the-daemon-anchors-path-resolves-an-item-twice.md) | — Unrelated (reference) | `runner::item_anchors`' signature is not changed here, so the entry stays as written |

## Affected Packages

- **`tddy-lsp`**:
  - the registry reports why a server it spawned never came up (`src/registry.rs`, `src/server_body.rs`, `src/error.rs`);
  - a new `fake_lsp` mode (`tests/bin/fake_lsp.rs`);
  - new and changed tests.
  - Package docs: `docs/` has only `workspace-root.md`, which does not describe errors. Nothing to update.
- **`tddy-index-daemon`**: `src/status.rs` (`status_of_lsp` gains one arm), plus a unit test.
- **`tddy-code-restructuring`**:
  - `src/item_anchor.rs` and a new `src/item_anchor/prefix.rs`;
  - `src/backends/rust/item_path.rs`, `src/backends/rust.rs`, `src/registry.rs`;
  - `src/runner/entry_points/check_entry_points.rs`;
  - `src/plan.rs`, `src/plan/codec.rs`, `src/plan/codec/file_hint.rs`;
  - tests.
  - Package docs, at wrap through this changeset: `docs/item-anchors.md` (§ "Static `check` cannot examine item anchors", § Plan header) and `docs/plan-store.md:73`.
- **Dev tooling**: `.agents/skills/code-restructuring/references/plan-schema.md:14,17` (drop `modified`), edited in this PR.

## Related Feature Documentation

- PRD: [PRD-2026-10-09-reshape-anchors-outline.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-anchors-outline.md)
- Feature: [Rust code restructuring](../../ft/coder/rust-code-restructuring.md). At wrap:
  - § Item anchors (static refusals);
  - § Plan format `:91` and § Live plans `:163` (`modified`);
  - § Known limitations `:865-867` (narrowed).

## Summary

- A cold run that cannot bring rust-analyzer up now says which program it ran and why it never came up,
  where today it prints the bare `lsp server exited`.
- The empty-outline record is closed on a repo-scale measurement.
- Static `check` refuses an item anchor whose crate or module does not match its file, in the deep
  resolver's own words.
- The never-read `modified` hint leaves the plan header.
- The superseded `anchor_for` path, about 145 lines, is deleted.

## Background

See the PRD's Background. The facts that decide the design, in short:

1. **`places_of` is dead.** Since #537 (`ddd599028`) `anchors` runs
   `items_anchor → resolve_item → outline_of → settled_outline → walk`. `anchor_for` has no production
   caller and three test callers.
2. **The warm path works at repo scale.** Index-daemon logs from 2026-10-07/08 show 103 answers and 3
   refusals, all of them request mistakes. The bridged backend learns quiescence from
   `notifications_to_fold` (`backends/lsp_bridge.rs:104-107`).
3. **`tddy-lsp` drops the startup failure.** `spawn_service` turns a dropped one-shot into
   `ServerExited` (`tddy-lsp/src/registry.rs:195-199`). The `TaskStatus::Failed { message }` that
   `server_body.rs:114-123/213-232` produced is lost.
   - `LspError::ServerNotFound` exists (`error.rs:10-12`) and is never constructed. The daemon already
     maps it to `failed_precondition` (`tddy-index-daemon/src/status.rs:133`), and a test pins that
     mapping (`:312-322`).
4. **Static `check` emits one finding per item-anchored operation**
   (`check_entry_points.rs:364-381`) without looking inside the anchor. The prefix rule
   (`backends/rust/item_path.rs:347-370`) needs no server, but it is reachable only behind one.
5. **`FileHint.modified` has a writer and no reader**, and old headers parse without it: `HintedHeader`
   and `FileHint` carry no `deny_unknown_fields`.

## Responsibility

- **`tddy-lsp`: a server that never came up is refused with its cause.**
  - A spawn error of kind `NotFound` becomes `LspError::ServerNotFound("<program>: <os error>")`.
  - Any other spawn error, an exit before the handshake (with its exit status), or a failed
    `initialize` becomes the new `LspError::ServerNotStarted { program, reason }`.
  - The body hands the typed error to the registry over its one-shot.
- **`tddy-index-daemon`:** `status_of_lsp` maps `ServerNotStarted` to `unavailable`.
- **`tddy-code-restructuring`, the prefix rule:** it moves into `item_anchor/prefix.rs`, the
  language-independent half. Both the Rust resolver and the static check call it.
- **Static `check`:** it refuses a foreign prefix per operation, and for an operation whose prefixes are
  sound it keeps the "only a deep check can resolve" finding, reworded.
- **Remove `FileHint.modified`:** the mtime read, `rfc3339`, the `#[cfg(test)]` re-export and the two
  `TODO(sharpen)` markers they carried.
- **Delete the dead `anchor_for` path and re-point its three test callers** (F6).
- **Measure at repo scale and record the result (M1).** This gates the rest of the node.

### The rules (the contract)

**R1 — which failures are "never started".** `get_or_spawn` → `spawn_service` → the server task. Each
failure before the client is handed back is classified as follows:

| What happened | Error | Display |
|---|---|---|
| `Command::spawn` failed with `io::ErrorKind::NotFound` | `ServerNotFound(format!("{program}: {err}"))` | `language server not found: rust-analyzer: No such file or directory (os error 2)` |
| `Command::spawn` failed otherwise (e.g. `PermissionDenied`) | `ServerNotStarted { program, reason: format!("could not be spawned: {err}") }` | `` language server `rust-analyzer` did not start: could not be spawned: Permission denied (os error 13) `` |
| the child exited before `initialize` answered | `ServerNotStarted { program, reason: format!("exited before the initialize handshake completed ({status})") }`, where `status` is `ExitStatus`'s Display | `` language server `…/fake_lsp` did not start: exited before the initialize handshake completed (exit status: 0) `` |
| `initialize` was answered with an error, or failed otherwise | `ServerNotStarted { program, reason: format!("the initialize handshake failed: {err}") }` | `` … did not start: the initialize handshake failed: lsp server error -32603: refused `` |
| the output channel is missing (a host wiring defect) | `ServerNotStarted { program, reason: "its task has no output channel".into() }` | — |

`program` is `LaunchSpec::program`, verbatim. **No lookup and no fallback:** the program is still run
as configured. A server that came up and later died keeps `ServerExited`, unchanged. `SPAWN_TIMEOUT`
expiring keeps `Timeout`, unchanged. The spawn observer is told of the process end exactly as today
(`report_end`).

**R2 — the prefix rule (one function, two callers).**
`item_anchor::prefix::segments_below(item, module, file)` is today's `segments_below`, moved verbatim
with its two refusals:
- "`{item}` is not in {file}, which is module `{module}`";
- "`{item}` names the module {file} is, not an item in it".

`item_anchor::prefix::refuse_a_foreign_prefix(root, anchor)` reads `module_path_of(root, file)`. It
applies `segments_below` to the `item` of an `Item` anchor, or to each `items` entry of an `Items`
anchor, in order, and returns the first refusal. For a `Symbol` or `Range` anchor it returns `Ok(())`.
`module_path_of`'s own refusals (outside `src/`, no package) pass through unchanged. All of these are
`RestructureError::MalformedPlan(reason)`.

**R3 — what a static check reports for an item-anchored operation.** For each operation whose anchors
include an `item` or `items` anchor (the `anchor` and every `also` anchor, through `op.anchors()`):
- **(a)** the first `refuse_a_foreign_prefix` refusal over its anchors becomes **one finding** whose
  `detail` is the refusal's `reason`, i.e. its `Display` without `plan is malformed: `. It replaces (b)
  for that operation, so there is one finding per operation, never two.
- **(b)** otherwise, one finding whose detail is exactly:

  `` {op:?} in `{file}` anchors by item, which only a deep check can resolve: its module prefix matches the file, and whether the item is there, unambiguous and unchanged needs `check --deep` ``

  It keeps the pinned phrases "anchors by item, which only a deep check can resolve" and
  "check --deep".
- Any other error, an I/O failure reading a manifest, is the check's error, as today.
- Group merging (`one_finding_per_refused_group`) applies unchanged.
- **The deep path is unchanged:** `check --deep` and `apply` still refuse a foreign prefix through
  `resolve_item`, which now calls R2's `segments_below`. Their output gains the `plan is malformed: `
  prefix as today.

**R4 — the v2 header.** `FileHint` is `{ sha256: String }`. `hint_of` reads only the hash. A header that
carries `"modified"` parses, the key is ignored, and the next rewrite of the header omits it. Nothing
else in the plan format changes.

**R5 — deletions (F2, developer-consented).**
- `LanguageBackend::anchor_for` (`registry.rs:91-107`).
- In `backends/rust.rs`: `RustBackend::anchor_for` (`:1190-1205`), `anchor_opening` (`:1409-1446`),
  `module_outline` (`:1727-1739`), `OutlineItem` with its `impl` (`:2353-2378`), `places_of`
  (`:2377-2399`) and `refuse_non_adjacent` (`:2401-2418`).
- `attached_trivia_starts_at` and `outline_is_empty` stay (other callers).
- An `items` anchor's adjacency refusal is unaffected. It lives in `item_anchor::covering_run`
  (`item_anchor.rs:261-311`).

## Boundaries

- **No change to how a language server is launched:** no `PATH` search, no toolchain pinning, and no
  fallback to another binary (proposed todo `…-registry-launches-rust-analyzer-unpinned`).
- **No change to the outline wait** (`settled_outline`, `outline_is_the_servers_answer`). A server
  that never sends `experimental/serverStatus` still waits until its caller cancels.
- **No change to `anchors`' arguments, output or request-mistake refusals.** In particular, the
  trait-impl spelling hint is a proposed todo.
- **No change to `runner::item_anchors`' signature** (that is the resolve-twice todo), to
  `snapshot`/`refresh.rs` or to v1 plans.
- **No static verdict on presence, ambiguity, fingerprint or relative-range length.** Those need the
  item.
- **`check_plan` does not grow.** It is on `#reshape` 16's >60-line list (115 lines). The one call at
  `:237` is replaced by a call to the new function, line for line.
- **No `.restructure/` for `anchors`**, so the cold spawn record stays unwritten for it (proposed todo).
- **No `RefactorOp` field, no new operation, no wire message.** `AnchorsRequest`/`AnchorsResponse`
  are unchanged.

## Dependencies

This node has no parent: it consumes no behaviour from any other `#reshape` node.

Textual overlap only, for rebases, with no consumed signature:
- **`apply-robust` (K=10)** also edits `tddy-lsp` (a `ProcessStart` field and a `fake_lsp` mode) and
  `tddy-index-daemon/src/status.rs` (`status_of`). This node edits `fake_lsp.rs`'s mode list and
  `status_of_lsp`, a different function in the same file. Whichever node greens second rebases over the
  other's lines.
- **Nodes 1–11** share `backends/rust.rs` with this node. This node only **deletes** lines there.
- **`rust-backend-split` (K=17)** will find `rust.rs` about 125 lines shorter. That is not an edge:
  node 17 does not depend on this deletion, it only benefits from it.

## Draft PR contract

Published with the wave-2 contract commit, as the first push of this PR. **Owned surface:**

- `tddy-lsp`:
  - `LspError::ServerNotStarted { program: String, reason: String }`, with
    `` #[error("language server `{program}` did not start: {reason}")] ``;
  - `LspError::ServerNotFound(String)` is now **constructed**, carrying `"{program}: {os error}"`;
  - `LspServerBody::client_tx: oneshot::Sender<Result<Arc<LspClient>, LspError>>`, changed from
    `oneshot::Sender<Arc<LspClient>>`; the two `server_body_test.rs` call sites compile unchanged by
    inference;
  - `fake_lsp` mode `--refuses-initialize`, which answers `initialize` with error `-32603 refused`.
- `tddy-index-daemon`: `status_of_lsp(&LspError::ServerNotStarted { .. }) -> Status::unavailable(message)`.
- `tddy-code-restructuring`:
  - `item_anchor::prefix::segments_below(item: &ItemPath, module: &[String], file: &str) -> Result<Vec<ItemSegment>>` (`pub(crate)`, moved);
  - `item_anchor::prefix::refuse_a_foreign_prefix(root: &Path, anchor: &Anchor) -> Result<()>` (`pub(crate)`);
  - `check_entry_points::item_anchor_findings(plan: &Plan, root: &Path) -> Result<Vec<Finding>>` (private, replacing `unresolvable_without_a_server`);
  - `FileHint { pub sha256: String }` (public, loses `modified`);
  - **removed:** `LanguageBackend::anchor_for` and the R5 list, and `plan::codec::rfc3339`.
- The contract commit stubs `item_anchor_findings` and `refuse_a_foreign_prefix` with `todo!()`-free
  bodies that keep today's behaviour: every item-anchored operation still gets today's finding. That
  makes tests 8–14 fail on assertions, not on panics.
- Failing tests: 1–4, 6–14, 16 and 17 (red); 5 and 18–21 are green pins.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes. It has no parent and needs nothing from another node.
**Concurrent with:**
- `feature/reshape/widen-same-crate`
- `feature/reshape/multi-seam-extract`
- `feature/reshape/tidy-facades`
- `feature/reshape/extract-method-clean`
- `feature/reshape/move-children`
- `feature/reshape/methods-leave-type`
- `feature/reshape/move-widen`
- `feature/reshape/move-grouped-use`
- `feature/reshape/new-crate`
- `feature/reshape/apply-robust`
- `feature/reshape/move-item-paths`

They collide only textually (`backends/rust.rs`; with `apply-robust`, also `tddy-lsp` and `status.rs`).

**Blocks:** none.

Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`,
`17→18`, `6→18`, `4→19`, `17→19`. **None of them touches node 12.**

## Successor PRs

None. No node depends on this one.

## Scope

- [ ] **M1 measurement**: repo-scale cold (inside and outside `./dev`) and warm runs, recorded under Validation results
- [ ] **`tddy-lsp`**: typed startup failure (R1), the `fake_lsp` mode, tests
- [ ] **`tddy-index-daemon`**: `status_of_lsp` arm and its test
- [ ] **Prefix rule** moved into `item_anchor/prefix.rs` (R2); the Rust resolver calls it
- [ ] **Static check** (R3)
- [ ] **Dead path deleted** (R5); 3 tests re-pointed (F6)
- [ ] **`FileHint.modified` removed** (R4); skill `plan-schema.md` updated
- [ ] **Package documentation** at wrap (list under Affected Packages); both code issues and the todo deleted with their final measurement
- [ ] **Testing**: `./test -p tddy-code-restructuring -p tddy-lsp -p tddy-index-daemon`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check --all-targets` for the three packages; `cargo clippy -p <each> --all-targets -- -D warnings`; `cargo fmt`; `item_anchor.rs` production lines stay ≤ 500, which is why the rule goes into a child module

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- **`tddy-lsp`**:
  - `LspServerBody` returns `TaskStatus::Failed { message }` on a spawn error, an exit before the
    handshake, or a failed `initialize` (`server_body.rs:114-123, 145-153, 213-232`), and drops
    `client_tx`;
  - `spawn_service` maps the dropped one-shot to `ServerExited` (`registry.rs:195-199`);
  - `ServerNotFound` is never constructed;
  - `spawn_observer_test.rs:151-170` pins `ServerExited` for `--exit-immediately`.
- **`tddy-index-daemon`**: `status_of_lsp` maps `ServerExited` → `unavailable` and `ServerNotFound` →
  `failed_precondition` (`status.rs:128-148`).
- **`tddy-code-restructuring`**:
  - `segments_below` is private to `backends/rust/item_path.rs:347-370`;
  - static `check` → `unresolvable_without_a_server` (`check_entry_points.rs:233-239, 364-381`);
  - `FileHint.modified` written by `hint_of` (`plan/codec/file_hint.rs:7-15`) with `rfc3339`
    (`:20-47`), re-exported for tests at `plan/codec.rs:224-227`;
  - `anchor_for` and its helpers are dead (R5).

### State B (Target)

R1–R5 hold. A cold run outside the dev shell prints
`Error: rust-analyzer LSP / Caused by: language server not found: rust-analyzer: No such file or directory (os error 2)`.
A static check of a plan whose item anchor names the wrong module fails with the deep resolver's words.
Plan headers carry only hashes. `rust.rs` is about 125 lines shorter.

### Delta (What's Changing)

#### `tddy-lsp`
- `src/error.rs`: the `ServerNotStarted` variant, and a doc line on `ServerNotFound` saying when it is raised.
- `src/server_body.rs`:
  - `client_tx` carries `Result`;
  - each failure branch sends the typed error (R1) before returning `TaskStatus::Failed` (the task status is kept as it is);
  - the exit-before-handshake branch reads `child.wait()`'s status, which it already reports to the observer.
- `src/registry.rs`: `Ok(Ok(Ok(client)))` → client; `Ok(Ok(Err(error)))` → `error`; `Ok(Err(_))` (sender dropped without a word: a body defect) → `ServerExited`, as today.
- `tests/bin/fake_lsp.rs`: `--refuses-initialize`.
- Tests: new `tests/server_start_failure_test.rs`; one assertion changed in `tests/spawn_observer_test.rs`.

#### `tddy-index-daemon`
- `src/status.rs`: one arm, `LspError::ServerNotStarted { .. } => Status::unavailable(failure)`, and one test.

#### `tddy-code-restructuring`
- **New** `src/item_anchor/prefix.rs` (~60 lines): `segments_below` (moved) and `refuse_a_foreign_prefix`. `item_anchor.rs` gains `mod prefix;`.
- `src/backends/rust/item_path.rs`: `segments_below` is deleted and imported from `item_anchor::prefix`.
- `src/runner/entry_points/check_entry_points.rs`: `item_anchor_findings` replaces `unresolvable_without_a_server`; the call at `:237` gains `root` and `?`.
- `src/backends/rust.rs`, `src/registry.rs`: the R5 deletions.
- `src/plan.rs`: the field is removed; unit tests `:1253-1280, 1335, 1368, 1385-1395` are updated (the `rfc3339` tests go with the function).
- `src/plan/codec/file_hint.rs`: `hint_of` → `Ok(FileHint { sha256: hash_file(path)? })`; `rfc3339` and the line-1 `TODO(sharpen)` are removed.
- `src/plan/codec.rs:224-227`: the `TODO(sharpen)` comment and the `#[cfg(test)] pub(crate) use …rfc3339` are removed.
- Tests:
  - new `tests/static_check_item_anchors.rs`, which is not in the e2e filterset;
  - re-pointed: `tests/cancellation_acceptance.rs`, `tests/workspace_root_acceptance.rs`, `tests/spawn_record_acceptance.rs`;
  - `tests/harness/mod.rs:255` drops the `"modified"` key from its fixture header;
  - `tests/snapshot_writes_a_missing_header.rs` gains one test.

#### Dev tooling
- `.agents/skills/code-restructuring/references/plan-schema.md:14,17`: the header example drops `modified`, and the "informational" sentence goes.

## Implementation milestones

- [ ] **M1** Repo-scale measurement (no code). `./dev cargo build -p tddy-tools`, then run `tddy-tools restructure anchors packages/tddy-core/src/workflow/ids.rs --items GoalId` in four ways:
  - (a) cold inside `./dev`, with `TDDY_INDEX_SOCKET` unset;
  - (b) cold outside the dev shell;
  - (c) warm through `eval $(./run-index-daemon | grep '^export ')`;
  - (d) a second warm request.

  Record the command, output, timing and `which rust-analyzer` for each under Validation results. **Stop and ask the developer** if (a), (c) or (d) does not resolve `GoalId`.
- [ ] **M2** `tddy-lsp`: R1; tests 1–6.
- [ ] **M3** `tddy-index-daemon`: status arm; test 7.
- [ ] **M4** Prefix rule move and static check (R2, R3); tests 8–14, with test 21 (existing, live) still green.
- [ ] **M5** R5 deletions and re-pointed tests; tests 18–20 stay green.
- [ ] **M6** R4; tests 15–17; skill reference.
- [ ] **M7** Scoped gates, length gate, docs staged for wrap.

## Testing plan

### Testing Strategy

**Library level, no rust-analyzer**, for every new behaviour:
- `tddy-lsp`'s own `fake_lsp` drives the startup failures, and a non-existent path stands in for a missing binary;
- a static check needs no server by definition (`checking_the_plan(…, deep: false)` passes `client: None`);
- the header is unit-tested.

**The only live-rust-analyzer evidence** is the existing `tests/item_anchor_acceptance.rs:200-216`.
It pins that the deep path's prefix wording is unchanged after the move, and the M1 measurement adds
to it. This node registers **no** new binary in `.config/rust-e2e.filterset`.

#### Option 1 (chosen): fake server and static check
**Locations**:
- `packages/tddy-lsp/tests/server_start_failure_test.rs` (new);
- `packages/tddy-code-restructuring/tests/static_check_item_anchors.rs` (new).

**Trade-off**: neither proves which cause hit `#carve` 5/11; M1 does that for this machine.

#### Option 2 (rejected): a `#[ignore]`d repo-scale test
A cold index of this workspace takes 6–18 minutes and needs the whole tree, so it would never run.
The measurement is recorded instead (PRD F5, approved).

### Coverage Requirements

- [ ] Every R1 row (the "output channel missing" row is not reachable through the public registry; it is covered by code review, not by a test)
- [ ] R2 for both refusals, `Item` and `Items` anchors, and `module_path_of`'s pass-through refusal
- [ ] R3 replacement (one finding per operation) and the reworded finding
- [ ] R4 read compatibility and write shape
- [ ] R5: the three re-pointed tests still assert what they asserted

## Acceptance tests

Names read as behaviour specifications. The "red on `master`" reason is given per test. 5 and 18–21
are green pins.

### `tddy-lsp`, in `packages/tddy-lsp/tests/server_start_failure_test.rs` (new; `fake_lsp`, no rust-analyzer)

1. `a_server_program_that_does_not_exist_is_refused_as_not_found_naming_the_program_and_the_os_error`.
   `LaunchSpec::new("<tempdir>/no-such-rust-analyzer")` → `Err(LspError::ServerNotFound(s))`, where `s`
   starts with that path followed by `": "` and contains `No such file or directory`.
   *Red today:* `ServerExited`.
2. `a_server_that_exits_before_the_handshake_is_refused_as_not_started_with_its_exit_status`.
   `--exit-immediately` → `ServerNotStarted { program: <fake path>, reason: "exited before the initialize handshake completed (exit status: 0)" }`.
   *Red today:* `ServerExited`.
3. `a_server_that_refuses_the_handshake_is_refused_as_not_started_naming_the_servers_error`.
   `--refuses-initialize` → the reason starts with `the initialize handshake failed: ` and contains `refused`.
   *Red today:* the mode does not exist and the error is `ServerExited`.
4. `a_server_that_did_not_start_reads_as_its_program_and_its_reason_in_one_line`.
   The `Display` of test 2's error is exactly `` language server `<fake path>` did not start: exited before the initialize handshake completed (exit status: 0) ``.
   *Red today:* no such variant.
5. `a_request_to_a_server_shut_down_after_it_came_up_is_still_refused_as_exited`.
   `get_or_spawn` succeeds, then `shutdown_all`, then a request through the old client → `ServerExited`.
   **Green pin:** a died-later server keeps today's error.

### `tddy-lsp`, in `packages/tddy-lsp/tests/spawn_observer_test.rs` (changed)

6. `a_server_that_exits_before_the_handshake_is_reported_ended_with_its_exit_code`. The assertion
   `matches!(refusal, Some(LspError::ServerExited))` becomes
   `matches!(refusal, Some(LspError::ServerNotStarted { .. }))`. The observer half
   (`ProcessOutcome::Exited { code: 0 }`) is unchanged.
   *Red today:* the expectation changes.

### `tddy-index-daemon`, in `packages/tddy-index-daemon/src/status.rs` (`mod tests`)

7. `reports_a_server_that_did_not_start_as_unavailable_carrying_why`.
   `ServerNotStarted { program: "rust-analyzer", reason: "exited before the initialize handshake completed (exit status: 1)" }`
   → `Code::Unavailable`, and `message()` equals the error's `Display`.
   *Red today:* no such variant.
   (The existing `reports_a_missing_language_server_as_a_failed_precondition` stays green.)

### `tddy-code-restructuring`, in `packages/tddy-code-restructuring/tests/static_check_item_anchors.rs` (new; static, no server)

Fixture: `a_workspace_holding_files` with package `stacks`, `src/lib.rs` (`pub mod workflow;`) and
`src/workflow.rs` (`pub struct Stack; impl Stack { pub fn new() -> Self { Stack } }`), plus
`tests/notes.rs`. Plans are written with `a_hinted_plan_of` and checked with
`checking_the_plan(…, false)`.

8. `a_static_check_refuses_an_item_anchor_whose_module_is_not_its_files_in_the_words_a_deep_check_uses`.
   An `extract_method` anchored on `stacks::planning::Stack::new` in `src/workflow.rs` gives exactly
   ``["`stacks::planning::Stack::new` is not in src/workflow.rs, which is module `stacks::workflow`"]``.
   This is the deep wording pinned by `item_anchor_acceptance.rs:208-213`, minus `plan is malformed: `.
   *Red today:* the generic finding.
9. `a_static_check_refuses_an_item_anchor_whose_crate_is_not_its_files_package`.
   `queues::workflow::Stack::new` → ``"`queues::workflow::Stack::new` is not in src/workflow.rs, which is module `stacks::workflow`"``.
   *Red today:* the generic finding.
10. `a_static_check_refuses_an_item_path_that_names_the_files_module_rather_than_an_item_in_it`.
    A `rename_symbol` on `stacks::workflow` in `src/workflow.rs` → ``"`stacks::workflow` names the module src/workflow.rs is, not an item in it"``.
    *Red today:* the generic finding.
11. `a_static_check_refuses_an_items_anchor_naming_the_one_name_outside_its_files_module`.
    `items: [stacks::workflow::Stack, stacks::planning::Plan]` → one finding naming `stacks::planning::Plan`.
    *Red today:* the generic finding.
12. `a_static_check_refuses_an_item_anchor_in_a_file_outside_src`.
    An item anchor in `tests/notes.rs` gives `module_path_of`'s refusal verbatim:
    `"tests/notes.rs is not under src, so it is no module of the package `stacks`"`. The `src` path is
    as `module_path_of` prints it; the exact string is fixed when red runs.
    *Red today:* the generic finding.
13. `a_static_check_of_an_item_anchor_with_a_sound_prefix_still_says_only_a_deep_check_can_resolve_it_and_that_its_module_was_verified`.
    Exactly R3(b)'s text for `ExtractMethod` in `src/workflow.rs`.
    *Red today:* the old wording.
14. `an_operation_refused_for_its_prefix_gets_one_finding_not_also_the_deep_check_one`.
    A plan of two operations, one with a bad prefix and one sound, gives exactly two findings at
    operations 0 and 1: the refusal, then R3(b).
    *Red today:* two generic findings.

### `tddy-code-restructuring`: the plan header

15. In `packages/tddy-code-restructuring/src/plan.rs` (`mod tests`):
    `a_v2_header_that_still_carries_modified_parses_and_keeps_only_the_hash`. This replaces
    `a_v2_header_carries_its_file_hints`. It parses the same line, and
    `serde_json::to_value(&plan.files["src/lib.rs"])` equals `json!({"sha256": "sha256:ab"})`.
    *Red today:* `modified` is kept.
16. In `packages/tddy-code-restructuring/tests/snapshot_writes_a_missing_header.rs`:
    `a_written_header_names_each_file_by_its_hash_alone`. After `runner::snapshot` of a headerless
    plan, `files["src/lib.rs"]` equals `json!({"sha256": <digest>})`.
    *Red today:* `modified` is written.
17. Same file: `a_plan_whose_header_carries_modified_is_checked_and_its_rewrite_drops_it`. A v2 plan
    whose header carries `modified` passes `check` with no findings about the header, and
    `runner::snapshot` writes it back without the key.
    *Red today:* the key is written back.

### `tddy-code-restructuring`: re-pointed and existing pins (green throughout)

18. `packages/tddy-code-restructuring/tests/spawn_record_acceptance.rs`:
    `a_language_server_the_backend_starts_itself_is_recorded_with_the_names_of_its_pinned_environment`.
    The trigger becomes `ItemResolver::resolve_item(&mut backend, ORIGIN_LIB, &ItemPath::parse("<crate>::level"))`,
    with the answer ignored as today. The assertions are unchanged.
19. `packages/tddy-code-restructuring/tests/cancellation_acceptance.rs`: `an_operation_that_waits_for_the_index`
    calls `backend.resolve(&an_extraction_of_the_function_body(), &workspace)`, which the same file
    already uses at `:228`, so it waits in `ensure_indexed` on `--cold-hovers`. The three tests at
    `:90, :111, :139` keep their assertions. The return type becomes `Result<Resolution, RestructureError>`;
    the assertions read only the error.
20. `packages/tddy-code-restructuring/tests/workspace_root_acceptance.rs`: `a_cancelled_wait` calls
    `backend.resolve(&<an extract_method over src/lib.rs's body>, &workspace)`. The two progress tests
    at `:193, :212` keep their assertions.
21. `packages/tddy-code-restructuring/tests/item_anchor_acceptance.rs:200`
    (`a_module_prefix_that_does_not_match_the_file_is_refused`, live, unchanged). It proves the deep
    wording survives the move of `segments_below`.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (stack brief, 2026-10-09): "All 12 wave-1 PRDs approved. Every node's own
F-decisions: take the agent's recommendation". "Consents: node 12 edits `tddy-lsp` and
`tddy-index-daemon`, and deletes the ~145 dead lines (`anchor_for`, `places_of`, `module_outline`, …)
moving 3 tests to `resolve_item`".

**TAKEN (PRD review):**
- **F1 — edit `tddy-lsp` and `tddy-index-daemon`.** Yes. The startup failure is decided in `tddy-lsp`.
- **F2 — delete the dead `anchor_for` path.** Yes (R5).
- **F3 — a static check whose prefixes are all sound still reports a finding.** Yes. The plan never
  passes a static check green (R3(b)).
- **F4 — remove `modified`, rather than give it a reader.** Remove. The stored value is the snapshot
  time, which no drift report needs.
- **F5 — repo-scale proof.** A manual measurement recorded in M1, not an automated test.

**OPEN (each with a recommendation):**
- **F6 — what the two waiting tests are re-pointed to. DECIDED (developer, 2026-10-09): (a).** The consent says "moving 3 tests to
  `resolve_item`". **`resolve_item` does not wait on the index.** It reads `documentSymbol` through
  `settled_outline`, and `fake_lsp` answers that at once with a non-empty outline
  (`fake_lsp.rs:621-628`). The cancellation and progress tests would then pass without ever waiting:
  green and pinning nothing.
  - **(a) Recommended:** `spawn_record_acceptance` moves to `resolve_item` (it only needs the server
    started), and `cancellation_acceptance` and `workspace_root_acceptance` move to
    `LanguageBackend::resolve` of an `extract_method`. That path waits in `ensure_indexed` on hover,
    exactly as `anchor_opening` did, and the same file already uses it at `:228`.
  - **(b)** Add a `fake_lsp` mode that answers an empty outline and never sends `quiescent`, so that
    `resolve_item` waits. This is more fake surface, it tests `settled_outline` instead of
    `ensure_indexed`, and it collides with `apply-robust`'s `fake_lsp` mode.
- **F7 — a missing binary: reuse `ServerNotFound` or fold it into `ServerNotStarted`. DECIDED (developer, 2026-10-09): (a); a missing binary answers `failed_precondition`.**
  - **(a) Recommended:** reuse it. The variant exists, the daemon already maps it to
    `failed_precondition` with a test, and "not installed" is a host condition, as that mapping's
    comment says.
  - **(b)** One variant for every startup failure. Simpler, but it leaves `ServerNotFound` dead.

  Under (a), a daemon whose rust-analyzer is missing answers `failed_precondition` instead of today's
  `unavailable`. That is a status-code change for a client of the daemon. `tddy-tools`' client reads
  only the message (`index_client.rs`), and `grep -rn Unavailable packages/tddy-tools/src` finds no branch on that code.
- **F8 — how the typed error reaches the registry.**
  - **(a) Recommended:** change `client_tx` to carry `Result`. It is typed and has no race.
  - **(b)** The registry reads the task's terminal `TaskStatus::Failed { message }` through
    `status_watch`. That leaves only a string, so `ServerNotFound` cannot be told apart, and it waits on
    a status the task registry sets after the body returns.

Decisions taken by this plan:
- the prefix rule lives in a child module, to keep `item_anchor.rs` ≤ 500 production lines;
- `check_plan`'s length does not change;
- no launch or toolchain behaviour changes.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

(empty; M1's measurement goes here first, then `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`)

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-anchors-outline-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run tests scoped to the touched packages (`./test -p tddy-code-restructuring -p tddy-lsp -p tddy-index-daemon`) — verify 100% pass; CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> --all-targets -- -D warnings` for each touched package, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-09-reshape-anchors-outline-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
