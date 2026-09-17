# Changeset: Restructure refusal truth and authoring gates

**Date**: 2026-09-17
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-17-restructure-refusal-truth-and-authoring-gates-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  two new error variants and the constructors that route to them; a third disambiguation tier in the
  import pass; `restore_visibility` fed from the produced text; a partial-relocation guard; the
  `snapshot` subcommand
- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — `status_of`
  classifies the two new variants
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — `apply` gains the verdict
  step `check` and `verify` already have
- **Repo root**: `run-index-daemon` — detach into its own session; `--status` dials instead of
  signalling
- **Skill**: `.agents/skills/code-restructuring/SKILL.md` and its
  `references/plan-schema.md` — the authoring gates

## Related Feature Documentation

- [PRD-2026-09-17-restructure-refusal-truth-and-authoring-gates.md](../../ft/coder/1-WIP/PRD-2026-09-17-restructure-refusal-truth-and-authoring-gates.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)
- [Warm code-intelligence daemon](../../ft/coder/warm-code-intelligence-daemon.md)

## Summary

`tddy-tools restructure` reports every failure as `plan is malformed:`, including the ones where the
plan is fine. Underneath that, two defects make correct plans fail: the import pass cannot recognise
a re-exported path as the one a file already imports, and `restore_visibility` narrows items back to
private on a survey the assist has already invalidated. This changeset splits the refusal taxonomy
into classes a caller acts on, fixes both defects, makes the warm daemon survive the shell that
started it, and rewrites the authoring workflow so its rehearsal gates read as the path rather than
as options.

## Background

A live `extract_module` spike against the `#carve` node 2 planning seam in
`packages/tddy-workflow-recipes/src/parser.rs` failed twice at ~20 minutes of indexing per attempt,
then produced source that did not compile. The handoff written afterwards catalogued eight failures;
verifying it against the tree found three misdiagnosed, two describing gates that already ship and
went unused, and the two real defects unnamed.

Both real defects were diagnosed from primary evidence rather than from the handoff's prose — the
apply log at `/tmp/restructure-apply-visible.log:397` for the import refusal, and the apply journal
at `it3z/.restructure/journal.jsonl` for the `E0603`. The journal was decisive: it holds
rust-analyzer's exact edit, showing an assist that relocated lines 10–116 of an anchored 10–152 and
rewrote the remainder in place to reach into the module it had just written.

The taxonomy complaint is already on the record twice —
`packages/tddy-index-daemon/src/status.rs:1-8` exists to prevent exactly it at the transport
boundary, and `#unbundle` node 6 filed it as a defect. It has never been fixed at the source.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⚠ DURING — Restructure defects found by the first real cross-crate move — [`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)

The `#unbundle` node 6 section states this changeset's Milestone 1 as a defect: *"`plan is malformed`
is the wrong error class. The plan was not malformed; the indexer did not settle. … the advice 'fix
your plan' is actively misleading."*

**Not resolved here, and the entry is not deleted.** The specific case it names — an indexing timeout
reported as a malformed plan — was already fixed by the `ServerNotSettled` variant; what this change
fixes is the same mistake for the other fifty-six refusals. The entry's remaining bullets are
untouched and all still live: the repo-scoped journal, the self-dependency in the destination
manifest, the facade-cycle false positive, `git mv` on an unstaged file, inconsistent caller
re-pointing, and the `check`/`apply` disagreement. Milestone 1 must **edit** this entry to strike the
error-class bullet and say which change closed it, leaving the rest.

The entry also records a budget overrun *exiting zero*. That is stale: `--indexing-budget` no longer
exists, and the exit-code claim in this change's own source document turned out to be a shell-wrapper
artifact. Milestone 1 strikes that bullet too.

### ℹ ANSWERED — Restructure defects found by the `connection_service.rs` split — [`2026-09-09-restructure-defects-from-the-connection-service-split.md`](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md)

This entry closes with *"the remaining obstacles are all in the import-restoration pass, not in the
assist or the anchors"*, and its **D8** is the same shape as Milestone 2: rust-analyzer offers a path
the file does not use — there, the unaliased path for an aliased import; here, the re-exported root
path for a canonically-imported item. D8's fix (`alias_target`, reconstructing from the parent's own
declaration) is the precedent Milestone 2 follows.

The entry is marked resolved for D6–D9 and is kept as the record of how each presented. **Not deleted
here.** Milestone 2 appends a **D10** section in the same voice, so the next person meets the
re-export case where they will look for it.

### ⚠ DURING — `backends/rust.rs` is 4,571 production lines — [`2026-09-16-backends-rust-rs-is-4500-production-lines.md`](../todo/2026-09-16-backends-rust-rs-is-4500-production-lines.md)

Every Rust change in this changeset lands in that file, and the entry's own table names the
`2500–4571` helper tail — where `choose_import` lives — as the cheapest and first thing to carve out.

**Constraint on how this work is done, not a blocker.** Milestones 1–3 add roughly one chooser tier,
one guard and two error constructors; they must not grow the helper tail beyond that, must not move
anything in it, and must not restructure the file. Doing the carve here would bury a reviewable diff
under a mechanical one — the trade the planning skill names explicitly, and the same reason the entry
gives for deferring.

The entry also observes that the carve wants a warm index and that `./run-index-daemon` now provides
one. **Milestone 5 makes that claim true in practice**: a daemon that dies with the shell that
started it is not a warm index anyone can carve against. Recorded here so the connection is not lost
— the entry stays open, and this change removes one obstacle in front of it.

### ⚠ DURING — `IndexDaemonRegistry::connect` is public API ahead of its caller — [`2026-09-16-indexdaemonregistry-connect-has-no-caller.md`](../todo/2026-09-16-indexdaemonregistry-connect-has-no-caller.md)

Milestone 5 makes `run-index-daemon --status` dial the service. That is a *script* dialling a socket,
not `tddy-daemon`'s registry, so it neither closes this entry nor gives it the acceptance test it
wants. Recorded so the next reader does not mistake one for the other: the entry explicitly says
`tddy-tools` is not the consumer that closes it, and a shell script is not either.

### — UNRELATED (considered, not recorded further)

`2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md` and
`2026-09-09-macro-expansion-as-a-restructure-operation.md` concern `move_module_to_crate` and a new
operation. This change touches neither.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **Refusal taxonomy**: `SeamRefused` and `ServerDefect`, all 56 call sites routed (10 seam / 24 server / 22 plan+transport), `status_of` extended ✅
- [x] **Import disambiguation**: a re-exported candidate recognised as the file's own binding ✅
- [x] **Visibility decided on produced text**, plus the partial-relocation guard ✅
- [x] **`restructure snapshot`**: the header rewritten from the working tree ✅
- [x] **Daemon operability**: detached session, `--ping` on the binary, dialling `--status` ✅
- [x] **Authoring gates**: skill, plan-schema reference, feature doc, and the daemon lines ✅
- [x] **Backlog hygiene**: both TODO entries edited, D10 appended, line count recorded ✅
- [ ] **Testing**: all acceptance tests passing
- [ ] **Code Quality**: `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`

## Technical Changes

### State A (Current)

**Error classes.** `RestructureError` has fourteen variants. `failure()`
(`backends/rust.rs:4568`) is the single constructor every `RustBackend` refusal goes through, and it
returns `MalformedPlan`, whose `Display` is `"plan is malformed: {0}"`. Fifty-six call sites use it.
They divide into four families — genuinely malformed plan, seam refused, unusable server answer,
transport — and every one of them tells the operator to edit their plan. `status_of`
(`tddy-index-daemon/src/status.rs:21`) maps `MalformedPlan` to `InvalidArgument`, propagating the
wrong class to every transport.

**Import restoration.** `choose_import` (`:2824`) picks among the paths rust-analyzer offers for an
unresolved name. Tier 1 keeps a candidate whose **full path string** appears in the file's own
imports; tier 2 keeps one whose **parent module** is a module the file already imports from;
otherwise it returns `None` and `next_import` refuses. A crate that re-exports an item at its root —
`tddy_core::ParseError` for `tddy_core::error::ParseError` — defeats both tiers, because the strings
differ and `tddy_core` is not `tddy_core::error`.

**Visibility.** `survey_moved_items` (`:1424`) runs on the **original** text over the **requested**
range and sets `MovedItem::reached_from_outside`. The assist then runs. `restore_visibility`
(`:4351`) narrows every moved item the assist widened back to the visibility it was written with,
unless `reached_from_outside`. When the assist relocates only part of the anchored range and rewrites
the remainder to reach into the new module, the survey's answer is stale and the narrowing is wrong.
`impl_widenings` (`:3797`) already reads the produced text — but only to build the report.

**Subcommands.** `apply`, `status`, `check`, `anchors`, `verify`. A plan's `sha256:` header must be
recomputed by hand after every edit to a snapshotted file; `hash_file` (`apply.rs:66`) is public but
nothing exposes it.

**Verdicts.** `restructure_cli::report` (`:94`) and the warm path's `verdict_on_findings` both turn
findings into a non-zero exit. The warm `apply` (`index_client.rs:121-140`) drains its event stream
and returns `Ok(())`; its exit status depends entirely on the stream erroring.

**The daemon script.** `run-index-daemon:173` starts the binary with `nohup … &` from the script's
own shell — protected from `SIGHUP`, still in the caller's process group. `--status` (`:105-113`)
reports a live daemon on `kill -0` plus the socket file existing.

**The workflow an author is given.** `SKILL.md` step 7 is
``check plan.jsonl [--deep]``; step 8 is "`--dry-run` then apply". Nothing says a plain `check` cannot
see an assist refusal, nothing points at `anchors --items` for authoring a range, and step 5 asks for
`sha256:` hashes without saying how to get them.

### State B (Target)

**Error classes.** Sixteen variants. `SeamRefused` — the plan is well formed and the code will not
permit this cut — and `ServerDefect` — rust-analyzer's answer was unusable. Three constructors
(`failure`, `seam_refusal`, `server_defect`) with the call sites routed by family. Every refusal
*message* is unchanged; the sentence in front of it is not. `status_of` maps `SeamRefused` to
`FailedPrecondition` and `ServerDefect` to `Internal`.

**Import restoration.** A third tier, tried only when tiers 1 and 2 both decline: keep the candidate
whose **crate root** matches the crate root of an in-scope binding **of that same name**. Decisive
for `tddy_core::ParseError` against `std::string::ParseError` and `chrono::ParseError`; still a
refusal when two candidates are rooted in crates the file binds that name from.

**Visibility.** `restore_visibility` takes the produced text as its evidence: an item the produced
parent still reaches through `module::Item` keeps its widening whatever the pre-assist survey said.
An item nothing outside reaches after the assist is still narrowed, unchanged. Beside it, a guard:
when the assist relocates materially less than the anchor asked for, the run refuses as a
`SeamRefused` naming the lines left behind and pointing at `anchors --items`.

**Subcommands.** Six. `restructure snapshot <plan.jsonl>` re-hashes every path the header names and
rewrites line 1. No LSP client, no index.

**Verdicts.** `apply` carries a verdict of its own on both paths.

**The daemon script.** The daemon is started in its own session, so the shell that launched it can
exit without taking it. `--status` opens a gRPC channel and issues `Workspaces`; a socket that will
not answer is reported as not answering, with no fall back to the pid check.

**The workflow.** `check --deep` is the seam-proving step with one line saying why. `anchors --items`
is how a range is authored. `snapshot` is named where the workflow says to snapshot. A short section
records what the import pass can and cannot restore, including the re-export case and D8's aliases.

### Delta (What's Changing)

#### tddy-code-restructuring

- **API**: `RestructureError::{SeamRefused, ServerDefect}`; `RestructureCommand::Snapshot` and
  `runner::Command::Snapshot`
- **Implementation**: `seam_refusal()` / `server_defect()` beside `failure()`; ~30 of the 56 call
  sites re-routed; `choose_import` third tier; `restore_visibility` signature takes the produced
  text; a `refuse_partial_relocation` guard; `runner` arm for `Snapshot`
- **Behaviour**: two classes of correct plan that fail today will succeed — one blocked by a
  re-exported import path, one that currently "succeeds" into non-compiling source

#### tddy-index-daemon

- **Implementation**: two arms in `status_of`. The exhaustive match makes omission a compile error,
  which is the entire reason that function is written the way it is.

#### tddy-tools

- **Implementation**: `apply` ends on a verdict rather than `Ok(())`

#### Repo root

- **`run-index-daemon`**: `setsid`-equivalent detachment; `--status` dials `Workspaces`

#### Skill and feature docs

- **`.agents/skills/code-restructuring/SKILL.md`**: steps 5, 7 and 8 rewritten; `snapshot` in the CLI
  block
- **`references/plan-schema.md`**: the import-restoration section gains the re-export case
- **`docs/ft/coder/rust-code-restructuring.md`**: refusal classes, the `snapshot` row, the import
  section, and the § Known limitations entries these close

## Implementation Milestones

- [x] **M1 — Refusal taxonomy.** Add the two variants; add `seam_refusal()` and `server_defect()`;
  route the call sites family by family; extend `status_of`; edit the two TODO entries
  `## Prerequisites` commits to.
- [x] **M2 — Import disambiguation.** Third tier in `choose_import`; append D10 to the
  connection-service-split entry.
- [x] **M3 — Visibility from produced text.** Feed `restore_visibility` the produced parent; add
  `refuse_partial_relocation`.
- [x] **M4 — `restructure snapshot`.** Args, runner command, front-end rendering.
- [x] **M5 — Daemon operability.** Detached session (`setsid`, pid written from inside the daemon); `tddy-index-daemon --ping`; `--status` dials and exits non-zero when nothing answers.
- [x] **M6 — Authoring gates.** Skill, plan-schema reference, feature doc: `anchors --items` as step 5, `snapshot` as step 6, `--deep` as the gate at step 8, the refusal-class table, the crate tier in the import table, two new § Known limitations entries, and the detach/`--ping` lines in the warm-daemon doc and AGENTS.md.

M1 lands first because M3's guard is a `SeamRefused` and would otherwise be written against a class
that does not exist yet. M2 and M4 are independent. M6 documents M2–M5 and lands last.

## Testing Plan

### Testing Strategy

**Primary test approach: unit and integration, with no acceptance test that spawns rust-analyzer.**

The changeset is four small behavioural changes in pure functions plus one script change. The
decisive question for each is what a function returns given a text, not what a workflow does
end to end:

- `choose_import` is a pure function over `(&str, &[&str])`. Its correct behaviour is fully
  specified by the spike's real candidate list.
- `restore_visibility` is a pure function over `(text, module, items)`. The bug is entirely in which
  text it is given.
- The refusal classes are a property of a constructor and of `status_of`, both pure.
- `restructure snapshot` is file-in/file-out over a temp worktree.

**Why not an end-to-end `extract_module` acceptance test.** A real assist needs a rust-analyzer
spawn against this workspace: six to ten minutes cold, and
`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md` already records the cost of
proving a refusal against a live server as the reason it was not done. The existing suites draw the
line in the same place — `move_module_to_crate_acceptance.rs` and `library_returns_its_results.rs`
exercise the static tier and the value contracts, and `warm_index_production.rs` is the one
`#[ignore]`d production suite that pays for a server. This change adds nothing to that suite.

**What replaces it.** The journal that proved the defect is the fixture: `restore_visibility`'s test
is handed the exact text rust-analyzer produced for the parser seam, so the test fails today for the
reason the spike failed, and passes only when the narrowing reads the produced parent.

### Testing Options Analysis

#### Option 1 — Unit tests against real recorded server output (chosen)

**Test level**: Unit, in `#[cfg(test)] mod tests` beside each function.
**Description**: Drive each changed function with the text and candidate list taken verbatim from the
spike's own artefacts.

**Scope**:
- `choose_import` with `["Import \`tddy_core::ParseError\`", "Import \`std::string::ParseError\`", "Import \`chrono::ParseError\`"]` against a file importing `tddy_core::error::ParseError`
- The same three candidates against a file importing the name from two crates — must still refuse
- `restore_visibility` over a produced parent that reaches `planning::StructuredPlan`
- `restore_visibility` over a produced parent that reaches nothing in the module — must still narrow
- Each refusal constructor's variant, and `status_of`'s code for it

**Assertions**:
- [ ] `choose_import` returns `Some("Import \`tddy_core::ParseError\`")` — the exact title, not "some candidate"
- [ ] `choose_import` returns `None` for the two-crate case
- [ ] The produced module declares `pub(crate) struct StructuredPlan` — the widening kept, asserted on the text
- [ ] The produced module declares `struct StructuredPlan` with no widening when nothing outside reaches it
- [ ] `seam_refusal(...)` is `RestructureError::SeamRefused`, and its `Display` does not contain `"plan is malformed"`
- [ ] `status_of(&SeamRefused(...)).code() == Code::FailedPrecondition`

**Reliability**: pure functions over literal strings. No server, no temp dirs, no timing. Same result
every run.

**Implementation location**: `packages/tddy-code-restructuring/src/backends/rust.rs` (`#[cfg(test)]`),
`packages/tddy-index-daemon/src/status.rs` (`#[cfg(test)]`).

#### Option 2 — Integration tests over the runner and the CLI surface

**Description**: The two changes that are not pure functions — the `snapshot` subcommand and the
partial-relocation guard — need a plan and a worktree.

**Scope**: a temp git worktree holding one source file and a plan whose header is stale; a plan whose
anchor exceeds what any assist would move.

**Assertions**:
- [ ] After `snapshot`, the header's hash equals `hash_file` of the file on disk
- [ ] The operations after line 1 are byte-identical — the subcommand rewrites the header and nothing else
- [ ] A second `snapshot` on an unchanged tree leaves the file byte-identical
- [ ] `RestructureCommand::Snapshot` needs no LSP client (`needs_lsp_client` is false)

**Trade-offs**: **Pros** — covers the subcommand at the level a user meets it, reusing
`a_workspace_holding` / `a_plan_under` from `library_returns_its_results.rs`. **Cons** — cannot cover
the guard without an assist, so the guard is tested at the function level in Option 1.

**Implementation location**: `packages/tddy-code-restructuring/tests/snapshot_rewrites_the_header.rs`.

#### Option 3 — Shell-level tests for `run-index-daemon`

**Description**: Verify detachment and the dialling `--status`.

**Scope**: start a daemon from a subshell, let the subshell exit, confirm the daemon still answers;
point `--status` at a socket file with nothing behind it.

**Assertions**:
- [ ] The daemon answers `Workspaces` after the shell that started it has exited
- [ ] `--status` exits non-zero on a socket with no listener, although the pid file names a live process

**Trade-offs**: **Pros** — the only level at which the defect exists; a unit test cannot express
"survives its parent". **Cons** — starting a real daemon means a rust-analyzer-capable dev shell and
a build. Marked `#[ignore]` and run via `./vm-tests`-style opt-in, matching how this repo already
treats tests that spawn servers.

**Note**: Options 1 and 2 are the gate. Option 3 is opt-in and does not run in `./test`.

### Testing Principles Applied

**✅ Appropriate test level** — unit for pure functions, integration for the file-in/file-out
subcommand, opt-in production for the one thing only a real process can show. No E2E, because no
user-facing workflow changes shape.

**✅ Strong assertions** — the exact import title, the exact visibility keyword in the produced text,
the exact `Code`. Not "is an error" or "contains something".

**✅ Deterministic** — the two central tests are pure functions over literals lifted from a recorded
run.

**✅ Complete scope** — each behavioural change has both its positive case and the case that must
still refuse or still narrow. The narrowing test matters most: a fix that simply stopped narrowing
would pass the positive case and silently undo D9.

**❌ Anti-patterns avoided** — no rust-analyzer spawn for a decision made before the server is
consulted; no assertion that a run "succeeded"; no mocked LSP client standing in for the assist,
which would only pin the mock.

### Coverage Requirements

- [ ] **Happy path**: the re-exported candidate is chosen; the widening is kept; the header is rewritten
- [ ] **Error scenarios**: two-crate ambiguity still refuses; a partial relocation refuses
- [ ] **Edge cases**: nothing outside reaches the item (still narrowed); an unchanged plan (no-op snapshot)
- [ ] **Integration points**: `status_of` for both new variants; `needs_lsp_client` for `Snapshot`
- [ ] **Actual effects verification**: assertions read the produced text and the rewritten file, not return codes
- [ ] **Side effects**: `snapshot` rewrites line 1 and nothing else

### Test Data Strategy

Fixtures are lifted verbatim from the spike's artefacts — the candidate list from
`/tmp/restructure-apply-visible.log:397` and the produced parent/child text from
`it3z/.restructure/journal.jsonl`. Copied into the test files as `const` literals, not read from
those paths: the worktree they live in is not part of this repo, and a test that read it would pass
only on one machine. Each test builds its own temp worktree where it needs one.

## Acceptance Tests

### tddy-code-restructuring — written, failing (M1–M2, M4 boundary)

- [ ] **Unit**: `settles_a_contested_name_on_the_crate_the_file_already_binds_it_from` (`src/backends/rust.rs`) — ❌ `None`, wants `tddy_core::ParseError`
- [ ] **Unit**: `keys_the_crate_tier_on_a_binding_of_the_contested_name` (`src/backends/rust.rs`) — ❌ `None`
- [ ] **Unit**: `keeps_the_widening_of_an_item_the_produced_parent_reaches_through_the_module` (`src/backends/rust.rs`) — ❌ narrowed to private
- [ ] **Unit**: `a_seam_refusal_does_not_tell_the_author_their_plan_is_malformed` (`src/backends/rust.rs`) — ❌ `MalformedPlan`
- [ ] **Unit**: `a_defect_in_the_servers_answer_is_not_reported_as_a_defect_in_the_plan` (`src/backends/rust.rs`) — ❌ `MalformedPlan`
- [ ] **Unit**: `accepts_a_snapshot_of_the_plan_whose_header_it_rewrites` (`src/restructure_args.rs`) — ❌ unknown subcommand

### tddy-code-restructuring — written, passing (guards that must keep passing)

- [x] **Unit**: `settles_nothing_when_two_candidates_share_the_crate_the_name_is_bound_from` (`src/backends/rust.rs`)
- [x] **Unit**: `narrows_an_item_the_produced_parent_does_not_reach_through_the_module` (`src/backends/rust.rs`)
- [x] **Unit**: `reads_no_outside_reach_from_a_mention_inside_the_module_itself` (`src/backends/rust.rs`)
- [x] **Unit**: `a_plan_that_does_not_say_enough_is_still_reported_as_a_malformed_plan` (`src/backends/rust.rs`)

### tddy-index-daemon — written, passing (classification declared with the variants)

- [x] **Unit**: `reports_a_refused_seam_as_a_failed_precondition` (`src/status.rs`)
- [x] **Unit**: `reports_an_unusable_answer_from_the_server_as_internal` (`src/status.rs`)

### Deferred to their own milestone's red step

These define API that does not exist, so writing them now would produce a compile failure rather
than a behavioural red — the one kind of red `.agents/commands/red.md` rule 3 forbids. Each is
written at the head of its milestone, against the surface that milestone introduces.

- [x] **M3** `refuses_an_assist_that_left_an_anchored_item_behind`, `accepts_a_relocation_that_carried_every_anchored_item`, `finds_an_anchored_item_the_assist_nested_inside_an_inline_module` (`src/backends/rust.rs`) ✅
- [x] **M4** `rewrites_a_stale_header_to_match_the_working_tree` (`tests/snapshot_rewrites_the_header.rs`) ✅
- [x] **M4** `leaves_every_operation_line_byte_identical` + `follows_the_file_when_it_changes_under_a_header_that_pinned_it` ✅
- [x] **M4** `is_a_no_op_on_a_plan_whose_header_already_matches` ✅
- [x] **M4** `a_snapshot_needs_no_language_server` (`src/restructure_cli.rs`) ✅
- [x] **M1** `an_apply_that_performed_no_operation_is_a_failed_run` + `an_apply_whose_stream_ended_without_an_outcome_is_a_failed_run` and three guards (`tddy-tools/src/index_console.rs`); the same judgement pinned on the cold path in `src/restructure_cli.rs` ✅
- [x] **M5** `the_daemon_outlives_the_shell_that_started_it` + `status_refuses_a_socket_with_no_listener_behind_it` (`tddy-index-daemon/tests/detached_daemon_production.rs`, `#[ignore]`d, 47s) ✅
- [x] **M5** `refuses_a_socket_path_that_nothing_is_listening_on`, `refuses_a_socket_path_that_does_not_exist`, `answers_a_daemon_that_is_serving_the_socket` (`tddy-index-daemon/tests/ping_answers_only_a_live_listener.rs`) ✅

## Technical Debt & Production Readiness

*Populated during development.*

- [x] `backends/rust.rs` grows again — recorded as 4,571 → 4,757 (+186) against
  [`2026-09-16-backends-rust-rs-is-4500-production-lines.md`](../todo/2026-09-16-backends-rust-rs-is-4500-production-lines.md)
  before wrap, and record the new figure in that entry

## Decisions & Trade-offs

- **Two new variants, not four.** Transport failures keep `Io` and `ServerCatchingUp` beside them; a
  fourth class nobody routes differently is ceremony. Revisit if a caller ever needs to distinguish
  "the server died" from "the server answered nonsense".
- **The crate-root tier is third, not first.** It is strictly weaker evidence than an exact path
  match and must never outrank one. The existing verify loop still rejects whatever it picks if the
  import fails to reduce the name's unresolved occurrences, so a wrong guess costs a refusal rather
  than bad source.
- **`restore_visibility` is fixed rather than a preflight added.** The PRD originally proposed a
  preflight refusing leftover references. Exploration 3 ruled that out: the leftover references do
  not exist until the assist has run, so nothing before the server could see them.
- **The partial-relocation guard refuses rather than warns.** A run that relocates less than it was
  asked to has produced something the author did not describe, and the existing refusals in this
  backend all take the same line — refusing beats reporting success over source that will not build.
- **No end-to-end assist test.** Stated in the Testing Plan with its reasoning; recorded here because
  it is the one place a reviewer may reasonably disagree, and the trade is deliberate.
- **One PR, not a stack.** M1 and M3 share the refusal constructors and M6 documents M2–M5; split
  nodes could not be reviewed apart. The cost is a PR touching four packages and two doc surfaces.

## Refactoring Needed

### From `/red` (TDD Red Phase)

*Populated during the red phase.*

### From `/validate-changes` (Change Validation)

*Populated during validation.*

### From `/validate-tests` (Test Quality)

*Populated during validation.*

### From `/validate-prod-ready` (Production Readiness)

*Populated during validation.*

### From `/analyze-clean-code` (Code Quality)

*Populated during validation.*

### From `refactor` (Completed Refactorings)

*Populated as refactorings land.*

## Validation Results

### Change Validation (`/validate-changes`)

*Not yet run.*

### Test Validation (`/validate-tests`)

*Not yet run.*

### Production Readiness (`/validate-prod-ready`)

*Not yet run.*

### Code Quality (`/analyze-clean-code`)

*Not yet run.*

## TODO

- [x] Record initial discovery (`2026-09-17-restructure-refusal-truth-and-authoring-gates-initial-discovery.md`)
- [x] Cross-check `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — a CI job, not a local one; push and read `scripts/ci-status.sh`
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
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `{slug}-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
