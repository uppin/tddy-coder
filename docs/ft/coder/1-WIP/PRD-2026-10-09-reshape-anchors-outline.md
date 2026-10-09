# `restructure anchors` says why it cannot start, and static `check` verifies an item anchor's module - PRD

**Date**: 2026-10-09
**PRD Type**: Bug fix / Enhancement
**Stack**: `#reshape` 12/19 (`feature/reshape/anchors-outline`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md):
  - `## Item anchors`: the refusals a static check can now give.
  - `## Plan format`: the v2 header loses `modified`.
  - `## LSP integration` / `### Refusal classes`: a server that never started is named.
  - `## Known limitations`: the static-check limitation narrows.

No other feature document changes. This change adds no new operation, plan-line field or flag. The
`anchors` command line and its output on success stay exactly as they are.

## Summary

This change fixes three things that cost a plan author time:

- **A cold run that cannot start rust-analyzer says why.** Today `restructure anchors` (and any other
  command that needs the server) prints `rust-analyzer LSP: lsp server exited`, whatever went wrong. It
  will name the program and the reason instead: the program is not on `PATH`, it exited before the
  handshake, or the handshake failed.
- **The empty-outline record is closed on a measurement.** The warm path is measured, and the cold path
  re-measured, at repo scale.
- **A static `check` (no `--deep`) verifies what it can about an item anchor without a server.** It
  refuses an anchor whose crate or module does not match its file, in the same words `check --deep`
  and `apply` use. An anchor whose prefix is sound still gets the finding that only a deep check can
  verify the item itself.
- **The plan header loses its dead field.** The v2 header stops writing `modified`, which no reader
  ever consulted. Plans that still carry the field keep parsing.
- **The superseded anchor path is deleted.** `LanguageBackend::anchor_for` and the code only it reached
  (`places_of`, `module_outline`, …) are deleted: about 145 production lines, about 125 of them in
  `backends/rust.rs`. The developer consented to this on 2026-10-09.

## Background

The code-restructuring skill makes `restructure anchors` mandatory for writing `extract_module` and
`move_item` plans: "**Do not hand-write line numbers.**" On `#carve` 5/11 the command refused every item
on the warm daemon, and on the cold path it died with `lsp server exited`. The four-way split of
`changeset.rs` was done by hand. The standing record,
[broken-restructure-anchors-empty-outline](../../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md),
was partly fixed by #537 and has been unowned since.

The re-examination for this node found three things:

1. **The warm-path diagnosis is stale.** The refusal text the record quotes comes from `places_of`.
   Since #537 that code is reached only through `LanguageBackend::anchor_for`, which no production
   path calls. `anchors` resolves items through the item-anchor resolver instead. The index daemons'
   logs from 2026-10-07/08 show **103 `anchors` requests answered at repo scale, and 3 refused**. All
   three refusals were mistakes in the request, not empty outlines.
2. **`lsp server exited` hides the cause.** `tddy-lsp` turns every failure to bring a server up into
   one bare `ServerExited`, and drops the message its server task produced. Those failures are:
   - a missing binary;
   - a server that exits at once;
   - a failed `initialize`.

   Outside the nix dev shell `rust-analyzer` is not on `PATH`. That is enough to produce exactly the
   record's output. Which cause hit `#carve` 5/11 is not known.
3. **Static `check` is half-way to what its todo asks.** The range-shape checks the todo proposes
   already run at plan parse (`Anchor::validate`). What is left, and needs no server, is the
   crate/module prefix. Today that is checked only by the Rust resolver, after a server has answered.

The dead `FileHint.modified` field
([dead-code-plan-filehint-modified](../../../../packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md))
has been re-measured seven times since 2026-10-02 with zero readers. Its writer costs one `fstat` per
file named in the plan.

## Proposed Changes

### What's Changing

- **A server that never started is named.** When the language server cannot be brought up, the
  refusal says which program was launched and why it did not come up:
  - the spawn error, such as `No such file or directory`;
  - an exit before the handshake, with its exit status;
  - the `initialize` error.

  A program that cannot be found is the existing `LspError::ServerNotFound`. Nothing constructs that
  variant today, and the daemon already maps it to `failed_precondition`. Every other failure to start
  is a new `LspError::ServerNotStarted { program, reason }`, which the daemon maps to `unavailable`.
  This applies to the cold command line and to the index daemon alike, and the gRPC status carries
  the same words. A server that exits **after** it came up keeps today's `lsp server exited`.
- **Static `check` refuses an item anchor whose prefix is wrong.** For every `item` and `items` anchor,
  a static check reads the file's module path from the package layout, with no server. It reports a
  finding for each case below, worded exactly as the deep resolver words it:
  - the crate name does not match the file's package;
  - the module segments do not match the file's path;
  - the path names the file's module rather than an item in it;
  - the file is not under its package's `src/`, or is in no package.
- **Static `check` keeps the deep-check finding.** An operation whose anchors all pass the prefix
  check still gets one finding saying the remaining checks need `check --deep`: the item is present,
  is unambiguous, and has an unchanged fingerprint. A plan of item anchors therefore still never passes
  a static check green, as today. Only the finding's wording changes. It still contains "anchors by
  item, which only a deep check can resolve", and adds that the module prefix was verified.
- **One rule for the prefix.** The prefix rule moves from the Rust backend into the
  language-independent item-anchor module, so the static and deep paths cannot drift apart.
- **The v2 header writes `{"sha256": …}` per file and no `modified`.** Reading a header that carries
  `modified` still works: the key is ignored, and the next rewrite drops it.
- **The dead `anchor_for` path is deleted.** This covers the trait method and its default
  (`registry.rs`), and in `backends/rust.rs`: `anchor_for`, `anchor_opening`, `module_outline`,
  `OutlineItem`, `places_of` and `refuse_non_adjacent`. Three tests used it only as "an operation that
  starts or waits on the server", and they move as follows:
  - the spawn-record test moves to `resolve_item`;
  - the two cancellation/progress tests move to `LanguageBackend::resolve` of an `extract_method`,
    because their fake server answers the outline at once and only that path waits on the index (see
    the changeset, F6 — decided by the developer, 2026-10-09).
- **The skill's plan-schema reference stops describing `modified`.** The package and feature docs
  follow at wrap.
- **The empty-outline record is closed by measurement.** It is deleted at wrap, with the measurements
  (warm and cold, inside and outside the dev shell) recorded in the change history.

### What's Staying the Same

- `anchors --items` and `anchors --at`: their arguments, their JSON output and every refusal they give
  today for a request mistake.
- The outline wait (`settled_outline`). An empty outline is believed only once the server is seen
  quiescent. A server that never sends `experimental/serverStatus` still waits until the caller
  cancels. This stays a documented limitation.
- `check --deep` and `apply`: they resolve item anchors as before, and their refusal text for a
  prefix mismatch is unchanged. The static check adopts those words.
- How the cold command line launches rust-analyzer. It is still the `rust-analyzer` found on `PATH`.
  This change does not add a lookup or a fallback. A missing binary is refused by name, never worked
  around.
- v1 `range` plans and `snapshot` (the `--rebase` proposal stays deferred).
- The daemon's anchors handler still resolves an item twice (a separate, deferred todo).

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`:
  - the prefix rule moves from `backends/rust/item_path.rs` to `item_anchor.rs`;
  - the static check uses it (`runner/entry_points/check_entry_points.rs`);
  - `FileHint.modified`, the mtime read in `hint_of` and `rfc3339` are removed, along with the two
    `TODO(sharpen)` markers that existed only because of `rfc3339`;
  - the production-dead `anchor_for` path is deleted, about 145 lines.
- `tddy-lsp`: the registry reports why a server it spawned never came up, instead of a bare
  `ServerExited`. **This is a package outside `tddy-code-restructuring`.** The brief scopes this node
  to the restructuring crate, but the cold-path message is decided in `tddy-lsp`.
- `tddy-index-daemon`: `status_of_lsp` maps `ServerNotStarted` to `unavailable`, carrying the
  message. A missing binary arrives as `ServerNotFound`, which is already `failed_precondition` (F7 — decided by the developer, 2026-10-09).
- **Breaking for Rust callers:** `FileHint` is public (`lib.rs:37`) and loses a field. No crate in
  this workspace reads it. The plan file format stays backward compatible for reading.
- New tests run without rust-analyzer: `tddy-lsp` integration tests over the existing fake server,
  and a static-check binary. Nothing joins `.config/rust-e2e.filterset`.

### User Impact

- An author whose cold run fails is told the actual cause (most likely "rust-analyzer is not on
  PATH"), not a message that reads like a crash.
- A plan with a mistyped crate or module in an item anchor is caught by the cheap static `check`,
  instead of after minutes of indexing in `check --deep` or `apply`.
- Plan headers get one field shorter. Nobody reads the old field, so no workflow changes.

## Implementation Plan

1. **Repo-scale measurement first.** Build `tddy-tools` and run
   `restructure anchors packages/tddy-core/src/workflow/ids.rs --items GoalId` four times: cold inside
   `./dev`, cold outside it, warm through `./run-index-daemon`, and warm on a second request. Record
   the output and timings in the changeset. If the warm or in-shell cold leg fails, stop and re-plan
   with the developer: the record would then hold a defect this PRD does not yet describe.
2. **`tddy-lsp`: a server that never came up is named.** The registry reads the spawned task's
   failure, and the error type gains a variant that carries the program and the reason. The existing
   exit-before-handshake test changes its expectation.
3. **`tddy-index-daemon`: status mapping** for the new variant.
4. **The prefix rule moves into `item_anchor.rs`.** The Rust resolver calls it. The deep-path wording
   is unchanged, which the existing live test pins.
5. **The static check verifies prefixes,** and the deep-check finding is re-worded within its pinned
   phrases.
6. **The dead `anchor_for` path is deleted** and its three test callers are moved.
7. **`FileHint.modified` is removed,** together with the mtime read, `rfc3339` and its re-export, the
   unit tests and the harness fixture key. The skill's `references/plan-schema.md` is updated.
8. **Docs at wrap:**
   - `docs/item-anchors.md`, `docs/plan-store.md` and the feature doc, through the changeset;
   - delete both code-issue records with their final measurement;
   - delete the static-check todo.

## Acceptance Criteria

- [ ] A repo-scale cold run and a warm run of `anchors … --items GoalId` resolve `GoalId`, and the measurements are recorded in the changeset ([Rust code restructuring](../rust-code-restructuring.md#item-anchors)).
- [ ] When the configured language-server program does not exist, getting a server is refused with an error that names the program and the spawn error. It is not `lsp server exited`.
- [ ] A server that exits before the handshake is refused naming that it exited before initializing, with its exit status. The process end is still reported to the spawn observer.
- [ ] The index daemon maps a server that did not start to `unavailable` and a missing one to `failed_precondition`, each carrying the message.
- [ ] `LanguageBackend::anchor_for`, `places_of`, `module_outline`, `OutlineItem` and `refuse_non_adjacent` no longer exist, and the three tests that called `anchor_for` still pin what they pinned.
- [ ] Static `check` reports a finding for an `item` anchor whose crate, or whose module, does not match its file, worded exactly as `check --deep` words the same refusal.
- [ ] Static `check` reports a finding for an `items` anchor with one mismatched name, and for an anchor in a file outside `src/`.
- [ ] Static `check` of a plan whose item anchors have sound prefixes still reports one finding per operation. The finding contains "anchors by item, which only a deep check can resolve" and says the module prefix was verified.
- [ ] A v2 header written by `snapshot` or by a plan refresh carries `sha256` and no `modified`.
- [ ] A v2 plan whose header still carries `modified` parses and runs.
- [ ] No `modified`/`rfc3339` code is left in `packages/tddy-code-restructuring/src` (the record's own `grep` finds only unrelated hits).
- [ ] Tests pass for `tddy-code-restructuring`, `tddy-lsp` and `tddy-index-daemon` (scoped; CI for the rest).

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-anchors-outline.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-anchors-outline-initial-discovery.md`
- Code issue this closes: [broken-restructure-anchors-empty-outline](../../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md)
- Code issue this closes: [dead-code-plan-filehint-modified](../../../../packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md)
- Todo this closes: [2026-10-02-static-check-cannot-verify-item-anchors](../../../dev/todo/2026-10-02-static-check-cannot-verify-item-anchors.md)
- Reference, not changed: [2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan](../../../dev/todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md), [2026-10-02-the-daemon-anchors-path-resolves-an-item-twice](../../../dev/todo/2026-10-02-the-daemon-anchors-path-resolves-an-item-twice.md)
- Package design: [item-anchors.md](../../../../packages/tddy-code-restructuring/docs/item-anchors.md)
