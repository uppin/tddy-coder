# Changeset: `restructure snapshot` writes the header a plan is missing

**Date**: 2026-10-05
**Status**: 🚧 In Progress
**Type**: Feature (engine capability; no new operation, no new subcommand)
**Draft PR**: https://github.com/uppin/tddy-coder/pull/592
**Stack**: `#sharpen` 5/8, branch `feature/sharpen/plan-header`, wave 2. PR title:
`feat(code-restructuring): restructure snapshot writes the header a plan is missing (#sharpen 5/8)`.
Base in the linear stack: `feature/sharpen/apply-heartbeat` (K=4); the only **real** edge is to
`feature/sharpen/tidy-engine-files` (K=1), by file overlap.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-05-sharpen-plan-header-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

The scan followed `deferred-work/references/planning-cross-check.md`. `grep -rl 'Claimed by:'` over
`tddy-code-restructuring`, `tddy-tools` and `tddy-index-daemon` finds one file
(`broken-restructure-anchors-empty-outline.md`) whose value is `none` (#537 merged 2026-10-02), so **no 🚧 claimed
issue is in this change's path and there is no wait-or-proceed fork.** Packages edited here with no
`docs/code-issues/` directory: none (`tddy-code-restructuring`, `tddy-tools` and `tddy-index-daemon` all have one;
`tddy-lsp` is not touched).

| Item | Verdict | What this change does about it |
|---|---|---|
| `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md` — **this entry exists only on `feature/carve/lifecycle-ports-agents` (PR #532)**; whichever of that PR and this node lands second deletes it at wrap | ✅ **RESOLVED HERE** (planned) | `snapshot` of a plan whose first line is an operation writes the header from the operations' anchored files. Closed when the acceptance tests below pass. Reference by branch and file, not by relative link: it is not on `master` |
| `packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md` | ⚠ **During** | The header is written through the existing `hint_of` (`plan/codec.rs:213` on `master`, `plan/codec/file_hint.rs` after `tidy-engine-files`); this change adds **no second writer of `modified`**. It does not close the issue (the field stays written and unread) |
| [2026-10-05-restructure-engine-files-past-the-500-line-budget.md](../todo/2026-10-05-restructure-engine-files-past-the-500-line-budget.md) | ⚠ **During** (resolved by `tidy-engine-files`, not here) | `plan/codec.rs` is the file this node edits. New code goes in a new child module `plan/codec/headerless.rs`; `codec.rs` gains one `mod` line and one call, and must still be <= 500 production lines afterwards. `plan.rs` and `item_anchor.rs` are not touched |
| `packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md` | — (no live claim) | Not routed through `anchors`; see Boundaries |
| [2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing.md](../todo/2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing.md) | — Unrelated | The live-plan `Snapshot` RPC is a different concern; this change only makes the RPC's `snapshot_resolving` call accept a headerless plan |
| [2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md) | — Unrelated | A range-plan rebase; no overlap |
| `docs/dev/1-WIP/2026-09-17-restructure-refusal-truth-and-authoring-gates.md` | ℹ not this stack's to wrap | All milestones `[x]`; it introduced `snapshot`. Looks unwrapped rather than active; ask whether it is stale before wrap |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) (the `snapshot` line),
  new child module `src/plan/codec/headerless.rs`, `src/plan/codec.rs` (one `mod` line, the refusal text),
  `src/runner/entry_points/check_entry_points.rs` (`snapshot`, `snapshot_resolving`).
  Docs at wrap: the plan-format description in [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)
  and [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (a headerless plan is accepted by `snapshot`).
- **`tddy-tools`**: [README](../../../packages/tddy-tools/README.md) is not edited; **one test file only**,
  `tests/restructure_cli_acceptance.rs`. No source edit: routing is already correct and is pinned by the tests.
- **`tddy-index-daemon`**: **one test file only**, `tests/code_index_service_acceptance.rs`. No source edit.
- **`.agents/skills/code-restructuring/SKILL.md`** step 6 (Snapshot): one sentence (dev-only, not distributed).

## Related Feature Documentation

- [PRD-2026-10-05-sharpen-plan-header.md](../../ft/coder/1-WIP/PRD-2026-10-05-sharpen-plan-header.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Plan format`, the `snapshot` row of the CLI table

## Summary

`tddy-tools restructure snapshot <plan>` writes line 1 of a plan whose first line is an operation: a schema header
computed from the files the operations' anchors name. A plan author no longer hand-computes a `sha256` to get a plan
past `check --deep`. `check`, `apply`, `status` and `load` still refuse a headerless plan, and the refusal now names
`restructure snapshot` as the remedy.

## Background

`restructure anchors <file> --items …` emits an anchor; wrapping it in a one-line plan is the obvious next step, and
`check --deep` refuses it with `plan is malformed: first line must be a snapshot header`. The command the skill names
for "rewrite the header", `snapshot`, refuses the same file with the same text, because it only rewrites a header that
is already there (todo above). The author wrote the v2 header by hand with `shasum -a 256`. A v2 header is only hints,
so the engine can compute it from the plan's own anchors.

## Responsibility

- `snapshot` (library `runner::snapshot`, hence the CLI `restructure snapshot` and the daemon's `Snapshot` RPC) accepts a
  plan whose first non-blank line is an operation and **inserts** a header above it, leaving every operation byte for byte
  as it arrived.
- `snapshot_resolving` (the router the CLI's cold path and the daemon call) reaches `snapshot` for such a plan without
  asking for, or waiting on, a language server.
- The header is computed from the anchors' `file` fields (primary and `also`), through the existing `hint_of`.
- A headerless plan given to anything else is refused with a message that names the remedy.
- Pin, by test, the three routing facts the design rests on (CLI does not start a server; CLI does not dial a named
  daemon; the daemon does not acquire a server).

## Boundaries

- **No new subcommand, no `plan new` skeleton, no change to `restructure anchors`.** `anchors` takes a file and reads no
  plan; the code issue `broken-restructure-anchors-empty-outline.md` is the reason not to route this through it.
- **`Plan::parse` stays strict.** Routing (`plan_file_has_item_anchors`) reads a headerless plan as "no item anchors" through
  the parse error. Making `parse` tolerant would send the first `snapshot` of a headerless item-anchored plan to a cold
  rust-analyzer. The tolerant read is a separate function, called only by `snapshot`.
- **No change to `check`, `apply`, `status`, `load`, `verify`, `anchors`, the plan store or the daemon's plan reader**, other than
  the text of one refusal.
- **No proto change, no `tddy-tools` or `tddy-index-daemon` source edit.**
- **No second writer of `modified`**; `hint_of` is the writer.
- **No re-resolution of anchors on the first snapshot.** A headerless plan has no header to have drifted from; its anchors resolve at
  run open as always.
- **`plan.rs` and `item_anchor.rs` are not edited.** `plan/codec.rs` only by one `mod` line and the refusal call.
- **Not re-done here** (see Dependencies): the length split of those three files.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| **`tidy-engine-files`** (K=1, `feature/sharpen/tidy-engine-files`) | `plan.rs`, `plan/codec.rs` and `item_anchor.rs` each <= 500 production lines, by child modules, made with the engine's own `move_item`/`extract_module`; no behaviour change. For this node that means: `hint_of` and `rfc3339` live in `plan/codec/file_hint.rs` and `refuse_split_groups` in `plan/codec/groups.rs`, with `hint_of` still reachable as `plan::hint_of` through the facades; `Plan::parse` and `parse_ops` stay in `plan/codec.rs` (about 452 lines). `RefactorKind`'s new home (`plan/refactor_kind.rs`, its decision D1) is **not** used here: this node adds no variant and no field. **File overlap and layout only** | `plan/codec.rs` is smaller and already split, so the new child module and the two-line edit sit on the post-split shape; the header is still written through `hint_of`, now in `file_hint.rs`; this PR rebases onto it before its own first commit | re-split `plan.rs`, `plan/codec.rs` or `item_anchor.rs`; move, rename or re-order `parse_op` rules or any existing test; re-measure the budget or claim `2026-10-05-restructure-engine-files-past-the-500-line-budget.md` as its own; add logic to `codec.rs` beyond one `mod` line and one message call |

No other node is a dependency. `move-fidelity`, `spawn-record` and `apply-heartbeat` sit below this node in the linear
order only because `gh stack` needs one line.

## Draft PR contract

Published with the wave-2 contract commit. The **owned surface is behaviour**, reached through public entry points;
the private helper names below are a proposal that green may reshape, and no test names them.

- **Public, unchanged signatures, new behaviour**: `tddy_code_restructuring::runner::snapshot(root: &Path, options: Options) -> Result<SnapshotRewrite>`
  and `runner::snapshot_resolving(root, options, client: Option<Arc<LspClient>>, cancel) -> Result<SnapshotRewrite>` accept a
  headerless plan; `SnapshotRewrite { plan, paths, rewritten, stale }` keeps its shape (`paths` = files named by the written header).
- **Proposed crate-private** (in `plan/codec/headerless.rs`): `Plan::starts_with_an_operation(jsonl: &str) -> bool`,
  `Plan::parse_headerless(jsonl: &str) -> Result<Plan>` (every non-blank line is an operation; `version` 2, empty `files`),
  `Plan::header_for_anchored_files(&self, root: &Path) -> Result<String>`.
- **Refusal text** (class unchanged, `RestructureError::MalformedPlan`): `first line must be a snapshot header; this plan's first
  line is an operation, so run `restructure snapshot <plan>` to write one`.
- **The failing tests that specify it** (all red on `master` today, for the reason stated) are the thirteen in
  "Acceptance tests". They are written against the post-`tidy-engine-files` tree.

Nothing in this surface is consumed by another node.

**Published with the contract commit** (as built on this branch, before `tidy-engine-files` lands): `plan/codec/headerless.rs` with
`Plan::starts_with_an_operation` and `Plan::parse_headerless` implemented, `Plan::anchored_files` (added: the sorted, de-duplicated
file set; `header_for_anchored_files` will use it), and `Plan::header_for_anchored_files` **refusing** with
`TODO(plan-header): implement`. `snapshot` routes a headerless plan to a private `insert_a_header` splice (leading blanks kept,
header inserted above the first operation) and `snapshot_resolving` uses `plan_file_has_item_anchors`. Wiring only: a headerless
`snapshot` still refuses. The refusal text for the other readers (test 8, 12) is left to green (M3) so those tests fail for the
missing behaviour. `codec.rs` is 517 production-ish lines on this branch (`tidy-engine-files` has not landed here); only one `mod` line was added.

## Green wave

**Wave:** 2 of 2.
**Greenable independently:** yes, once `feature/sharpen/tidy-engine-files` is on its base (file overlap only; its tests do not depend on any other node's behaviour).
**Concurrent with:** `feature/sharpen/retarget-impl`, `feature/sharpen/repoint-call`, `feature/sharpen/repoint-facade` (same wave, no edge between them; the line serialises them because `retarget-impl` and `repoint-call` also add a `mod` line and a call to `plan/codec.rs`).
**Blocks:** none.
Real dependency edges (whole stack): `tidy-engine-files -> plan-header, retarget-impl, repoint-call, repoint-facade`; `move-fidelity -> repoint-facade`; `retarget-impl -> repoint-call`. Nothing else is an edge: `spawn-record` and `apply-heartbeat` consume nothing and nothing consumes them (`spawn-record` lands after open draft PR #586, a merge-order fact, not a stack edge).

## Successor PRs

None. No later node consumes this surface.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **Header from anchors**: `plan/codec/headerless.rs` with the tolerant read and `header_for_anchored_files`
- [x] **`snapshot` inserts the header** above an operation first line; operation bytes untouched; idempotent
- [x] **`snapshot_resolving`** reaches `snapshot` for a headerless plan (one-line change)
- [x] **Refusal text** of every other reader names `restructure snapshot`
- [x] **Routing pinned**: CLI starts no server and dials no daemon; the daemon holds no root for it
- [x] **Refusals**: anchored file missing, anchored file outside the workspace, first line neither header nor operation
- [ ] **Package documentation**: plan format (`rust-code-restructuring.md`, `plan-schema.md`, SKILL.md step 6, README `snapshot` line) at wrap
- [x] **Testing**: the thirteen acceptance tests pass; `./test -p tddy-code-restructuring -p tddy-tools -p tddy-index-daemon`, scoped
- [x] **Code quality**: `cargo clippy -p tddy-code-restructuring -p tddy-tools -p tddy-index-daemon --all-targets -- -D warnings`, `cargo fmt`, `plan/codec.rs` <= 500 production lines
- [ ] **Stack bookkeeping**: the whole-work todo for this entry removed at wrap by whichever of #532 and this PR lands second

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

On `a77bca29`:

- `Plan::parse` (`plan/codec.rs:55-66`): the first non-blank line is a v2 header (`header_version == Some(2)`) or must deserialise as
  `SnapshotHeader`; anything else, including an operation, is `plan is malformed: first line must be a snapshot header`.
- `snapshot` (`check_entry_points.rs:71`) parses with `Plan::parse`, asks `plan.rehashed_header(root)` (which re-hashes **only the paths the
  header already names**) and **replaces** the first non-blank line, keeping leading blank lines and every later line as bytes. It writes
  nothing when the result equals the file.
- `snapshot_resolving` (`:109`) calls `read_plan(&path)?` (= `Plan::parse`) to decide whether the plan has item anchors; a headerless plan fails there.
- Routing for a plan that does not parse: `plan_file_has_item_anchors` is `false` (the parse error is swallowed), so the CLI
  (`restructure_cli.rs:279`), the dial decision (`tddy-tools/src/index_client.rs:57-61`) and the daemon (`queries.rs:150`) all take the in-process path.
- `check`, `apply`, `status`, `load` and `tddy-daemon-rpc/.../code_navigation/plan.rs:51` all read through `Plan::parse`.
- v2 header = `{"v":2,"files":{"<path>":{"sha256":"sha256:…","modified":"<RFC 3339>"}}}`, hints only, never refuses a run (`plan-schema.md:10-21`).
  v1 header = `{"v":1,"snapshot":{"<path>":"sha256:…"}}`, refuses on drift.

### State B (Target)

| Path | Headerless plan today | After this node |
|---|---|---|
| `restructure snapshot <plan>` (CLI; with or without `TDDY_INDEX_SOCKET`) | `plan is malformed: first line must be a snapshot header` | writes the header, in process; never starts a language server, never dials a named daemon |
| `runner::snapshot` | same | writes the header |
| `runner::snapshot_resolving` (daemon `Snapshot` RPC; CLI cold path) | same, raised by `read_plan` | routes to `snapshot`; no server |
| `restructure check`, `check --deep`, `apply`, `status`, `load`, the daemon's plan reader | same text | still refused (`MalformedPlan`); the text now ends `…run \`restructure snapshot <plan>\` to write one` |
| `restructure anchors`, `verify`, `warm` | not plan readers / unchanged | unchanged |

#### How the header is computed

1. **Detect.** The first non-blank line is a JSON object with an `op` key and none of `v`, `snapshot`, `files`. (`starts_with_an_operation`.)
   Any other first line that is not a header keeps the existing refusal text, including the added remedy only when the line **is** an operation.
2. **Read every line as an operation** with the existing `parse_ops`, so every operation refusal (`CodeTextInPlan`, an unknown field, a split group,
   a code-bearing field) surfaces from `snapshot` exactly as it would from `check`.
3. **Files** = the set of `anchor.file()` over `op.anchors()` (the primary anchor and every `also` anchor), de-duplicated, in the `BTreeMap`'s
   sorted order.
4. **Validate each file** before anything is written: relative (not absolute), no `..` component, and `root.join(file)` is a regular file.
5. **Version** per Decision O1: **v2** when every anchor of every operation is `item` or `items`; **v1** when any anchor is a `range` or `symbol`
   (recommended; the brief's text is "v2 form" and is the alternative).
6. **Write** the header with the serialisation `rehashed_header` already uses (`HintedHeader{v:2, files}` through `hint_of`, or `SnapshotHeader{v:1, snapshot}`
   through `crate::apply::hash_file`), so the line is byte-identical to what a second `snapshot` of the headed plan produces.
7. **Splice**: insert `header + "\n"` immediately before the first non-blank line. Leading blank lines, every operation line and the file's
   trailing newline state are carried as the bytes they arrived as. `SnapshotRewrite { paths = files.len(), rewritten: true, stale: [] }`.

#### Idempotence

Running it twice: the second run reads a **headed** plan, so it takes the existing path (`rehashed_header` over the paths line 1 names), produces
the same line, and reports `rewritten: false` and an untouched file (and so an unchanged mtime). If a file the header names changed between the runs,
the second run updates its hint, as `snapshot` always did. A file an operation anchors that line 1 does **not** name is not added by the second run
(existing contract: "inventing a claim the author never made"); the first run is the only time the engine authors the set.

*Consequence worth stating:* the plan now holds item anchors, so a second `snapshot` of it takes the item-anchor route (a warm daemon's `Snapshot`
RPC, or a cold rust-analyzer without one). That is the existing behaviour for every headed item-anchored plan, not a new cost.

#### Refusals (class `plan is malformed:` unless stated)

| Condition | Message (stem) | Written? |
|---|---|---|
| file empty / only blank lines | `plan is empty` (existing) | no |
| first line is neither a header nor an operation | `first line must be a snapshot header` (existing text, no remedy) | no |
| first line is an operation | not a refusal for `snapshot`; for every other reader: `first line must be a snapshot header; this plan's first line is an operation, so run \`restructure snapshot <plan>\` to write one` | no |
| an operation line does not parse | that operation's own refusal (`CodeTextInPlan`, unknown field, split group, …) | no |
| **ops name no file** (the only way a headerless plan has none: a line-1 operation whose every anchor `file` is the empty string) | `an anchor names no file: the header lists the files a plan was written against` | no |
| an anchored file is not there / not a regular file | `<path> could not be read: …` (the existing `hint_of` text) or `<path> is not a file` | no |
| an anchored file is absolute or contains `..` | `` `<file>` is outside the workspace: an anchor's `file` is relative to the workspace root `` | no |
| the plan file cannot be read | `Io` (existing), unchanged | no |

All refusals leave the plan file byte-identical. Every refusal is raised **before** the write, so there is no partial header.

### Delta (What's Changing)

#### `tddy-code-restructuring`

- **New**: `src/plan/codec/headerless.rs` (~60 production lines): `starts_with_an_operation`, `parse_headerless`,
  `header_for_anchored_files`, the file validation, the refusal-with-remedy helper.
- **`src/plan/codec.rs`**: `mod headerless;` and the `Plan::parse` refusal at `:65` calls the helper (2 lines changed, 1 added).
- **`src/runner/entry_points/check_entry_points.rs`**: `snapshot` chooses between the replace-splice (headed) and an insert-splice
  (headerless), ~15 lines; `snapshot_resolving` replaces `!has_item_anchors(&read_plan(&path)?)` with `!plan_file_has_item_anchors(&path)` (1 line;
  `plan_file_has_item_anchors` is already `pub` in `item_anchor.rs`).
- **No** change to `plan.rs`, `item_anchor.rs`, `rust.rs`, `plan_store.rs`, `restructure_cli.rs`.

#### `tddy-tools`, `tddy-index-daemon`

- Tests only. No source change.

## Implementation milestones

- [x] **M1** Library: `Plan::starts_with_an_operation`, `parse_headerless`, `header_for_anchored_files`; `snapshot` inserts; tests 1-6 and 9 pass (written first, failing)
- [x] **M2** `snapshot_resolving` reaches `snapshot` for a headerless plan; test 7 passes
- [x] **M3** The refusal text of the other readers names the remedy; tests 8 and 12 pass
- [x] **M4** CLI and daemon routing pinned: tests 10 and 11 (CLI, with and without a named socket) and 13 (daemon) pass
- [ ] **M5** Docs staged for wrap (plan format, SKILL.md step 6, README); changeset Scope and `docs/dev/1-WIP/` note the todo removal at wrap
- [x] **M6** Scoped gate: `./test -p tddy-code-restructuring -p tddy-tools -p tddy-index-daemon`; clippy and fmt on the three; `plan/codec.rs` production lines <= 500 (`restructure check --budget 500` over the file, run once at the end)

## Testing plan

### Testing Strategy

**Primary Test Approach: library level, no server** — `runner::snapshot` / `runner::snapshot_resolving` against a temp workspace, in
milliseconds, in the style of `tests/snapshot_rewrites_the_header.rs`. The behaviour is a file transformation; nothing about it needs
rust-analyzer, and a live test would add tens of seconds to say nothing more. Two thin tests above it pin the routing facts the
design relies on (CLI process, daemon coordinate with fake language servers), because routing is where a tolerant parse would have
silently cost a cold index.

#### Option 1: Library (chosen)
**Test Level**: Integration (library public API, real filesystem)
**Scope**: header content, byte identity of operations, idempotence, refusals, `snapshot_resolving` without a client.
**Assertions**: exact `files` key set; each `sha256` equals `hash_file`; operation lines `==` the input lines; second run `rewritten == false` and file bytes equal; each refusal leaves the file bytes equal.
**Reliability**: no clock dependence (idempotence compares two runs on an unchanged tree); temp dirs; no network; no server.
**Implementation Location**: `packages/tddy-code-restructuring/tests/snapshot_writes_a_missing_header.rs`

#### Option 2: CLI child process (chosen, thin)
**Test Level**: E2E (the real `tddy-tools` binary, `assert_cmd`)
**Scope**: the whole command path including `TDDY_INDEX_SOCKET` set to an unreachable socket.
**Trade-offs**: **Pro** proves "never dials" end to end; **Con** slower than the library test (a process spawn each), so only three cases.
**Implementation Location**: `packages/tddy-tools/tests/restructure_cli_acceptance.rs`

#### Option 3: Daemon coordinate over fake language servers (chosen, thin)
**Test Level**: Integration
**Scope**: the registered `Snapshot` RPC for a headerless plan; `Workspaces` lists no held root.
**Implementation Location**: `packages/tddy-index-daemon/tests/code_index_service_acceptance.rs`

#### Option 4: live rust-analyzer fixture crate (rejected)
**Why not**: nothing here asks a language server anything. Adding a live binary would also need entries in `.config/nextest.toml`'s
`rust-analyzer` test group and `.config/rust-e2e.filterset` for no coverage gain. **No new live binary is added by this node.**

### Coverage Requirements

- [ ] **Happy path**: one-operation and two-operation headerless plans
- [ ] **Error scenarios**: missing file, file outside the workspace, no file named, first line neither header nor operation, an operation that does not parse
- [ ] **Edge cases**: leading blank lines; no trailing newline; an `also` anchor in another file; two operations anchoring the same file (one key)
- [ ] **Integration points**: routing (CLI, daemon)
- [ ] **Actual effects**: file bytes on disk, not return values alone

## Acceptance tests

Names read as behaviour specifications. Each is **red on `master` today** for the reason given.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/snapshot_writes_a_missing_header.rs` (new; library level, no server)

1. `writes_a_header_naming_every_file_the_operations_anchor` — two item-anchored operations over `src/lib.rs` and `src/other.rs`; line 1 parses as
   `{"v":2,"files":{…}}` naming exactly those two with `hash_file` digests; `rewrite.paths == 2`, `rewrite.rewritten`. *Fails today*: `Err(MalformedPlan("first line must be a snapshot header"))`.
2. `names_the_files_of_an_also_anchor_too` — one operation whose `also` anchors a third file. *Fails today*: same refusal.
3. `leaves_every_operation_line_byte_identical_and_in_order_and_keeps_leading_blank_lines` — two operations, two leading blank lines, no trailing newline in one variant. *Fails today*: same refusal.
4. `a_second_snapshot_of_the_plan_it_wrote_changes_nothing` — snapshot, then snapshot again on the unchanged tree; second `rewritten == false`, bytes equal. *Fails today*: the first call fails.
5. `refuses_an_operation_that_anchors_a_file_that_is_not_there_and_writes_nothing` — error names the path; plan bytes equal before and after. *Fails today*: refused for the wrong reason (no header), so the assertion on the path fails.
6. `refuses_an_anchor_outside_the_workspace_and_a_plan_naming_no_file` — absolute path, `../x.rs`, and an empty `file`; each refused, plan bytes equal. *Fails today*: wrong reason.
7. `snapshot_resolving_writes_the_header_with_no_language_server` — `runner::snapshot_resolving(root, options, None, CancellationToken::new())` on an item-anchored headerless plan returns `Ok` and writes the header. *Fails today*: `Err` raised by `read_plan` before the client question.
8. `every_other_reader_of_a_headerless_plan_is_told_to_run_snapshot` — `runner::check` (static) and `Plan::parse` refuse with a message containing `restructure snapshot`; the same plan with a garbage first line does **not** contain it. *Fails today*: the message carries no remedy.
9. `writes_a_snapshot_header_for_a_plan_that_anchors_by_range` (**present only if O1 is accepted as recommended**; if v2-always wins, it becomes `writes_a_v2_header_even_for_a_range_anchor` asserting `"v":2`) — a `range`-anchored operation; line 1 is `{"v":1,"snapshot":{…}}` and a later `check` of the plan refuses after the file is edited (`snapshot mismatch`). *Fails today*: no header is written.

### `tddy-tools` — `packages/tddy-tools/tests/restructure_cli_acceptance.rs` (existing file; `assert_cmd`, no rust-analyzer)

10. `restructure_snapshot_writes_the_header_a_plan_of_operations_lacks` — real binary, `TDDY_INDEX_SOCKET` removed, tempdir with `src/lib.rs` and a headerless plan: exit 0, line 1 is a header, operation lines unchanged. *Fails today*: exit 1, `first line must be a snapshot header`.
11. `restructure_snapshot_of_a_headerless_plan_does_not_dial_a_named_daemon` — `TDDY_INDEX_SOCKET=<a path nothing listens on>`: exit 0 and the header written (a dial would fail, a set-but-unreachable socket being an error). *Fails today*: same refusal.
12. `restructure_check_of_a_headerless_plan_names_snapshot_as_the_remedy` — exit non-zero, stderr contains `restructure snapshot`. *Fails today*: stderr says only `first line must be a snapshot header`.

### `tddy-index-daemon` — `packages/tddy-index-daemon/tests/code_index_service_acceptance.rs` (existing file, fake language servers)

13. `snapshots_a_headerless_plan_without_waiting_for_a_language_server` — beside `snapshots_a_plan_with_no_item_anchors_without_waiting_for_a_language_server` (`:1722`): a headerless plan over a symbol anchor (the file's `an_extraction_of` helper) through the registered `Snapshot` coordinate: `(paths, rewritten, stale) == (1, true, vec![])` and `Workspaces` lists no root. *Fails today*: `FailedPrecondition`/`InvalidArgument` from the malformed plan.

**Added beyond the list**: library test `names_a_file_once_when_two_operations_anchor_it` (the "two operations anchoring the same file (one key)"
edge case the Coverage list names), so the library file holds ten tests and the node fourteen. Test 2 uses `move_cluster_to_crate` because
`also` is refused on every other operation. **Recorded decisions**: O1 (recommended: v1 for a coordinate anchor, v2 otherwise), O2 (refuse naming
the remedy), O3 (new child module), O4 (no `plan new`), O5 (no re-resolve) are followed as recommended; test 9 is the v1 variant.
Test 9's final assertion (`snapshot mismatch` from a later `check`) is not yet reachable and is unverified until green writes the v1 header.

(Thirteen tests: nine at library level, three CLI, one daemon. The CLI and daemon cases are thin routing pins.)

## Technical Debt & Production Readiness

- **Boundary note (green, 2026-10-07).** The green commit also carries a one-hunk mechanical
  `rustfmt` reformat of `src/runner/compile_gate.rs`, a file outside this node's surface. The
  offending line is node 4's (`f10d2210`), and the base branch is `cargo fmt --all -- --check`-red
  because of it (verified: clean on `master`, red on `feature/sharpen/apply-heartbeat`). Keeping the
  reformat is what makes this branch fmt-clean for CI (`.github/workflows/ci.yml:74`). Zero
  behaviour; the change is byte-identical to what `cargo fmt` produces. Node 4 owns the same fix at
  its source — this is a carry, not a claim on that file.

## Decisions & Trade-offs

Decisions already taken by the developer, from the stack brief (quoted):

- "8-node decomposition approved (2026-10-05)." This is node 5 of 8.
- "Prep node: ADD the mechanical node first (the developer overrode the recommendation to decline)." That is `tidy-engine-files`, the only parent of this node.
- "Log-history fix is its own PR #586 — NOT in this stack."

**OPEN decisions** (not settled by the brief; each has a recommendation, and the developer decides):

- **O1 — which header version for a plan that anchors by `range` or `symbol`.**
  (a) v2 always — what the brief and the todo say ("the v2 form"). *Cost*: a v2 header never refuses a run, so a range or symbol anchor
  (coordinates, which do depend on the whole file) runs against a drifted file with only a progress line.
  (b) **v1 when any anchor is a coordinate, v2 when all are `item`/`items`** — *recommended*: the engine writes the header whose promise matches the anchors, so a plan
  that would have needed a hand-written v1 header still refuses on drift. ~6 extra lines.
  (c) refuse a headerless plan with coordinate anchors. Safest, but removes the helper exactly where the old hand-written header was needed.
- **O2 — what `check`, `apply`, `status`, `load` do with a headerless plan.**
  (a) **refuse, naming `restructure snapshot`** — *recommended*: one writer of the header, a plan file is never mutated by a reader, `Plan::parse` stays strict (routing).
  (b) read it as an in-memory v2 plan with no hints: `check --deep` then works with no extra step, but `apply`'s plan store writes the plan back (normalising it) and thereby adds a header the author never asked for; and `parse` is no longer strict.
  (c) tolerant for `check` only. Two behaviours for one file, and `check` passing a plan `apply` refuses is the defect `check_precondition_parity` exists to prevent.
- **O3 — where the code lives.** (a) **new child module `plan/codec/headerless.rs`** — *recommended* (the `signature_fields.rs` precedent; keeps `codec.rs` <= 500).
  (b) inline in `codec.rs` — rejected: pushes the file back over the budget `tidy-engine-files` just closed.
- **O4 — a `restructure plan new` / `anchors --plan` skeleton emitter** (the todo's other suggestion). **Recommendation: no.** A second way to author a header, and `anchors` is the
  command with an open code issue. If wanted it is its own node.
- **O5 — whether the first `snapshot` of a headerless item-anchored plan should also re-resolve the anchors** (as a headed one does). **Recommendation: no**: the anchors were just emitted
  against this tree, the re-resolution costs a cold index, and it would make a headerless plan's first snapshot route through a server, which the routing tests forbid.

Decisions taken by this plan (a reviewer can check them):

- The header goes in through `hint_of` (honouring the open `FileHint.modified` issue: no second writer).
- `Plan::parse` stays strict; `snapshot` alone reads tolerantly.
- Refusals happen before any write, so a refused `snapshot` leaves the plan byte-identical.
- The refusal remedy is added only when line 1 **is** an operation, so a corrupt first line is not told to run a command that cannot help.

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

(empty; populated by `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`)

## References

- Stack brief and whole-work discovery: `docs/dev/1-WIP/2026-10-05-engine-fixes-whole-work-initial-discovery.md` on the planning branch (copied in full into this node's companion as Exploration 1)
- Exemplar for the code-structure precedent: `plan/codec/signature_fields.rs`

## TODO

- [x] Record initial discovery (`2026-10-05-sharpen-plan-header-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-05-sharpen-plan-header.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning, eight nodes would conflict)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring -p tddy-tools -p tddy-index-daemon`) — verify 100% pass; CI answers for the rest of the workspace
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
- [ ] Linting and formatting (`cargo clippy -p <touched> --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-05-sharpen-plan-header-initial-discovery.md`, and deletes `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md` if it has reached `master` (else whichever of #532 and this PR lands second deletes it)
- [ ] USER REVIEW — work complete, decide next steps
